use exact_painted::gamepad::{read, Change, Fetch, Layout, Opened, State};
use exact_painted::presenter::{PainterChoice, Presenter};
use serde_json::{json, Value};

#[test]
fn delayed_pad_reports_tap_and_hold_through_the_presenter() {
    let module = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "{}shortcut_fixture_gpu{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX,
        ));
    let mut compat: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/compat.json"))).unwrap();
    compat["embedded"]["gpu"] = json!({
        "app": compat["inputs"]["app"],
        "cohort": compat["id"],
        "name": module,
        "trust": "development",
        "sha256": exact_update::gpu_code_digest(&std::fs::read(&module).unwrap()),
    });
    // A new pad announces neutral first; an already announced pad can select
    // its first canvas with the press itself.
    for (release, taps, holds, announced) in [
        (250., 1, 0, false),
        (310., 0, 1, false),
        (250., 1, 0, true),
        (310., 0, 1, true),
    ] {
        let (mut presenter, error) = Presenter::boot_with(
            include_bytes!(concat!(env!("OUT_DIR"), "/app.plan")),
            (),
            (640., 480.),
            1.,
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        assert_eq!(
            presenter.set_delivery_from_compat(&compat.to_string()),
            None
        );
        let ready = exact_painted::agent::handle(&mut presenter, r#"{"op":"state"}"#);
        presenter.sync_surfaces();
        assert!(presenter.has_gamepad_input(), "{ready}");
        assert_eq!(
            presenter.focus(),
            None,
            "the first report selects the owner"
        );
        assert_eq!(presenter.clock(1500.).1, None);
        let mut pads = vec![(
            (),
            Opened {
                index: 0,
                state: State::new([None; 8], Layout::Positional),
                announced,
            },
        )];
        // Both edges arrive at one late read. Their device times, translated
        // by the reader's clock origin, must still span 150 ms or 210 ms.
        let mut fetched = Some(Fetch::Changes(vec![
            (Change::Key(0x131, 1), 100.),
            (Change::Report, 100.),
            (Change::Key(0x131, 0), release),
            (Change::Report, release),
        ]));
        for (index, reading, at) in read(&mut pads, (500., 1500.), |_| fetched.take().unwrap()) {
            let reading = reading.unwrap();
            presenter.hardware_pad(index, true, reading.buttons, reading.axes, at);
        }
        let reply: Value = serde_json::from_str(&exact_painted::agent::handle(
            &mut presenter,
            r#"{"op":"clock","to":1600}"#,
        ))
        .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        let state: Value = serde_json::from_str(&exact_painted::agent::handle(
            &mut presenter,
            r#"{"op":"state"}"#,
        ))
        .unwrap();
        let worlds = state["world"]
            .as_array()
            .expect("the game's surface is mounted");
        assert_eq!(worlds.len(), 1, "{state}");
        let shop = &worlds[0]["resources"]["Shop"];
        assert_eq!(
            (shop["ones"].clone(), shop["jumps"].clone()),
            (json!(taps), json!(holds)),
            "released at {release}, announced={announced}: {shop}"
        );
    }
}
