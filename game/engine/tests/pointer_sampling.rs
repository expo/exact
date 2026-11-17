use exact_game::*;

#[derive(Default, Resource)]
struct Edges(Vec<PointerEdge>);
struct Fingers;
impl Game for Fingers {
    const ID: &'static str = "frame-pointers";
    const HZ: u32 = 100;
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.insert_resource(Edges::default());
    }
    fn tick(w: &mut World, input: &Input, _: &()) {
        w.resource_mut::<Edges>()
            .0
            .extend_from_slice(input.pointer_events());
    }
}
fn pointer(id: u64, phase: PointerPhase, x: f32, at_ms: f64) -> InputEvent {
    InputEvent::Pointer {
        id,
        phase,
        x,
        y: 0.,
        dx: 1.,
        dy: 0.,
        buttons: u32::from(matches!(phase, PointerPhase::Down | PointerPhase::Move)),
        at_ms,
        player: 0,
    }
}
#[test]
fn newest_samples_do_not_consume_edges_or_change_a_save() {
    let mut sim = Sim::<Fingers>::new(()).unwrap();
    sim.advance(1000., Clock::Seekable);
    sim.input(pointer(7, PointerPhase::Down, 0., 1001.));
    assert!(!sim.frame_pointer(7).unwrap().current_contact);
    sim.advance(1010., Clock::Seekable);
    assert!(sim.frame_pointer(7).unwrap().current_contact);
    sim.input(pointer(7, PointerPhase::Move, 3., 1013.));
    sim.input(pointer(7, PointerPhase::Move, 2., 1012.));
    sim.input(pointer(9, PointerPhase::Down, 80., 1014.));
    let save = sim.save().unwrap();
    let hash = sim.world().hash();
    for _ in 0..10 {
        let p = sim.frame_pointer(7).unwrap();
        assert_eq!(
            (p.position.x, p.at_ms, p.phase),
            (3., 1013., PointerPhase::Move)
        );
        assert!(p.current_contact);
        assert_eq!(sim.frame_pointer(9).unwrap().position.x, 80.);
    }
    assert_eq!(save, sim.save().unwrap());
    assert_eq!(hash, sim.world().hash());
    sim.input(pointer(7, PointerPhase::Up, 4., 1015.));
    sim.input(pointer(9, PointerPhase::Cancel, 80., 1016.));
    assert_eq!(sim.frame_pointer(7).unwrap().phase, PointerPhase::Up);
    sim.advance(1020., Clock::Seekable);
    let edges: Vec<_> = sim
        .world()
        .resource::<Edges>()
        .0
        .iter()
        .map(|e| (e.id, e.phase))
        .collect();
    assert_eq!(
        edges,
        [
            (7, PointerPhase::Down),
            (9, PointerPhase::Down),
            (7, PointerPhase::Up),
            (9, PointerPhase::Cancel)
        ]
    );
    assert_eq!(sim.frame_pointer(7).unwrap().at_ms, 1015.);
    sim.advance(1030., Clock::Seekable);
    assert!(sim.frame_pointer(7).is_none());
    assert!(sim.frame_pointer(9).is_none());
}

#[test]
fn restored_queue_has_no_fabricated_host_timestamps_and_reused_ids_wait_for_a_tick() {
    let mut sim = Sim::<Fingers>::new(()).unwrap();
    sim.advance(1000., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Down, 0., 1001.));
    sim.advance(1010., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Move, 5., 1015.));
    let saved = sim.save().unwrap();
    sim.restore(&saved).unwrap();
    assert!(sim.frame_pointer(1).is_none());
    sim.advance(2000., Clock::Seekable);
    sim.advance(2020., Clock::Seekable);
    assert!(sim.frame_pointer(1).is_none());
    sim.input(pointer(1, PointerPhase::Up, 5., 2021.));
    sim.input(pointer(1, PointerPhase::Down, 100., 2022.));
    sim.input(pointer(1, PointerPhase::Move, 102., 2023.));
    assert!(!sim.frame_pointer(1).unwrap().current_contact);
    sim.advance(2030., Clock::Seekable);
    assert!(sim.frame_pointer(1).unwrap().current_contact);
    sim.input(InputEvent::Blur {
        at_ms: 2031.,
        player: 0,
    });
    assert!(sim.frame_pointer(1).is_none());
}

