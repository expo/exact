use super::{support::*, *};

#[test]
fn region_receipt_carries_resolved_exclusions_with_selected_frames() {
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        for external in [false, true] {
            for padded in [false, true] {
                region_exclusions(profile, external, padded);
            }
        }
    }
}
fn region_exclusions_fixture(profile: RegionProfile, external: bool, padded: bool) -> Kernel {
    let mut k = fixture();
    let mut paragraph = StyleProps::default();
    paragraph.width = Dimension::Points(400.);
    paragraph.height = Dimension::Points(180.);
    paragraph.mask = mask(&[StyleId::Width, StyleId::Height]);
    if padded {
        paragraph.padding_left = Dimension::Percent(10.);
        paragraph.padding_top = Dimension::Points(10.);
        paragraph.border_width_left = 3.;
        paragraph.border_width_top = 5.;
        paragraph.border_style_left = BorderStyle::Solid;
        paragraph.border_style_top = BorderStyle::Solid;
        for id in [
            StyleId::PaddingLeft,
            StyleId::PaddingTop,
            StyleId::BorderWidthLeft,
            StyleId::BorderWidthTop,
            StyleId::BorderStyleLeft,
            StyleId::BorderStyleTop,
        ] {
            paragraph.mask.set(id);
        }
    }
    let mut ball = StyleProps::default();
    ball.width = Dimension::Points(120.);
    ball.height = Dimension::Points(120.);
    ball.top = Dimension::Points(0.);
    ball.left = Dimension::Points(60.);
    ball.position_type = PositionType::Absolute;
    ball.wrap_flow = WrapFlow::Both;
    ball.rare.shape_outside = ShapeOutside::parse("circle()").unwrap();
    ball.mask = mask(&[
        StyleId::Width,
        StyleId::Height,
        StyleId::Top,
        StyleId::Left,
        StyleId::PositionType,
        StyleId::WrapFlow,
        StyleId::ShapeOutside,
    ]);
    k.apply(
        0,
        0,
        &[
            Op::SetStyle {
                id: 4,
                patch: Box::new(paragraph),
            },
            Op::CreateView {
                id: 7,
                node_type: NodeType::View,
            },
            Op::SetStyle {
                id: 7,
                patch: Box::new(ball),
            },
            Op::SetChildren {
                id: 3,
                children: vec![4, 7],
            },
        ],
    )
    .unwrap();
    if external {
        k.apply(
            0,
            0,
            &[
                Op::SetChildren {
                    id: 3,
                    children: vec![4],
                },
                Op::CreateView {
                    id: 8,
                    node_type: NodeType::Image,
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2, 6, 7, 8],
                },
            ],
        )
        .unwrap();
    }
    k.set_content_region_profile(Some(binding(&k)), profile)
        .unwrap();
    k
}
fn region_exclusions(profile: RegionProfile, external: bool, padded: bool) {
    let mut k = region_exclusions_fixture(profile, external, padded);
    let (inset_x, inset_y) = if padded { (43., 15.) } else { (0., 0.) };
    let shapes = |cx: f32, inset_y: f32| {
        [FlowShape::Circle {
            cx: cx - inset_x,
            cy: 60. - inset_y,
            r: 60.,
        }]
    };
    let initial = ready(&mut k, 400., 1);
    assert_eq!(
        k.node(4).unwrap().flow_shapes(),
        &[FlowShape::Circle {
            cx: 120.,
            cy: 60.,
            r: 60.
        }]
    );
    assert_eq!(initial.shell.flow_changed, vec![key(&k, 4)]);
    let RegionSelection::Accepted(publication) = &initial.selection else {
        panic!("expected accepted publication");
    };
    let artifact = publication.paint_artifact(key(&k, 4)).unwrap();
    artifact.request().with_request(|request| {
        assert_eq!(request.exclusions, &shapes(120., inset_y));
    });
    let mut moved = StyleProps::default();
    moved.left = Dimension::Points(80.);
    moved.mask = mask(&[StyleId::Left]);
    k.apply(
        0,
        0,
        &[Op::SetStyle {
            id: 7,
            patch: Box::new(moved),
        }],
    )
    .unwrap();
    let waiting = pass(&mut k, 400., 1);
    assert!(
        !waiting.current,
        "moving a shape needs a fresh paint artifact"
    );
    let stale = k.region_text_request().unwrap().clone();
    stale.with_request(|request| {
        assert_eq!(request.exclusions, &shapes(140., inset_y));
    });
    let mut moved = StyleProps::default();
    moved.left = Dimension::Points(100.);
    moved.mask = mask(&[StyleId::Left]);
    k.apply(
        0,
        0,
        &[Op::SetStyle {
            id: 7,
            patch: Box::new(moved),
        }],
    )
    .unwrap();
    assert!(!k
        .resolve_region_text(&stale, TextMetrics::default(), Rc::new(()))
        .unwrap());
    drop(stale);
    // The selected publication owns its original geometry while the new one waits.
    artifact.request().with_request(|request| {
        assert_eq!(request.exclusions, &shapes(120., inset_y));
    });
    let r = ready(&mut k, 400., 1);
    let RegionSelection::Accepted(next) = &r.selection else {
        panic!("expected accepted publication")
    };
    let next = next.paint_artifact(key(&k, 4)).unwrap();
    assert_eq!(next.request().stamp(), artifact.request().stamp());
    assert!(std::sync::Arc::ptr_eq(
        next.request().source(),
        artifact.request().source()
    ));
    next.request().with_request(|request| {
        assert_eq!(request.exclusions, &shapes(160., inset_y));
    });
    if !external {
        assert_eq!(r.shell.flow_changed, vec![key(&k, 4)]);
    }
    assert_eq!(
        k.node(4).unwrap().flow_shapes(),
        &[FlowShape::Circle {
            cx: 160.,
            cy: 60.,
            r: 60.,
        }]
    );
    assert!(!r.shell.changed.contains(&key(&k, 4)));
    drop(waiting);
    drop(initial);
    if padded {
        let mut previous = None;
        for top in [30., 50.] {
            let mut padding = StyleProps::default();
            padding.padding_top = Dimension::Points(top);
            padding.mask = mask(&[StyleId::PaddingTop]);
            k.apply(
                0,
                0,
                &[Op::SetStyle {
                    id: 4,
                    patch: Box::new(padding),
                }],
            )
            .unwrap();
            if let Some(stale) = previous.take() {
                assert!(!k
                    .resolve_region_text(&stale, TextMetrics::default(), Rc::new(()))
                    .unwrap());
            }
            let waiting = pass(&mut k, 400., 1);
            assert!(!waiting.current, "inset changes need fresh paint geometry");
            let stale = k.region_text_request().unwrap().clone();
            assert_eq!(stale.stamp(), next.request().stamp());
            assert_eq!(stale.offer(), next.request().offer());
            assert_eq!(stale.exclusions(), &shapes(160., top + 5.));
            previous = Some(stale);
        }
        let accepted = ready(&mut k, 400., 1);
        let RegionSelection::Accepted(current) = accepted.selection else {
            panic!("expected accepted publication")
        };
        assert_eq!(
            current
                .paint_artifact(key(&k, 4))
                .unwrap()
                .request()
                .exclusions(),
            &shapes(160., 55.)
        );
    }
}

