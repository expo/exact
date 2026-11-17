//! Raw observations of native targets (LLP 1123 D9, "The native chain"),
//! driven through the kernel's leaf before lowering produces it.
use super::*;
use crate::style::targets::{ChainEntry, Kind, NativeTarget, NativeTargets, Relation};

fn native(id: u64, top: f32) -> NativeTarget {
    NativeTarget {
        id,
        generation: 1,
        insets: Some([top, 0.0, 0.0, 0.0]),
    }
}

fn entry(id: u64, top: f32, kind: Option<Kind>) -> ChainEntry {
    ChainEntry {
        target: native(id, top),
        kind,
    }
}

/// A report whose chain is one controller, `top` at the top.
fn controller(id: u64, top: f32) -> NativeTargets {
    NativeTargets {
        chain: vec![entry(id, top, None)],
        relations: Vec::new(),
    }
}

/// `id` pads its top by `target`'s top inset, 7 where that is unavailable.
fn reads(id: ViewId, target: Target) -> Op {
    let pad = vec![(StyleId::PaddingTop, observe(target, Edge::Top, Some(7.0)))];
    style(id, pad, &[(StyleId::Height, "10")])
}

#[test]
fn a_reader_reads_its_owners_targets_by_number_kind_and_relation() {
    // The owner 2's chain: its controller (10), a navigation controller (20),
    // a tab bar controller (30) and an unattached column; its window (44)
    let readers = [
        (3, Target::Chain(2), 20.0),
        (4, Target::Kind(Kind::Owner), 10.0),
        (5, Target::Kind(Kind::Parent), 20.0),
        (6, Target::Kind(Kind::Tab), 30.0),
        (7, Target::Relation(Relation::Window), 44.0),
        // Absent or unavailable: the fallback
        (8, Target::Kind(Kind::Split), 7.0),
        (9, Target::Chain(9), 7.0),
        (10, Target::Relation(Relation::Presenter), 7.0),
        (11, Target::Kind(Kind::Column), 7.0),
        // The nearest of two navigation controllers
        (12, Target::Kind(Kind::Navigation), 20.0),
    ];
    let mut ops = create(3..=13);
    ops.extend(
        readers
            .iter()
            .map(|(id, target, _)| reads(*id, target.clone())),
    );
    // 13 reads the controller in `var(--gap, var(--exact-native-1-…))`'s fallback
    let gap = Node::Var(calc::Var {
        name: "--gap".into(),
        fallback: Some(Box::new(observe(Target::Chain(1), Edge::Top, None))),
    });
    let pad = vec![(StyleId::PaddingTop, gap)];
    ops.push(style(13, pad, &[(StyleId::Height, "10")]));
    let held: Vec<ViewId> = (3..=13).collect();
    ops.extend([children(1, &[2]), children(2, &held)]);
    let mut k = owner(390.0, 844.0, ops);
    let column = ChainEntry {
        target: NativeTarget {
            insets: None,
            ..native(4, 0.0)
        },
        kind: Some(Kind::Column),
    };
    let targets = NativeTargets {
        chain: vec![
            entry(1, 10.0, None),
            entry(2, 20.0, Some(Kind::Navigation)),
            entry(3, 30.0, Some(Kind::Tab)),
            column,
            entry(6, 60.0, Some(Kind::Navigation)),
        ],
        relations: vec![(Relation::Window, native(5, 44.0))],
    };
    assert!(
        k.set_owner_targets(2, targets).unwrap(),
        "its readers derived again"
    );
    lay_out(&mut k);
    for (id, target, want) in readers {
        assert_eq!(top(&k, id), want, "{target:?}");
    }
    assert_eq!(top(&k, 13), 10.0, "a custom property's fallback");
    assert_eq!(
        k.arena.observations.rounds, 0,
        "a raw reader needs no solve"
    );
}

