use super::*;

#[test]
fn held_queue_expires_after_250_ms() {
    let (mut active, script, handle) = setup(LIMITS, 1);
    let mut held = with_transport(LIMITS, &handle);
    submit(&mut active, 1, 0);
    drop(active.take_with_time(&live()));
    submit(&mut held, 2, 0);
    drop(held.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    submit(&mut active, 3, 0);

    for now_ms in [0, 16, 249] {
        script.state.lock().unwrap().now_ms = now_ms;
        drop(held.take_with_time(&time(false, 0)));
        drop(active.take_with_time(&live()));
        let posted = script.state.lock().unwrap().postings.len();
        assert_eq!(posted, 1, "at {now_ms} ms");
    }
    for (index, now_ms, value) in [(1, 250, 3), (2, 266, 4), (3, 282, 5)] {
        script.state.lock().unwrap().now_ms = now_ms;
        drop(active.take_with_time(&live()));
        let (_, attempts) = script.posting(index);
        assert_eq!(script.posted_values(&attempts), [value]);
        script.complete(attempts[0]);
        submit(&mut active, value + 1, 0);
    }
}

#[test]
fn competing_queues_alternate_with_one_or_two_takes() {
    for takes in [1, 2] {
        let (mut first, script, handle) = setup(LIMITS, 1);
        let mut second = with_transport(LIMITS, &handle);
        let mut owners = Vec::new();
        for frame in 0..6 {
            let value = 2 * frame + 1;
            submit(&mut first, value, 0);
            submit(&mut second, value + 1, 0);
            let time = live();
            script.state.lock().unwrap().now_ms = u64::from(frame) * 16;
            for _ in 0..takes {
                drop(first.take_with_time(&time));
            }
            drop(second.take_with_time(&time));
            let posted = script.state.lock().unwrap().postings.len();
            assert_eq!(posted, frame as usize + 1);
            let (owner, attempts) = script.posting(frame as usize);
            owners.push(owner);
            assert_eq!(
                owner,
                owners[frame as usize % 2],
                "first queue took {takes} time(s)"
            );
            if frame == 1 {
                assert_ne!(owners[0], owners[1], "first queue took {takes} time(s)");
            }
            for attempt in attempts {
                script.complete(attempt);
            }
        }
    }
}

#[test]
fn a_queue_with_no_late_unit_yields_the_worker_to_another_queue() {
    let (mut first, script, handle) = setup(LIMITS, 1);
    let mut second = with_transport(LIMITS, &handle);
    submit(&mut first, 1, 0);
    for value in 2..=4 {
        let mut take = first.take_with_time(&late());
        take.mark_late_unit_spent();
        drop(take);
        submit(&mut second, value, 0);
        drop(second.take_with_time(&live()));
        let (_, attempts) = script.posting((value - 2) as usize);
        assert_eq!(script.posted_values(&attempts), [value]);
        script.complete(attempts[0]);
        assert_eq!(results(&mut second, live()), [value]);
    }
    assert_eq!(first.pending(), 1);
}

#[test]
fn running_limit_leaves_a_second_worker_for_another_queue() {
    let (mut first, script, handle) = setup(LIMITS, 2);
    let mut second = with_transport(LIMITS, &handle);
    submit(&mut first, 1, 0);
    submit(&mut first, 2, 0);
    drop(first.take_with_time(&late()));
    submit(&mut second, 3, 0);
    drop(second.take_with_time(&live()));
    assert_eq!(script.posted_values(&script.posting(1).1), [3]);
    assert_eq!(first.report().running, 1);
}

#[test]
fn late_post_blocks_delivery_in_the_same_prepare() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    submit(&mut jobs, 2, 0);

    let frame = late();
    drop(jobs.take_with_time(&frame));
    assert_eq!(script.posted_values(&script.posting(1).1), [2]);
    assert_eq!(
        jobs.take_with_time(&frame).next().map(|done| done.result),
        None
    );
    assert_eq!(results(&mut jobs, late()), [1]);
}

