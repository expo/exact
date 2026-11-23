use contract_syntax::Step;

#[test]
fn a_test_drags_through_waypoints_as_the_driver_does() {
    // #1333: a hold, then legs with their own travel, holds and reads, and
    // the keys held, written as `agent`'s `tap … drag` takes them.
    let src = "test \"hold then drag\"\n  tap \"stage\" drag 100 0 from 50 50 press 500 modifiers \"Shift\" over 200 during \"state\" then 0 80 over 150 hold 100 during \"clock +50\" then -10 0\n";
    let tests = contract::tests(src).unwrap();
    let Step::Drag {
        press,
        modifiers,
        during,
        path,
        ..
    } = &tests[0].steps[0]
    else {
        panic!("{:?}", tests[0].steps[0])
    };
    assert_eq!(
        (*press, modifiers.as_str(), during.as_slice()),
        (Some(500.0), "Shift", ["state".to_string()].as_slice())
    );
    assert_eq!(path.len(), 2);
    assert_eq!(
        (path[0].dx, path[0].dy, path[0].over, path[0].hold),
        (0.0, 80.0, Some(150.0), Some(100.0))
    );
    assert_eq!(path[0].during, ["clock +50"]);
    assert_eq!(
        (path[1].dx, path[1].over, path[1].hold),
        (-10.0, None, None)
    );
    let json = contract::tests_json(&tests);
    assert!(
        json.contains("\"press\":500,\"over\":200,\"during\":[\"state\"],\"modifiers\":\"Shift\",\"path\":[{\"dx\":0,\"dy\":80,\"over\":150,\"hold\":100,\"during\":[\"clock +50\"]},{\"dx\":-10,\"dy\":0}]"),
        "{json}"
    );
    // As the driver reads it, a leg's reads may come before its hold.
    contract::tests("test \"t\"\n  tap \"a\" drag 1 0 during \"state\" hold 10\n").unwrap();
    // A leg takes its own travel, hold and reads; the press, start, button and keys are the drag's.
    for (line, says) in [
        (
            "drag 1 0 then 0 1 press 100".to_string(),
            "`press` is the drag's",
        ),
        (
            "drag 1 0 then 0 1 from 1 1".to_string(),
            "`from` is the drag's",
        ),
        (
            "drag 1 0 then 0 1 over 10 over 20".to_string(),
            "given twice in one leg",
        ),
        (
            "drag to \"b\" then 0 1".to_string(),
            "`drag to` takes no `then`",
        ),
        (format!("drag 1 0{}", " then 1 0".repeat(8)), "at most 7"),
    ] {
        let e = contract::tests(&format!("test \"t\"\n  tap \"a\" {line}\n")).unwrap_err();
        assert!(e.message.contains(says), "{line}: {}", e.message);
    }
}