#[test]
fn shell_exclusions_preserve_unrelated_commits() {
    shell_exclusions_preserve_unrelated(0);
}
#[test]
fn shell_exclusions_preserve_unrelated_intrinsic_measurements() {
    shell_exclusions_preserve_unrelated(1);
}
#[test]
fn shell_exclusions_preserve_unrelated_declined_symbols() {
    shell_exclusions_preserve_unrelated(2);
}
fn shell_exclusions_preserve_unrelated(change: usize) {
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        let mut k = region_exclusions_fixture(profile, true, false);
        let RegionSelection::Accepted(initial) = ready(&mut k, 400., 1).selection else {
            panic!("expected accepted publication")
        };
        let ticket = initial.ticket().clone();
        drop(initial);
        for pending in [false, true] {
            let request = if pending {
                let mut moved = StyleProps::default();
                moved.left = Dimension::Points(80.);
                moved.mask = mask(&[StyleId::Left]);
                k.apply(
                    0,
                    0,
                    &[Op::SetStyle {
                        id: 7,
                        patch: Box::new(moved),
                    }],
                )
                .unwrap();
                assert!(!pass(&mut k, 400., 1).current);
                Some(k.region_text_request().unwrap().clone())
            } else {
                None
            };
            match change {
                0 => {
                    k.apply(
                        0,
                        0,
                        &[Op::SetProp {
                            id: 6,
                            prop: PropId::Text,
                            value: format!("typed {pending}").into(),
                        }],
                    )
                    .unwrap();
                }
                1 => k
                    .set_intrinsic_size(8, Some((if pending { 30. } else { 20. }, 20.)))
                    .unwrap(),
                _ => {
                    k.apply(
                        0,
                        0,
                        &[Op::SetProp {
                            id: 8,
                            prop: PropId::ImageSource,
                            value: format!("sf/unmeasured-{pending}").into(),
                        }],
                    )
                    .unwrap();
                }
            }
            let receipt = pass(&mut k, 400., 1);
            assert_eq!(receipt.current, !pending, "{profile:?}: change {change}");
            if let Some(request) = &request {
                assert_eq!(k.region_text_request().unwrap().ticket(), request.ticket());
            } else if let RegionSelection::Accepted(publication) = receipt.selection {
                assert_eq!(
                    publication.ticket(),
                    &ticket,
                    "{profile:?}: change {change}"
                );
            }
            if let Some(request) = request {
                let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
                assert!(k
                    .resolve_region_text(&request, metrics, Rc::new(()))
                    .unwrap());
                assert!(ready(&mut k, 400., 1).current);
            }
        }
    }
}

