use super::*;

#[test]
fn retained_inline_opacity_uses_the_accepted_source() {
    let _service = crate::content_region::test_service();
    let draw = |opacity: f32, paint: &str| {
        crate::content_region::test_wait_idle();
        let source = format!(
            "component App\n  state changed = false\n  action replace\n    changed = true\n  view\n    view id=\"owner\" width=400 height=200 overflow-x=\"hidden\" overflow-y=\"hidden\"\n      view id=\"content\" width=400 height=200\n        text font-size=32 color=\"#000000\" opacity=0.5 {paint} testId=\"paragraph\" press=replace\n          text opacity=0.5\n            text (changed ? \"new paragraph\" : \"HHHH\") opacity=(changed ? 1 : {opacity})\n      text \"pending\" id=\"pending\" position=\"absolute\"\n"
        );
        let (mut p, error) = Presenter::boot_with_content_region(
            &crate::test_plan(&source),
            (),
            (400., 200.),
            1.,
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
            PainterChoice::Cpu,
            ContentRegionRegistration {
                activate: None,
                owner: "owner",
                content: "content",
                pending: "pending",
            },
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        ready(&mut p);
        let original = p.frame().clone();
        let (mut live, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (400., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        assert!(
            original.data() == live.frame().data(),
            "retained paint differs for {paint}"
        );
        let gate = crate::content_region::test_hooks::next_text();
        assert!(p
            .host
            .dispatch_at(id(&p, "paragraph"), Event::Press, 1.)
            .is_none());
        assert!(p.after_commit().is_none());
        gate.entered();
        assert_eq!(
            p.frame().data(),
            original.data(),
            "pending opacity must not change accepted ink"
        );
        drop(gate);
        ready(&mut p);
        original
    };
    for paint in [
        "",
        "text-decoration-line=\"underline\"",
        "text-shadow=\"0px 40px 0px red\"",
    ] {
        let full = draw(1., paint);
        assert!(full.data().chunks_exact(4).any(|p| p != [255; 4]));
        for opacity in [0., 0.5] {
            let actual = draw(opacity, paint);
            for (a, s) in actual.data().iter().zip(full.data()) {
                let expected = (255. - f32::from(255 - s) * opacity).round() as u8;
                assert!(
                    a.abs_diff(expected) <= 2,
                    "opacity {opacity}: {a} instead of {expected}"
                );
            }
        }
    }
}

#[test]
fn retained_inline_opacity_is_the_presented_opacity() {
    let _service = crate::content_region::test_service();
    crate::content_region::test_wait_idle();
    // At 0 ms the fill holds the run at opacity 0; its style says 1.
    let source = "keyframes appear\n  from opacity=0\n  to opacity=1\ncomponent App\n  view\n    view id=\"owner\" width=400 height=200 overflow-x=\"hidden\" overflow-y=\"hidden\"\n      view id=\"content\" width=400 height=200\n        text font-size=32 color=\"#000000\"\n          text opacity=0.5\n            text \"HHHH\" animation=\"appear 1s linear both\"\n      text \"pending\" id=\"pending\" position=\"absolute\"\n";
    let (mut p, error) = Presenter::boot_with_content_region(
        &crate::test_plan(source),
        (),
        (400., 200.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    ready(&mut p);
    let retained = p.frame().clone();
    let (mut live, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let live = live.frame().clone();
    assert!(
        live.data().chunks_exact(4).all(|p| p == [255; 4]),
        "the live painter shows the run at its presented opacity, 0"
    );
    assert!(
        retained.data() == live.data(),
        "the retained paragraph takes the presented opacity"
    );
}