#[test]
fn a_late_encoding_stays_spent_after_a_throw_ends_the_lane() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    {
        let mut state = script.state.lock().unwrap();
        state.refuse_post = true;
        state.fail_replacement = true;
    }
    let frame = late();
    drop(jobs.take_with_time(&frame));
    let mut fallback = jobs.take_with_time(&frame);
    assert_eq!(fallback.lane(), Lane::Inline);
    assert!(fallback.next().is_none());
    drop(fallback);
    assert_eq!(results(&mut jobs, late()), [1]);
}

#[test]
fn mark_blocks_late_delivery_and_posting() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    submit(&mut jobs, 2, 0);

    let frame = late();
    let mut take = jobs.take_with_time(&frame);
    take.mark_late_unit_spent();
    assert_eq!(take.next().map(|done| done.result), None);
    drop(take);
    assert_eq!(
        jobs.take_with_time(&frame).next().map(|done| done.result),
        None
    );
    let posted = script.state.lock().unwrap().postings.len();
    assert_eq!(posted, 1);
    assert_eq!(results(&mut jobs, late()), [1]);
}

#[test]
fn traps_replace_workers_without_ending_the_lane() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    for index in 0..3 {
        submit(&mut jobs, index as u8, 0);
        drop(jobs.take_with_time(&live()));
        let mut state = script.state.lock().unwrap();
        let worker = state.next_worker;
        let attempt = state.postings[index].1[0];
        state.answer(attempt, Outcome::Trapped, Vec::new());
        drop(state);
        let mut take = jobs.take_with_time(&live());
        assert_eq!(take.lane(), Lane::Worker);
        assert_eq!(
            take.next().unwrap().result,
            Err(JobError("job trapped".into()))
        );
        drop(take);
        let workers = script.state.lock().unwrap().workers.clone();
        assert!(!workers.contains(&worker));
        assert_eq!(workers.len(), 1);
    }
    assert_eq!(jobs.report().failed, 3);
}

#[test]
fn three_rejected_answers_end_the_lane() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    for index in 0..3 {
        let key = index as u8 + 1;
        submit(&mut jobs, key, 0);
        drop(jobs.take_with_time(&live()));
        let attempt = script.posting(index).1[0];
        script
            .state
            .lock()
            .unwrap()
            .answer(attempt, Outcome::Bytes(1), vec![0]);
        assert_eq!(results(&mut jobs, live()), [key]);
    }
    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.lane(), Lane::Inline);
    assert!(take.next().is_none());
}

#[test]
fn only_accepted_answers_break_a_loss_streak() {
    for value in [1, 255, 3] {
        let (mut jobs, script, _) = setup(LIMITS, 1);
        submit(&mut jobs, value, 0);
        for posting in 0..2 {
            drop(jobs.take_with_time(&live()));
            script.lose_batch_with(script.posting(posting).1[0]);
        }
        drop(jobs.take_with_time(&live()));
        let attempt = script.posting(2).1[0];
        if value == 3 {
            script
                .state
                .lock()
                .unwrap()
                .answer(attempt, Outcome::Trapped, Vec::new());
        } else {
            script.complete(attempt);
        }
        let losses = script.state.lock().unwrap().losses;
        assert_eq!(losses, 2, "before accepting {value}");
        let mut take = jobs.take_with_time(&live());
        let expected = match value {
            255 => Err(JobError("missing required field".into())),
            3 => Err(JobError("job trapped".into())),
            _ => Ok(value),
        };
        assert_eq!(take.next().unwrap().result, expected);
        drop(take);
        let losses = script.state.lock().unwrap().losses;
        assert_eq!(losses, 0, "after accepting {value}");
        assert_eq!(jobs.report().lane, Lane::Worker);
    }
}

