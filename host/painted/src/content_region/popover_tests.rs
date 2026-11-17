use super::*;

#[test]
fn a_region_recaptures_while_its_popover_is_open() {
    const CHILD: &str = "EXACT_REGION_POPOVER_TEST";
    if std::env::var_os(CHILD).is_none() {
        let name = std::thread::current().name().unwrap().to_owned();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    crate::app::show_neutral_look();
    let _service = crate::content_region::test_service();
    let source = APP.replace(
        "              text title href=link",
        "              button testId=\"open\" popovertarget=\"pop\" height=30\n                text \"Open\"\n              column id=\"pop\" testId=\"pop\" popover=\"manual\" width=160 height=50\n                text \"Popover\"\n              text title href=link",
    );
    let (mut p, error) = Presenter::boot_with_content_region(
        &crate::test_plan(&source),
        (),
        (400., 500.),
        1.,
        PathBuf::new(),
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
    p.host.top_layer(true);
    p.frame();
    ready(&mut p);
    let first = p.display_frame().unwrap();
    assert!(p.display_complete(&first));
    p.tap(id(&p, "open")).unwrap();
    p.run_commands(|| ());
    p.frame();
    assert_eq!(p.alert_root(), Some(id(&p, "pop")));
    let publication = p
        .host
        .content_region()
        .unwrap()
        .receipt()
        .unwrap()
        .selection
        .clone();
    assert!(p
        .host
        .dispatch_at(id(&p, "replace"), Event::Press, 1.)
        .is_none());
    assert!(p.after_commit().is_none());
    ready(&mut p);
    let next = p.display_frame().unwrap();
    assert!(
        p.last_frame_succeeded,
        "the new publication must capture without the open popover"
    );
    assert!(p.display_complete(&next));
    assert!(p.rect_of(id(&p, "pop")).is_some());
    let exact_kernel::RegionSelection::Accepted(old) = publication else {
        panic!("accepted")
    };
    let exact_kernel::RegionSelection::Accepted(new) = &p
        .host
        .content_region()
        .unwrap()
        .receipt()
        .unwrap()
        .selection
    else {
        panic!("accepted")
    };
    assert!(!std::rc::Rc::ptr_eq(&old, new));
}