#[test]
fn shell_exclusion_dependencies_reject_pending_results_before_layout() {
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        for change in 0..5 {
            let mut k = region_exclusions_fixture(profile, true, false);
            ready(&mut k, 400., 1);
            let mut moved = StyleProps::default();
            moved.left = Dimension::Points(80.);
            moved.mask = mask(&[StyleId::Left]);
            k.apply(
                0,
                0,
                &[Op::SetStyle {
                    id: 7,
                    patch: Box::new(moved),
                }],
            )
            .unwrap();
            assert!(!pass(&mut k, 400., 1).current);
            let stale = k.region_text_request().unwrap().clone();
            match change {
                0 => k
                    .apply(0, 0, &[Op::DestroyView { id: 7 }])
                    .map(|_| ())
                    .unwrap(),
                1 | 2 => {
                    let mut patch = StyleProps::default();
                    patch.wrap_flow = WrapFlow::Auto;
                    patch.mask = mask(&[StyleId::WrapFlow]);
                    k.apply(
                        0,
                        0,
                        &[Op::SetStyle {
                            id: 7,
                            patch: Box::new(patch),
                        }],
                    )
                    .unwrap();
                    if change == 2 {
                        assert!(!k
                            .resolve_region_text(&stale, TextMetrics::default(), Rc::new(()))
                            .unwrap());
                        drop(stale);
                        // A newly admitted shape is a dependency even without a layout.
                        pass(&mut k, 400., 1);
                        let newly_pending = k.region_text_request().unwrap().clone();
                        let mut patch = StyleProps::default();
                        patch.wrap_flow = WrapFlow::Both;
                        patch.mask = mask(&[StyleId::WrapFlow]);
                        k.apply(
                            0,
                            0,
                            &[Op::SetStyle {
                                id: 7,
                                patch: Box::new(patch),
                            }],
                        )
                        .unwrap();
                        assert!(!k
                            .resolve_region_text(
                                &newly_pending,
                                TextMetrics::default(),
                                Rc::new(())
                            )
                            .unwrap());
                        continue;
                    }
                }
                3 => {
                    let mut patch = StyleProps::default();
                    patch.padding_left = Dimension::Points(10.);
                    patch.mask = mask(&[StyleId::PaddingLeft]);
                    k.apply(
                        0,
                        0,
                        &[Op::SetStyle {
                            id: 1,
                            patch: Box::new(patch),
                        }],
                    )
                    .unwrap();
                }
                _ => k
                    .set_host_cover(1, Some(HostCover::Edges([1.; 4])))
                    .unwrap(),
            }
            assert!(
                !k.resolve_region_text(&stale, TextMetrics::default(), Rc::new(()))
                    .unwrap(),
                "{profile:?}: change {change}"
            );
        }
    }
}

#[test]
fn exclusion_descendants_reject_pending_results_before_layout() {
    // An auto-height exclusion takes its height from an image inside it.
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        for change in 0..2 {
            let mut k = region_exclusions_fixture(profile, true, false);
            let mut auto = StyleProps::default();
            auto.height = Dimension::Auto;
            auto.mask = mask(&[StyleId::Height]);
            k.apply(
                0,
                0,
                &[
                    Op::SetStyle {
                        id: 7,
                        patch: Box::new(auto),
                    },
                    Op::CreateView {
                        id: 9,
                        node_type: NodeType::Image,
                    },
                    Op::SetChildren {
                        id: 7,
                        children: vec![9],
                    },
                ],
            )
            .unwrap();
            k.set_intrinsic_size(9, Some((120., 120.))).unwrap();
            ready(&mut k, 400., 1);
            let mut moved = StyleProps::default();
            moved.left = Dimension::Points(80.);
            moved.mask = mask(&[StyleId::Left]);
            k.apply(
                0,
                0,
                &[Op::SetStyle {
                    id: 7,
                    patch: Box::new(moved),
                }],
            )
            .unwrap();
            assert!(!pass(&mut k, 400., 1).current);
            let stale = k.region_text_request().unwrap().clone();
            if change == 0 {
                k.set_intrinsic_size(9, Some((120., 60.))).unwrap();
            } else {
                let mut short = StyleProps::default();
                short.height = Dimension::Points(60.);
                short.mask = mask(&[StyleId::Height]);
                k.apply(
                    0,
                    0,
                    &[Op::SetStyle {
                        id: 9,
                        patch: Box::new(short),
                    }],
                )
                .unwrap();
            }
            assert!(
                !k.resolve_region_text(&stale, TextMetrics::default(), Rc::new(()))
                    .unwrap(),
                "{profile:?}: change {change}"
            );
            drop(stale);
            let r = ready(&mut k, 400., 1);
            let RegionSelection::Accepted(publication) = &r.selection else {
                panic!("expected accepted publication")
            };
            publication
                .paint_artifact(key(&k, 4))
                .unwrap()
                .request()
                .with_request(|request| {
                    assert_eq!(
                        request.exclusions,
                        &[FlowShape::Circle {
                            cx: 140.,
                            cy: 30.,
                            r: 30.,
                        }],
                        "{profile:?}: change {change}"
                    );
                });
        }
    }
}