#[test]
fn a_box_outside_every_owner_reads_the_session_views_targets() {
    // 3 is outside every owner, and 5 is in the popover 4 in the owner 2:
    // each reads its chain's first entry, the session view's controller
    // (50), not the owner's (10); the popover joins the top layer at the
    // layout
    let mut ops = create(3..=5);
    let owner_entry = Target::Kind(Kind::Owner);
    ops.extend([
        reads(3, owner_entry.clone()),
        reads(5, owner_entry),
        popover(4),
    ]);
    ops.extend([children(1, &[2, 3]), children(2, &[4]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    k.set_session_targets(controller(2, 50.0)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 50.0);
    assert!(k.is_top_layer(4));
    assert_eq!(top(&k, 5), 50.0);
}

#[test]
fn a_holder_change_derives_only_the_readers_whose_holder_it_changes() {
    // The owner 2 holds the reader 5 and the owner 3, which holds the
    // reader 4; when 2 joins the top layer, keeping its block, only 5's
    // holder changes, to the session
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    let owner_entry = Target::Kind(Kind::Owner);
    ops.extend([reads(4, owner_entry.clone()), reads(5, owner_entry)]);
    ops.extend([children(1, &[2]), children(2, &[3, 5]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    k.set_owner_targets(3, controller(2, 30.0)).unwrap();
    k.set_session_targets(controller(3, 50.0)).unwrap();
    lay_out(&mut k);
    assert_eq!((top(&k, 4), top(&k, 5)), (30.0, 10.0));
    assert_eq!(k.held_through(slot(&k, 2)), vec![slot(&k, 5)]);
    k.apply(0, 2, &[popover(2)]).unwrap();
    lay_out(&mut k);
    assert_eq!((top(&k, 4), top(&k, 5)), (30.0, 50.0));
}

#[test]
fn a_reader_moved_into_another_owner_reads_its_targets() {
    // 4 moves from the owner 2 (10) into the owner 3 (30)
    let mut ops = create(3..=4);
    ops.push(reads(4, Target::Kind(Kind::Owner)));
    ops.extend([children(1, &[2, 3]), children(2, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
    k.set_host_block(3, Some(block)).unwrap();
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    k.set_owner_targets(3, controller(2, 30.0)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 10.0);
    k.apply(0, 2, &[children(2, &[]), children(3, &[4])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 30.0);
}

#[test]
fn a_box_given_a_block_gives_its_readers_its_targets() {
    // 3, in the owner 2 (10), holds the reader 4; 3's report (30) applies
    // while 3 holds a block of its own
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    ops.push(reads(4, Target::Kind(Kind::Owner)));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    let held = k.set_owner_targets(3, controller(2, 30.0)).unwrap();
    assert!(!held, "3 holds no reader while it has no block");
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 10.0);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 30.0);
    let held = k.set_owner_targets(2, controller(1, 15.0)).unwrap();
    assert!(!held, "2's report reaches no reader 3 holds");
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 15.0, "2's report as it now is");
}

#[test]
fn a_registered_target_reads_anywhere_and_follows_its_replacement() {
    // 3 in the owner 2 and 4 outside every owner read the target
    // registered as `map`
    let map = Target::Registered("map".into());
    let mut ops = create(3..=4);
    ops.extend([reads(3, map.clone()), reads(4, map)]);
    ops.extend([children(1, &[2, 4]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let view = |id, generation, top| {
        Some(NativeTarget {
            id,
            generation,
            insets: Some([top, 0.0, 0.0, 0.0]),
        })
    };
    assert!(k.set_registered_target("map", view(1, 1, 12.0)).unwrap());
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 4)), (12.0, 12.0));
    // Replaced by another view, with a newer generation
    k.set_registered_target("map", view(2, 2, 24.0)).unwrap();
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 4)), (24.0, 24.0));
    let stale = k.set_registered_target("map", view(1, 1, 12.0));
    assert!(stale.is_err(), "a report from the view it replaced");
    let unchanged = k.set_registered_target("map", view(3, 2, 30.0));
    assert!(
        unchanged.is_err(),
        "another view at the generation it replaces"
    );
    let right = NativeTarget {
        insets: Some([24.0, 9.0, 0.0, 0.0]),
        ..view(2, 2, 0.0).unwrap()
    };
    assert!(
        !k.set_registered_target("map", Some(right)).unwrap(),
        "an edge no reader reads"
    );
    // Removed, its readers take their fallback, and its generation stays
    k.set_registered_target("map", None).unwrap();
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 4)), (7.0, 7.0));
    assert!(
        k.set_registered_target("map", view(2, 1, 12.0)).is_err(),
        "stale"
    );
    k.set_registered_target("map", view(4, 3, 36.0)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 36.0);
    for name in ["Map", "-map", "map-", ""] {
        assert!(
            k.set_registered_target(name, view(3, 3, 1.0)).is_err(),
            "{name:?}"
        );
    }
}

#[test]
fn a_report_derives_only_the_readers_of_the_targets_it_moves() {
    // The owners 2 and 5 side by side; 3, in 2, reads its navigation
    // controller's top and left
    let navigation = |edge| observe(Target::Kind(Kind::Navigation), edge, Some(7.0));
    let pads = vec![
        (StyleId::PaddingTop, navigation(Edge::Top)),
        (StyleId::PaddingLeft, navigation(Edge::Left)),
    ];
    let mut ops = create(3..=5);
    ops.push(style(3, pads, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2, 5]), children(2, &[3])]);
    let mut k = owner(390.0, 400.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(400.0));
    k.set_host_block(5, Some(block)).unwrap();
    let report = |navigation, window| NativeTargets {
        chain: vec![
            entry(1, 0.0, None),
            entry(2, navigation, Some(Kind::Navigation)),
        ],
        relations: vec![(Relation::Window, native(3, window))],
    };
    assert!(k.set_owner_targets(2, report(20.0, 44.0)).unwrap());
    lay_out(&mut k);
    // A window 3 does not read, and another owner's navigation controller,
    // owe no layout
    assert!(!k.set_owner_targets(2, report(20.0, 50.0)).unwrap());
    assert!(!k.set_owner_targets(5, report(30.0, 44.0)).unwrap());
    assert!(k.set_owner_targets(2, report(25.0, 50.0)).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 25.0);
    let edged = |right, left| {
        let mut r = report(25.0, 50.0);
        r.chain[1].target.insets = Some([25.0, right, 0.0, left]);
        r
    };
    assert!(
        !k.set_owner_targets(2, edged(5.0, 0.0)).unwrap(),
        "an edge 3 does not read"
    );
    assert!(
        k.set_owner_targets(2, edged(5.0, 6.0)).unwrap(),
        "the other edge 3 reads"
    );
    lay_out(&mut k);
    assert_eq!(left(&k, 3), 6.0);
}

