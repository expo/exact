use super::*;

#[test]
fn a_snap_kind_change_and_same_batch_scroll_work_with_a_content_region() {
    let _service = crate::content_region::test_service();
    for same_batch in [false, true] {
        let source = format!(
            r#"component App
  state enabled = false
  action enable
    enabled = true
  view
    column width=400 height=600
      button "Enable" press=enable testId="enable"
      scroll id="pager" testId="pager" width=400 height=200 overflow-x="scroll" overflow-y="hidden" display="flex" scrollLeft={} scroll-snap-type=(enabled ? "x mandatory" : "none")
        for i in [1, 2, 3] key=i
          box width=400 height=200 flex-shrink=0 scroll-snap-align="start"
            text "Page"
      view id="owner" width=400 height=100 overflow="hidden"
        view id="content" width=400 height=100
          text "Region content"
        text "Preparing" id="pending" position="absolute"
"#,
            if same_batch {
                "(enabled ? 260 : 0)"
            } else {
                "0"
            }
        );
        let (mut p, error) = Presenter::boot_region_with_fixture_fonts(
            &crate::test_plan(&source),
            (),
            (400., 600.),
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
        assert_eq!(error, None);
        p.frame();
        ready(&mut p);
        let pager = id(&p, "pager");
        if !same_batch {
            p.scroll_to(pager, (260., 0.));
            p.frame();
            assert_eq!(p.scroll_of(pager).0, 260.);
        }
        p.tap(id(&p, "enable")).unwrap();
        let frame = p.display_frame().unwrap();
        assert!(p.display_complete(&frame));
        assert_eq!(p.box_of(pager).unwrap().scroll, Some((400., 0.)));
        assert!(!p.snap_probe_needed());
        drop(p);
        crate::content_region::test_wait_idle();
    }
}