#[test]
fn early_delivery_leaves_a_late_unit() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    for value in [1, 2, 3] {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    for attempt in script.posting(0).1 {
        script.complete(attempt);
    }
    let early = live();
    assert_eq!(jobs.take_with_time(&early).next().unwrap().result, Ok(1));
    let expired = HookTime {
        serial: early.serial,
        ..late()
    };
    assert_eq!(results(&mut jobs, expired), [2]);
    assert_eq!(results(&mut jobs, late()), [3]);
}

#[test]
fn stale_counts_only_answers_known_to_the_queue() {
    for learned in [false, true] {
        let (mut jobs, script, _) = setup(LIMITS, 1);
        submit(&mut jobs, 1, 0);
        drop(jobs.take_with_time(&live()));
        script.complete(script.posting(0).1[0]);
        if learned {
            drop(jobs.take_with_time(&live()));
        }
        jobs.cancel(1);
        assert_eq!(jobs.report().stale, u64::from(learned));
        assert!(results(&mut jobs, live()).is_empty());
    }
}

#[cfg(debug_assertions)]
#[test]
fn jobs_submitted_after_end_still_check_the_debug_codec() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    script.state.lock().unwrap().end();
    drop(jobs.take_with_time(&live()));
    jobs.submit(
        1,
        0,
        0,
        Work {
            required: true,
            ..Work::new(255)
        },
    )
    .unwrap();
    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.lane(), Lane::Inline);
    assert_eq!(
        take.next().unwrap().result,
        Err(JobError("missing required field".into()))
    );
}

#[test]
fn ready_with_no_workers_ends_the_queue() {
    let (mut jobs, script, _) = setup(LIMITS, 0);
    script.state.lock().unwrap().ready_zero = true;
    submit(&mut jobs, 1, 0);
    assert_eq!(results(&mut jobs, live()), [1]);
    script.state.lock().unwrap().ready_zero = false;
    script.ready(1);
    submit(&mut jobs, 2, 0);
    assert_eq!(results(&mut jobs, live()), [2]);
    assert_eq!(script.state.lock().unwrap().starts, 0);
}

#[test]
fn a_cold_end_differs_from_starting_and_a_partial_start_can_work() {
    let (mut jobs, script, _) = setup(LIMITS, 0);
    script.state.lock().unwrap().end();
    submit(&mut jobs, 1, 0);
    assert_eq!(results(&mut jobs, live()), [1]);
    assert_eq!(script.state.lock().unwrap().starts, 0);

    let (mut jobs, script, _) = setup(LIMITS, 0);
    submit(&mut jobs, 1, 0);
    assert_eq!(results(&mut jobs, live()), [1]);
    script.finish_start(&[false, true]);
    submit(&mut jobs, 2, 0);
    assert!(results(&mut jobs, live()).is_empty());
    assert_eq!(jobs.report().lane, Lane::Worker);
    assert_eq!(script.state.lock().unwrap().workers.len(), 1);
    script.complete(script.posting(0).1[0]);
    assert_eq!(results(&mut jobs, live()), [2]);
    submit(&mut jobs, 3, 0);
    drop(jobs.take_with_time(&live()));
    script.state.lock().unwrap().fail_replacement = true;
    script.lose_batch_with(script.posting(1).1[0]);
    assert_eq!(results(&mut jobs, live()), [3]);
    assert_eq!(jobs.report().lane, Lane::Inline);
}

#[test]
fn a_replaced_workers_late_answer_cannot_finish_the_reposted_job() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    let old = script.posting(0).1[0];
    script.lose_batch_with(old);
    drop(jobs.take_with_time(&live()));
    let retry = script.posting(1).1[0];
    assert_ne!(old, retry);
    script.complete(old);
    assert!(results(&mut jobs, live()).is_empty());
    script.complete(retry);
    assert_eq!(results(&mut jobs, live()), [1]);
}