#[test]
fn a_report_finds_its_readers_in_an_owner_of_many_boxes() {
    // The owner 2 holds ten boxes, the reader 13 and the owner 15, which
    // holds the reader 16, more than its walk down takes for its readers;
    // 14 reads outside every owner
    let mut ops = create(3..=16);
    let first = Target::Chain(1);
    ops.extend([13, 14, 16].map(|id| reads(id, first.clone())));
    let mut held: Vec<ViewId> = (3..=13).collect();
    held.push(15);
    ops.extend([
        children(1, &[2, 14]),
        children(2, &held),
        children(15, &[16]),
    ]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(15, Some(block)).unwrap();
    assert_eq!(k.held_through(slot(&k, 2)), vec![slot(&k, 13)]);
    assert!(k.set_owner_targets(2, controller(1, 10.0)).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 13), 10.0);
}

#[test]
fn a_reader_with_no_fallback_follows_its_targets_availability() {
    // 3 reads the chain's first entry with no fallback: invalid at computed
    // value time while it is unavailable, so its padding is the initial 0
    let mut ops = create(3..=3);
    let pad = vec![(
        StyleId::PaddingTop,
        observe(Target::Chain(1), Edge::Top, None),
    )];
    ops.push(style(3, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 0.0);
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 10.0);
    let unattached = ChainEntry {
        target: NativeTarget {
            insets: None,
            ..native(1, 0.0)
        },
        kind: None,
    };
    let report = NativeTargets {
        chain: vec![unattached],
        relations: Vec::new(),
    };
    assert!(k.set_owner_targets(2, report).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 0.0);
}

#[test]
fn a_report_is_refused_whole() {
    let mut ops = create(3..=3);
    ops.push(reads(3, Target::Chain(1)));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_targets(2, controller(1, 10.0)).unwrap();
    lay_out(&mut k);
    let older = ChainEntry {
        target: NativeTarget {
            generation: 0,
            ..native(1, 20.0)
        },
        kind: None,
    };
    let refused = [
        NativeTargets {
            chain: vec![entry(1, 20.0, Some(Kind::Owner))],
            relations: Vec::new(),
        },
        NativeTargets {
            chain: vec![entry(1, 20.0, None)],
            relations: vec![
                (Relation::Window, native(2, 1.0)),
                (Relation::Window, native(3, 1.0)),
            ],
        },
        controller(1, f32::NAN),
        controller(1, -1.0),
        NativeTargets {
            chain: vec![older],
            relations: Vec::new(),
        },
        NativeTargets {
            chain: vec![entry(9, 20.0, None); 256],
            relations: Vec::new(),
        },
    ];
    for report in refused {
        let result = k.set_owner_targets(2, report.clone());
        assert!(
            matches!(
                result,
                Err(KernelError::Layout(LayoutError::NativeTarget(_)))
            ),
            "{report:?}"
        );
    }
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 10.0, "the report it holds");
    // The entry leaves, and its older report, coming late, is still stale
    k.set_owner_targets(2, NativeTargets::default()).unwrap();
    let late = NativeTargets {
        chain: vec![older],
        relations: Vec::new(),
    };
    assert!(k.set_owner_targets(2, late).is_err());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0, "the fallback, the entry gone");
}

#[test]
fn the_leaf_names_its_target_as_an_author_writes_it() {
    let cases = [
        (Target::Chain(2), "native-2-"),
        (Target::Kind(Kind::Navigation), "navigation-"),
        (Target::Relation(Relation::TabContent), "tab-content-"),
        (Target::Registered("map".into()), "target-map-"),
    ];
    for (target, name) in cases {
        let mut css = String::new();
        let leaf = observe(target, Edge::Top, Some(7.0));
        calc::Comparison::intern(leaf).css(&mut css);
        assert_eq!(css, format!("var(--exact-{name}safe-area-inset-top, 7px)"));
    }
}
