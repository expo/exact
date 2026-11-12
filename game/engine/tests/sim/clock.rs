use super::*;

fn live(hz: f64) -> Sim<Counter> {
    let mut s = Sim::<Counter>::new(CounterArgs::default()).unwrap();
    s.frame_period(1000.0 / hz);
    s.advance(0.0, Clock::Live);
    s
}
fn drawn_counter(s: &Sim<Counter>) -> f64 {
    if s.world().tick() == 0 {
        0.0
    } else {
        (s.world().tick() - 1) as f64 + f64::from(s.alpha())
    }
}
#[test]
fn unknown_period_has_no_lookahead_and_duplicates_preserve_the_pose() {
    let mut s = sim();
    s.advance(10.0, Clock::Live);
    assert_eq!(s.world().tick(), 0);
    s.frame_period(1000.0 / 120.0);
    s.advance(10.0, Clock::Live);
    assert_eq!(s.world().tick(), 0); // A new period cannot move a zero-delta pose.
    let pose = drawn_counter(&s);
    for _ in 0..10 {
        assert_eq!(s.ticks_due(10.0, Clock::Live), 0);
        assert_eq!(s.advance(10.0, Clock::Live), 0);
        assert_eq!(drawn_counter(&s), pose);
    }
}
#[test]
fn period_transitions_share_one_bounded_monotonic_render_slew() {
    for (old, new) in [(0.0, 60.0), (60.0, 120.0), (60.0, 144.0), (120.0, 60.0)] {
        for offset in [0.0, 3.1, 6.2] {
            let mut s = if old == 0.0 { sim() } else { live(old) };
            let before_period = 1000.0 / if old == 0.0 { new } else { old };
            let mut now = offset;
            for _ in 0..600 {
                now += before_period;
                s.advance(now, Clock::Live);
            }
            let period = 1000.0 / new;
            let mut previous = drawn_counter(&s);
            s.frame_period(period);
            assert_eq!(s.advance(now, Clock::Live), 0);
            assert_eq!(drawn_counter(&s), previous);
            for frame in 1..=1800 {
                now += period;
                assert_eq!(s.ticks_due(now, Clock::Live), s.advance(now, Clock::Live));
                let drawn = drawn_counter(&s);
                let delta = (drawn - previous) * 1000.0 / 60.0;
                assert!(delta > 0.0);
                assert!(
                    (delta - period).abs() <= period * 0.0025 + 0.00003,
                    "{old}->{new}, {offset}, frame {frame}: {delta}"
                );
                previous = drawn;
            }
            // L has actually arrived, rather than merely ignoring the new rate.
            // The grid has also settled: ticking frames have the new rate's alpha.
            let mut ticking = 0;
            for _ in 0..60 {
                now += period;
                if s.advance(now, Clock::Live) > 0 {
                    ticking += 1;
                }
                let horizon = drawn_counter(&s) + 1.0;
                let l = period.min(1000.0 / 60.0) * 60.0 / 1000.0;
                // The world time in a save is exact when this frame did not tick early.
                if s.alpha() as f64 >= l {
                    let mut restored = sim();
                    restored.restore(&s.save().unwrap()).unwrap();
                    assert!(
                        (horizon - l - restored.world().tick() as f64 - restored.alpha() as f64)
                            .abs()
                            < 0.0001
                    );
                }
            }
            assert!(ticking > 0);
        }
    }
}
#[test]
fn sub_half_percent_period_noise_cannot_restart_grid_slew() {
    let mut stable = live(144.0);
    let mut noisy = live(144.0);
    for frame in 1..=1500 {
        let period = 1000.0 / 144.0;
        noisy.frame_period(period * if frame % 2 == 0 { 1.004 } else { 0.996 });
        let now = 3.1 + frame as f64 * period;
        assert_eq!(
            stable.advance(now, Clock::Live),
            noisy.advance(now, Clock::Live)
        );
        assert_eq!(stable.alpha(), noisy.alpha());
        assert_eq!(stable.save().unwrap(), noisy.save().unwrap());
    }
}
#[test]
fn duplicate_stamp_with_negative_half_unit_remainder_cannot_retreat() {
    let mut s = sim();
    let now = 20.000025; // 1_200_001.5 units: round up and retain -0.5.
    s.advance(now, Clock::Live);
    let pose = drawn_counter(&s);
    let saved = s.save().unwrap();
    for _ in 0..100 {
        assert_eq!(s.ticks_due(now, Clock::Live), 0);
        assert_eq!(s.advance(now, Clock::Live), 0);
        assert_eq!(drawn_counter(&s), pose);
        assert_eq!(s.save().unwrap(), saved);
    }
}
#[test]
fn delivered_live_input_is_eligible_at_the_paced_frame_but_seekable_future_waits() {
    for clock in [Clock::Live, Clock::Seekable] {
        let mut s = live(60.0);
        key(&mut s, "KeyE", true, 16.8);
        assert_eq!(s.advance(1000.0 / 60.0, clock), 1);
        assert_eq!(
            s.world().resource::<Counts>().pressed,
            u32::from(clock == Clock::Live)
        );
        s.advance(33.334, clock); // Seekable rounds to integer µs, past tick 2.
        assert_eq!(s.world().resource::<Counts>().pressed, 1);
    }
}
#[test]
fn synthetic_live_frames_have_no_equal_rate_beats() {
    for hz in [60.0, 59.94, 120.0] {
        for epoch in [0.0, 1234.567, 1_000_000.123] {
            let mut s = Sim::<Counter>::new(CounterArgs::default()).unwrap();
            s.frame_period(1000.0 / hz);
            s.advance(epoch, Clock::Live);
            let mut extra = 0;
            for frame in 1..=3600 {
                let now = epoch + frame as f64 * 1000.0 / hz;
                let due = s.ticks_due(now, Clock::Live);
                let ticks = s.advance(now, Clock::Live);
                assert_eq!(due, ticks);
                if hz == 60.0 {
                    assert_eq!(ticks, 1, "frame {frame}, epoch {epoch}");
                } else if hz == 120.0 {
                    assert_eq!(ticks, u32::from(frame % 2 == 0));
                } else {
                    assert!((1..=2).contains(&ticks));
                    extra += ticks - 1;
                }
            }
            if hz == 59.94 {
                assert!((3..=4).contains(&extra));
            }
        }
    }
}
#[test]
fn jittered_sixty_hz_stays_within_one_tick_over_six_hundred_frames() {
    let mut s = live(60.0);
    let mut now = 0.0;
    // The five deltas total five nominal periods, without making individual
    // raw callbacks a lattice. Counts can straddle a boundary by at most one.
    for frame in 1..=600 {
        now += [16.0, 17.5, 16.4, 16.2, 1000.0 / 12.0 - 66.1][(frame - 1) % 5];
        let due = s.ticks_due(now, Clock::Live);
        assert_eq!(s.advance(now, Clock::Live), due);
        assert!((s.world().tick() as i64 - frame as i64).abs() <= 1);
    }
}
#[test]
fn live_stall_and_its_first_recovery_frame_keep_the_same_period() {
    for hz in [60.0, 120.0, 144.0] {
        let delta = 1000.0 / hz;
        let mut s = live(hz);
        for frame in 1..=10 {
            s.advance(frame as f64 * delta, Clock::Live);
        }
        let mut previous = drawn_counter(&s);
        let now = 10.0 * delta + 40.0;
        s.advance(now, Clock::Live);
        assert!((drawn_counter(&s) - previous - 40.0 * 60.0 / 1000.0).abs() < 1e-6);
        previous = drawn_counter(&s);
        for frame in 1..=10 {
            s.advance(now + frame as f64 * delta, Clock::Live);
            let drawn = drawn_counter(&s);
            assert!(
                (drawn - previous - 60.0 / hz).abs() < 1e-6,
                "{hz} recovery {frame}"
            );
            previous = drawn;
        }
    }
}
#[test]
fn every_starting_phase_aligns_within_four_seconds_and_holds() {
    for hz in [60.0, 120.0, 240.0] {
        let period = 1000.0 / hz;
        for phase in 0..=100 {
            let mut s = live(hz);
            let offset = phase as f64 / 100.0 * period;
            let mut previous = 0.0;
            for frame in 1..=(hz as usize * 6) {
                let now = offset + frame as f64 * period;
                let due = s.ticks_due(now, Clock::Live);
                let ticks = s.advance(now, Clock::Live);
                assert_eq!(due, ticks);
                let drawn = drawn_counter(&s);
                if frame > 3 {
                    let delta = (drawn - previous) * 1000.0 / 60.0;
                    assert!(
                        (delta - period).abs() <= period * 0.0025 + 0.00002,
                        "{hz}/{phase}/{frame}: {delta}"
                    );
                }
                if frame as f64 * period >= 4000.0 {
                    // T/step = drawn + 1 - L/step; the origin meets this lattice.
                    let time = (drawn + 1.0) * 1000.0 / 60.0 - period;
                    let error = (time / period - (time / period).round()).abs() * period;
                    assert!(error < 0.0001, "{hz}/{phase}/{frame}: {error}");
                    if ticks > 0 {
                        assert!((s.alpha() as f64 - 60.0 / hz).abs() < 1e-6);
                    }
                }
                previous = drawn;
            }
        }
    }
}
#[test]
fn aligned_ticks_consume_input_delivered_after_the_preceding_frame() {
    for hz in [60.0, 120.0, 240.0] {
        let period = 1000.0 / hz;
        let mut s = live(hz);
        let offset = period * 0.37;
        let mut last = 0.0;
        for frame in 1..=(hz as usize * 4) {
            last = offset + frame as f64 * period;
            s.advance(last, Clock::Live);
        }
        let mut edges = 0;
        for frame in 1..=hz as usize {
            let now = last + frame as f64 * period;
            let due = s.ticks_due(now, Clock::Live);
            if due > 0 {
                // Delivery really happens after the previous advance, including
                // an event exactly at this frame's stamp. No prequeued future input.
                key(
                    &mut s,
                    "KeyE",
                    edges % 2 == 0,
                    now - if edges % 3 == 0 { 0.0 } else { period / 3.0 },
                );
                edges += 1;
            }
            assert_eq!(s.advance(now, Clock::Live), due);
            let c = s.world().resource::<Counts>();
            assert_eq!(c.pressed + c.released, edges, "display {hz}, frame {frame}");
        }
    }
}
#[test]
fn tick_phase_is_one_at_sixty_and_half_at_one_twenty() {
    for hz in [60.0, 120.0] {
        let mut s = live(hz);
        let (mut total, mut count) = (0.0, 0);
        for frame in 1..=600 {
            if s.advance(frame as f64 * 1000.0 / hz, Clock::Live) > 0 {
                total += s.alpha() as f64;
                count += 1;
            }
        }
        assert!((total / count as f64 - 60.0 / hz).abs() < 1e-6);
    }
}
#[test]
fn submicrosecond_backwards_stamps_cannot_add_time_or_change_lookahead() {
    let mut s = live(120.0);
    s.advance(10.0004, Clock::Live);
    let pose = drawn_counter(&s);
    let saved = s.save().unwrap();
    for _ in 0..1000 {
        s.frame_period(1000.0 / 60.0);
        assert_eq!(s.ticks_due(9.9996, Clock::Live), 0);
        assert_eq!(s.advance(9.9996, Clock::Live), 0);
        assert_eq!(drawn_counter(&s), pose);
        s.frame_period(1000.0 / 120.0);
        assert_eq!(s.advance(10.0004, Clock::Live), 0);
        assert_eq!(drawn_counter(&s), pose);
        assert_eq!(s.save().unwrap(), saved);
    }
}
#[test]
fn early_live_save_and_switch_catch_the_exact_clock_up_without_a_tick() {
    let mut s = live(120.0);
    s.advance(10.0, Clock::Live);
    assert_eq!(s.world().tick(), 1);
    key(&mut s, "KeyE", true, 12.0); // A future input survives save and switch.
    let saved = s.save().unwrap();
    let hash = s.world().hash();
    let mut restored = sim();
    restored.restore(&saved).unwrap();
    assert_eq!(restored.save().unwrap(), saved);
    assert_eq!(restored.world().hash(), hash);
    assert!(restored.alpha() < 0.0001); // ceil(1e6/60) us, exact seekable convention.
    assert_eq!(s.ticks_due(10.0, Clock::Seekable), 0);
    assert_eq!(s.advance(10.0, Clock::Seekable), 0);
    assert_eq!(s.save().unwrap(), saved);
    assert_eq!(s.world().hash(), hash);
    restored.advance(10.0, Clock::Seekable);
    for now in [12.0, 26.667, 43.334, 100.0] {
        s.advance(now, Clock::Seekable);
        restored.advance(now, Clock::Seekable);
        assert_eq!(s.save().unwrap(), restored.save().unwrap());
        assert_eq!(s.world().hash(), restored.world().hash());
    }
    assert_eq!(s.world().resource::<Counts>().pressed, 1);
}
#[test]
fn unpause_preserves_live_time_and_drops_the_pause_gap() {
    let mut s = live(120.0);
    s.advance(10.0, Clock::Live);
    s.bind(&CounterArgs { paused: true }.values(), None)
        .unwrap();
    s.advance(2000.0, Clock::Live);
    s.bind(&CounterArgs::default().values(), None).unwrap();
    assert_eq!(s.ticks_due(50_000.0, Clock::Live), 0);
    assert_eq!(s.advance(50_000.0, Clock::Live), 0);
    assert_eq!(s.world().tick(), 1);
    assert_eq!(s.advance(50_000.0 + 1000.0 / 60.0, Clock::Live), 1);
}
#[test]
fn live_pause_holds_the_drawn_pose_through_binds_frames_and_resume() {
    for hz in [60.0, 90.0, 120.0, 144.0] {
        for stamp_offset in [None, Some(0.0), Some(4.0)] {
            let mut s = live(hz);
            let period = 1000.0 / hz;
            let now = 30.0 * period;
            for frame in 1..=30 {
                s.advance(frame as f64 * period, Clock::Live);
            }
            let stamp = stamp_offset.map(|offset| now + offset);
            let before = drawn_counter(&s);
            s.bind(&CounterArgs { paused: true }.values(), stamp)
                .unwrap();
            let pose = drawn_counter(&s);
            let delta = stamp_offset.unwrap_or(0.0) * 60.0 / 1000.0;
            assert!(
                (pose - before - delta).abs() < 0.001,
                "{hz} Hz, stamp {stamp:?}: {before} -> {pose}"
            );
            let saved = s.save().unwrap();
            for at in [now + period, now + 1000.0, now + 2000.0] {
                assert_eq!(s.advance(at, Clock::Live), 0);
                assert_eq!(drawn_counter(&s), pose);
                assert_eq!(s.save().unwrap(), saved);
                s.bind(&CounterArgs { paused: true }.values(), stamp.map(|_| at))
                    .unwrap();
                assert_eq!(drawn_counter(&s), pose);
            }
            s.bind(
                &CounterArgs::default().values(),
                stamp.map(|_| now + 3000.0),
            )
            .unwrap();
            assert_eq!(drawn_counter(&s), pose);
            assert_eq!(s.advance(now + 4000.0, Clock::Live), 0);
            assert_eq!(drawn_counter(&s), pose);
            s.advance(now + 4000.0 + period, Clock::Live);
            assert!((drawn_counter(&s) - pose - 60.0 / hz).abs() < 0.002);
        }
    }
}
#[test]
fn live_three_tick_catchup_spreads_input_by_stamp_and_caps_a_large_gap() {
    let mut s = live(60.0);
    // Acquire the origin on a normal frame before the stall.
    s.advance(1000.0 / 60.0, Clock::Live);
    key(&mut s, "KeyE", true, 35.0);
    key(&mut s, "KeyE", false, 52.0);
    let mut seen = Vec::new();
    assert_eq!(
        s.advance_with(1000.0 / 60.0 + 40.0, Clock::Live, |w, _| {
            let c = w.resource::<Counts>();
            seen.push((w.tick(), c.held, c.pressed, c.released));
        }),
        3
    );
    assert_eq!(seen, [(2, 0, 0, 0), (3, 1, 1, 0), (4, 1, 1, 1)]);
    assert_eq!(s.advance(10_056.667, Clock::Live), 15);
}
#[test]
fn slew_and_period_are_absent_from_save_state_snapshot_and_seekable_continuation() {
    let mut s = live(120.0);
    for frame in 1..=480 {
        s.advance(3.0 + frame as f64 * 1000.0 / 120.0, Clock::Live);
    }
    let mut plain = sim();
    plain.restore(&s.save().unwrap()).unwrap();
    assert_eq!(s.world().hash(), plain.world().hash());
    assert_eq!(s.world().save(), plain.world().save());
    let state = s.agent(r#"{"op":"state"}"#);
    let restored_state = plain.agent(r#"{"op":"state"}"#).replace(
        r#""restored":true,"restoredFrom":{"paused":false},"#,
        r#""restored":false,"#,
    );
    assert_eq!(state, restored_state);
    for forbidden in ["lookahead", "slew", "period", "live_time"] {
        assert!(!state.contains(forbidden));
    }
    let now = 4003.0;
    s.advance(now, Clock::Seekable);
    plain.advance(now, Clock::Seekable);
    // The restored host has never had a period or a slew. Continuation is exact.
    for delta in [0.0, 1.0, 16.667, 100.0, 1000.0] {
        s.advance(now + delta, Clock::Seekable);
        plain.advance(now + delta, Clock::Seekable);
        assert_eq!(s.save().unwrap(), plain.save().unwrap());
        assert_eq!(s.world().hash(), plain.world().hash());
    }
}

#[test]
fn aligned_live_input_is_drawn_one_frame_before_seekable_interpolation() {
    struct Mover;
    impl Game for Mover {
        const ID: &'static str = "latency";
        type Args = ();
        fn setup(w: &mut World, _: &()) {
            w.spawn_named("player", Transform::default());
        }
        fn actions() -> Actions {
            Actions::new().button("move", &["KeyE"])
        }
        fn tick(w: &mut World, i: &Input, _: &()) {
            if i.held("move") {
                w.get_mut::<Transform>("player").unwrap().position.x += 1.0;
            }
        }
    }
    let period = 1000.0 / 120.0;
    let mut first = Vec::new();
    for clock in [Clock::Live, Clock::Seekable] {
        let mut s = Sim::<Mover>::new(()).unwrap();
        s.frame_period(period);
        s.advance(0.0, clock);
        let (mut previous, mut current) = (0.0, 0.0);
        let mut first_frame = None;
        for frame in 1..=6 {
            if frame == 4 {
                // Delivered after frame 3 (25 ms), before tick 2's deadline.
                s.input(InputEvent::Key {
                    code: "KeyE".into(),
                    down: true,
                    at_ms: 30.0,
                    player: 0,
                });
            }
            s.advance_with(frame as f64 * period, clock, |w, _| {
                previous = current;
                current = w.get::<Transform>("player").unwrap().position.x;
                if current == 1.0 {
                    assert_eq!(w.tick(), 2);
                }
            });
            if previous + (current - previous) * s.alpha() > 0.01 && first_frame.is_none() {
                first_frame = Some(frame);
            }
        }
        first.push(first_frame.unwrap());
    }
    assert_eq!(first, [4, 5]);
}
#[test]
fn paused_input_follows_device_events_queued_before_the_pause() {
    // A key goes down after the last frame, the app pauses and the key lifts,
    // all before the next frame: the release must not overtake the press.
    let mut s = live(60.0);
    s.advance(16.7, Clock::Live);
    key(&mut s, "KeyE", true, 19.7);
    s.bind(&CounterArgs { paused: true }.values(), None)
        .unwrap();
    key(&mut s, "KeyE", false, 25.0);
    s.advance(33.4, Clock::Live);
    s.advance(50.1, Clock::Live);
    s.bind(&CounterArgs::default().values(), None).unwrap();
    for frame in 4..64 {
        s.advance(frame as f64 * 16.7, Clock::Live);
    }
    assert_eq!(s.world().resource::<Counts>().held, 0);
    let state = s.agent(r#"{"op":"state"}"#);
    assert!(state.contains(r#""held":[],"forwarded":[]"#), "{state}");
}