#[test]
fn declined_jobs_run_while_earlier_wire_jobs_are_queued_or_out() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 10);
    for (value, expected_postings) in [(2, 0), (3, 1)] {
        let mut take = jobs.take_with_time(&live());
        take.submit(
            value,
            0,
            0,
            Work {
                local: true,
                ..Work::new(value as u8)
            },
        )
        .unwrap();
        assert_eq!(take.next().unwrap().result, Ok(value as u8));
        assert!(take.next().is_none());
        assert_eq!(
            script.state.lock().unwrap().postings.len(),
            expected_postings
        );
    }
    script.complete(script.posting(0).1[0]);
    assert_eq!(results(&mut jobs, live()), [1]);
}

#[test]
fn a_refused_replacement_delivers_the_old_value() {
    for arrived in [false, true] {
        let (mut jobs, script, _) = setup(
            Limits {
                queued_bytes: 2,
                ..LIMITS
            },
            1,
        );
        submit(&mut jobs, 1, 0);
        submit(&mut jobs, 2, 0);
        drop(jobs.take_with_time(&live()));
        let (_, ids) = script.posting(0);
        if arrived {
            script.complete(ids[0]);
            drop(jobs.take_with_time(&live()));
        }
        assert!(jobs
            .submit(
                1,
                1,
                10,
                Work {
                    bytes: 2,
                    ..Work::new(11)
                }
            )
            .is_err());
        if !arrived {
            script.complete(ids[0]);
        }
        let got = jobs.take_with_time(&live()).next().unwrap();
        assert_eq!((got.key, got.generation, got.result), (1, 0, Ok(1)));
    }
}

#[test]
fn invalidated_out_and_arrived_jobs_stay_gone_at_clock_handoff_and_end() {
    for seekable in [true, false] {
        let (mut jobs, script, _) = setup(LIMITS, 1);
        for value in 1..=3 {
            submit(&mut jobs, value, 0);
        }
        drop(jobs.take_with_time(&live()));
        let (_, ids) = script.posting(0);
        script.complete(ids[0]);
        drop(jobs.take_with_time(&live()));
        jobs.cancel(1);
        jobs.submit(2, 1, 10, Work::new(12)).unwrap();
        assert_eq!(jobs.report().running, 1);
        if !seekable {
            script.state.lock().unwrap().end();
        }
        assert_eq!(results(&mut jobs, time(seekable, 8)), [12, 3]);
        script.complete(ids[1]);
        assert!(results(&mut jobs, live()).is_empty());
    }
}

#[test]
fn a_snapshot_below_the_completed_threshold_can_post() {
    let (mut jobs, script, _) = setup(
        Limits {
            completed_bytes: 3,
            ..LIMITS
        },
        1,
    );
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    submit(&mut jobs, 2, 0);
    drop(jobs.take_with_time(&live()));
    assert_eq!(script.posted_values(&script.posting(1).1), [2]);
    assert_eq!(results(&mut jobs, live()), [1]);
}

#[test]
fn a_bad_answer_defers_worker_loss_until_the_next_snapshot() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    let worker = script.state.lock().unwrap().next_worker;
    script
        .state
        .lock()
        .unwrap()
        .answer(ids[0], Outcome::Bytes(1), vec![0]);
    submit(&mut jobs, 2, 0);
    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.next().unwrap().result, Ok(1));
    let during_take = script.state.lock().unwrap().next_worker;
    assert_eq!(during_take, worker);
    drop(take);
    let postings = script.state.lock().unwrap().postings.len();
    assert_eq!(postings, 1);
    assert!(results(&mut jobs, live()).is_empty());
    assert_ne!(script.state.lock().unwrap().next_worker, worker);
    let retry = script.posting(1).1[0];
    script.complete(retry);
    assert_eq!(results(&mut jobs, live()), [2]);
}