#[test]
fn a_sample_delivered_after_its_successor_invalidates_only_the_frame_anchor() {
    let mut sim = Sim::<Fingers>::new(()).unwrap();
    sim.advance(1000., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Down, 0., 1001.));
    sim.input(pointer(1, PointerPhase::Move, 10., 1009.));
    sim.advance(1010., Clock::Seekable);
    assert!(sim.frame_pointer(1).unwrap().current_contact);
    sim.input(pointer(1, PointerPhase::Move, 8., 1008.));
    // The current tick still represents the newer sample until delivery.
    assert!(sim.frame_pointer(1).unwrap().current_contact);
    sim.advance(1020., Clock::Seekable);
    let p = sim.frame_pointer(1).unwrap();
    assert_eq!((p.position.x, p.at_ms), (10., 1009.));
    assert!(!p.current_contact);
    sim.input(pointer(1, PointerPhase::Move, 12., 1021.));
    assert!(!sim.frame_pointer(1).unwrap().current_contact);
    sim.advance(1030., Clock::Seekable);
    assert!(sim.frame_pointer(1).unwrap().current_contact);
}

#[test]
fn paranoid_rebuild_keeps_live_queue_stamps_and_terminal_samples_do_not_anchor_hover() {
    for mode in [Paranoid::Off, Paranoid::Save, Paranoid::FreshGame] {
        let mut sim = Sim::<Fingers>::new(()).unwrap().paranoid(mode);
        sim.advance(1000., Clock::Seekable);
        sim.input(pointer(1, PointerPhase::Down, 70., 1001.));
        sim.input(pointer(1, PointerPhase::Move, 75., 1015.));
        sim.advance(1010., Clock::Seekable);
        let p = sim.frame_pointer(1).unwrap();
        assert_eq!((p.position.x, p.at_ms), (75., 1015.));
        assert!(p.current_contact);
        sim.advance(1020., Clock::Seekable);
        assert!(sim.frame_pointer(1).unwrap().current_contact);
        sim.input(pointer(1, PointerPhase::Cancel, 75., 1021.));
        assert!(sim.frame_pointer(1).unwrap().current_contact);
        sim.advance(1030., Clock::Seekable);
        assert!(sim.frame_pointer(1).unwrap().current_contact);
        sim.input(pointer(1, PointerPhase::Move, 200., 1031.));
        assert!(!sim.frame_pointer(1).unwrap().current_contact);
        sim.advance(1040., Clock::Seekable);
        assert!(!sim.frame_pointer(1).unwrap().current_contact);
        sim.input(pointer(1, PointerPhase::Move, 201., 1041.));
        sim.advance(1050., Clock::Seekable);
        assert!(!sim.frame_pointer(1).unwrap().current_contact);
        sim.input(pointer(1, PointerPhase::Down, 80., 1051.));
        assert!(!sim.frame_pointer(1).unwrap().current_contact);
        sim.advance(1060., Clock::Seekable);
        assert!(sim.frame_pointer(1).unwrap().current_contact);
    }
}

#[test]
fn saved_future_input_invalidates_any_host_sample_received_after_load() {
    let mut sim = Sim::<Fingers>::new(()).unwrap();
    sim.advance(1000., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Down, 70., 1001.));
    sim.advance(1010., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Cancel, 70., 1051.));
    sim.input(pointer(1, PointerPhase::Down, 200., 1052.));
    let saved = sim.save().unwrap();
    sim.restore(&saved).unwrap();
    sim.advance(2000., Clock::Seekable);
    sim.input(pointer(1, PointerPhase::Move, 75., 2001.));
    sim.advance(2010., Clock::Seekable);
    assert!(sim.frame_pointer(1).unwrap().current_contact);
    sim.advance(2060., Clock::Seekable);
    assert!(sim.frame_pointer(1).is_none());
    sim.input(pointer(1, PointerPhase::Move, 202., 2061.));
    assert!(!sim.frame_pointer(1).unwrap().current_contact);
    sim.advance(2070., Clock::Seekable);
    assert!(sim.frame_pointer(1).unwrap().current_contact);
}

#[test]
fn queued_terminal_edges_and_blur_follow_stamp_order_before_a_tick() {
    for blur in [false, true] {
        let mut sim = Sim::<Fingers>::new(()).unwrap();
        sim.advance(1000., Clock::Seekable);
        sim.input(pointer(1, PointerPhase::Down, 70., 1001.));
        sim.advance(1010., Clock::Seekable);
        if blur {
            sim.input(InputEvent::Blur {
                at_ms: 1030.,
                player: 0,
            });
            sim.input(pointer(1, PointerPhase::Move, 75., 1020.));
            assert!(sim.frame_pointer(1).is_none());
        } else {
            sim.input(pointer(1, PointerPhase::Move, 80., 1030.));
            sim.input(pointer(1, PointerPhase::Up, 75., 1020.));
            let p = sim.frame_pointer(1).unwrap();
            assert_eq!(
                (p.position.x, p.at_ms, p.phase),
                (80., 1030., PointerPhase::Move)
            );
            assert!(!p.current_contact);
        }
    }
}