#[test]
fn attempts_cross_the_u32_boundary() {
    let (mut jobs, script, handle) = setup(LIMITS, 1);
    handle
        .next_attempt
        .store((u32::MAX - 1).into(), Ordering::Relaxed);
    for value in 1..=3 {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    let ids: Vec<_> = ids.into_iter().map(u128::from).collect();
    assert_eq!(
        ids,
        [u32::MAX as u128 - 1, u32::MAX as u128, u32::MAX as u128 + 1]
    );
}

#[test]
fn hook_work_blocks_late_posts_on_this_take_and_later_takes() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    let frame = late();
    let mut first = jobs.take_with_time(&frame);
    first.mark_late_unit_spent();
    first.submit(1, 0, 0, Work::new(1)).unwrap();
    drop(first);
    drop(jobs.take_with_time(&frame));
    let posts = script.state.lock().unwrap().postings.len();
    assert_eq!(posts, 0);
    drop(jobs.take_with_time(&late()));
    assert_eq!(script.posted_values(&script.posting(0).1), [1]);
}

#[test]
#[should_panic(expected = "accepted attempt left its worker without a terminal event")]
fn scripted_transport_rejects_an_orphaned_attempt() {
    let script = Script::default();
    script.ready(1);
    script.set_asking(1, true);
    script
        .post(
            1,
            &[Posting {
                attempt: 1,
                name: Work::NAME,
                input: vec![1, 1],
            }],
        )
        .unwrap();
    script.state.lock().unwrap().batches.clear();
    script.events(1);
}

#[test]
fn advancing_agent_clock_merges_declined_and_requeued_wire_jobs() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    drop(jobs.take_with_time(&live()));
    jobs.submit(
        2,
        0,
        0,
        Work {
            local: true,
            ..Work::new(2)
        },
    )
    .unwrap();
    submit(&mut jobs, 3, 0);
    assert_eq!(results(&mut jobs, time(true, 2)), [1, 2]);
    assert_eq!(results(&mut jobs, time(true, 1)), [3]);
    assert!(script.state.lock().unwrap().copies.is_empty());
}

#[test]
fn a_trap_survives_loss_and_end_on_live_takes() {
    for (learn_first, end, seekable) in [
        (false, true, false),
        (true, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let (mut jobs, script, _) = setup(LIMITS, 1);
        submit(&mut jobs, 1, 0);
        submit(&mut jobs, 2, 0);
        drop(jobs.take_with_time(&live()));
        let (_, ids) = script.posting(0);
        {
            let mut state = script.state.lock().unwrap();
            state.fail_replacement = end && !learn_first;
            state.answer(ids[0], Outcome::Trapped, Vec::new());
        }
        if learn_first {
            drop(jobs.take_with_time(&live()));
            if end {
                script.state.lock().unwrap().end();
                if seekable {
                    drop(jobs.take_with_time(&live()));
                }
            }
        }
        let got: Vec<_> = jobs.take_with_time(&time(seekable, 8)).collect();
        let expected = if seekable {
            Ok(1)
        } else {
            Err(JobError("job trapped".into()))
        };
        assert_eq!(
            got[0].result, expected,
            "learned={learn_first}, ended={end}"
        );
        assert_eq!(
            got.get(1).map(|done| &done.result),
            (end || seekable).then_some(&Ok(2))
        );
        assert_eq!(jobs.report().failed, u64::from(!seekable));
    }
}

#[test]
fn a_ready_queue_drains_answers_while_a_replacement_starts() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    script.state.lock().unwrap().workers.clear();
    submit(&mut jobs, 2, 0);

    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.lane(), Lane::Worker);
    assert_eq!(take.next().unwrap().result, Ok(1));
    assert!(take.next().is_none());
    drop(take);
    let postings = script.state.lock().unwrap().postings.len();
    assert_eq!(postings, 1);

    script.ready(1);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(1).1[0]);
    assert_eq!(results(&mut jobs, live()), [2]);
}

#[test]
fn end_drops_all_buffers_even_for_queues_that_stop_taking() {
    let (mut first, script, handle) = setup(LIMITS, 1);
    let mut second = with_transport(LIMITS, &handle);
    submit(&mut first, 1, 0);
    drop(first.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    drop(first.take_with_time(&live()));
    submit(&mut second, 2, 0);
    drop(second.take_with_time(&live()));
    script.complete(script.posting(1).1[0]);
    let (outputs, announced) = {
        let mut state = script.state.lock().unwrap();
        assert_eq!(state.outputs.len(), 2);
        state.end();
        (state.outputs.len(), state.announced.len())
    };
    assert_eq!((outputs, announced), (0, 0));
    assert_eq!(results(&mut second, live()), [2]);
}

#[test]
fn an_open_take_falls_back_when_another_queue_ends_the_lane() {
    let (mut first, script, handle) = setup(LIMITS, 2);
    let mut second = with_transport(LIMITS, &handle);
    submit(&mut first, 1, 0);
    submit(&mut second, 2, 0);
    let frame = live();
    drop(first.take_with_time(&frame));
    drop(second.take_with_time(&frame));
    let first_attempt = script.posting(0).1[0];
    let second_attempt = script.posting(1).1[0];
    {
        let mut state = script.state.lock().unwrap();
        state.losses = 2;
        state.answer(first_attempt, Outcome::Bytes(2), vec![0, 99]);
        state.answer(second_attempt, Outcome::Bytes(1), vec![0]);
    }
    let frame = live();
    let mut take = first.take_with_time(&frame);
    assert_eq!(take.lane(), Lane::Worker);
    assert!(script
        .state
        .lock()
        .unwrap()
        .announced
        .contains(&first_attempt));
    assert_eq!(results(&mut second, frame), [2]);
    assert_eq!(second.take_with_time(&frame).lane(), Lane::Inline);
    assert_eq!(take.next().unwrap().result.unwrap(), 1);
    assert!(take.next().is_none());
    drop(take);
    assert_eq!(first.pending(), 0);
    assert!(results(&mut first, live()).is_empty());
    assert_eq!(script.state.lock().unwrap().losses, 3);
}

#[test]
fn a_single_out_or_arrived_job_blocks_an_oversize_admission() {
    let (mut jobs, script, _) = setup(
        Limits {
            queued_bytes: 1,
            ..LIMITS
        },
        1,
    );
    let oversize = || Work {
        bytes: 2,
        ..Work::new(2)
    };
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    assert!(jobs.submit(2, 0, 0, oversize()).is_err());
    script.complete(script.posting(0).1[0]);
    drop(jobs.take_with_time(&live()));
    assert!(jobs.submit(2, 0, 0, oversize()).is_err());
    assert_eq!(results(&mut jobs, live()), [1]);
    jobs.submit(2, 0, 0, oversize()).unwrap();
    assert_eq!(jobs.report().queued_bytes, 2);
}

#[test]
fn completed_byte_accounting_can_exceed_wasm_address_space() {
    let (mut jobs, script, _) = setup(
        Limits {
            completed_bytes: u32::MAX as usize,
            ..LIMITS
        },
        1,
    );
    for value in [1, 2] {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    {
        let mut state = script.state.lock().unwrap();
        for (attempt, len) in ids.into_iter().zip([u32::MAX as usize, 1]) {
            state.answer(attempt, Outcome::Bytes(len), Vec::new());
        }
    }
    submit(&mut jobs, 3, 0);
    drop(jobs.take_with_time(&live()));
    assert_eq!(script.state.lock().unwrap().postings.len(), 1);
    let state = jobs.shared.state.lock().unwrap();
    let accounted = &state.completed_bytes as &dyn std::any::Any;
    assert_eq!(accounted.downcast_ref::<u64>().copied(), Some(1u64 << 32));
    drop(state);
    jobs.cancel(1);
    drop(jobs.take_with_time(&live()));
    assert_eq!(script.posted_values(&script.posting(1).1), [3]);
}
