//! LLP 1101.003 D9: a plan says which medium it was compiled for, and each
//! host's decoder takes only its own: a terminal plan is refused by every
//! GUI host, a GUI plan by the terminal.

use exact_plan::{Plan, PlanError};

const SOURCE: &str = "shape Note\n  text: string\ncomponent App\n  view\n    text \"Hello\"\n";

#[test]
fn every_compile_writes_its_surface_and_decoding_refuses_the_other() {
    let dir = std::env::temp_dir().join(format!("contract-plan-surface-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.contract");
    std::fs::write(&path, SOURCE).unwrap();
    let gui = [
        contract::compile(SOURCE).unwrap(),
        contract::compile_path(&path).unwrap(),
    ];
    let terminal = [
        contract::compile_terminal(SOURCE).unwrap(),
        contract::compile_path_terminal(&path).unwrap(),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    for plan in &gui {
        assert_eq!(plan.surface, "gui");
        let bytes = plan.encode();
        assert_eq!(Plan::decode(&bytes).unwrap().surface, "gui");
        assert_eq!(
            Plan::decode_terminal(&bytes).unwrap_err(),
            PlanError::WrongSurface {
                plan: "gui".into(),
                host: "terminal"
            }
        );
    }
    for plan in &terminal {
        assert_eq!(plan.surface, "terminal");
        let bytes = plan.encode();
        assert_eq!(Plan::decode_terminal(&bytes).unwrap().surface, "terminal");
        assert_eq!(
            Plan::decode(&bytes).unwrap_err(),
            PlanError::WrongSurface {
                plan: "terminal".into(),
                host: "gui"
            }
        );
        // The generators read a plan of either surface.
        assert!(contract::rust(plan, &Default::default())
            .unwrap()
            .contains("struct Note"));
        contract::typescript(plan).unwrap();
    }
}

/// `contract build --surface terminal`: a GUI source lowered for the
/// terminal, so the terminal host measures it (LLP 1121 stage 11 C), with
/// what a terminal entry refuses said as warnings; with no `--surface`, the
/// GUI plan.
#[test]
fn a_gui_source_builds_for_the_terminal_with_its_refusals_as_warnings() {
    let app = crate::diagnostics::App::new("surface");
    app.write(
        "app.contract",
        "component App\n  view\n    column width=120 padding=8\n      text \"Hello\"\n",
    );
    let built = app.run(&[
        "app.contract",
        "-o",
        "t.plan",
        "--surface",
        "terminal",
        "--json",
    ]);
    let said = crate::diagnostics::diagnostics(&built, 0);
    let refusals: Vec<&str> = (said.iter())
        .map(|d| d["id"].as_str().unwrap())
        .filter(|id| id.starts_with("terminal-"))
        .collect();
    assert_eq!(refusals, ["terminal-length", "terminal-length"], "{said:?}");
    assert!(said.iter().all(|d| d["severity"] == "warning"), "{said:?}");
    let bytes = std::fs::read(app.0.join("t.plan")).unwrap();
    assert_eq!(Plan::decode_terminal(&bytes).unwrap().surface, "terminal");

    let built = app.run(&["app.contract", "-o", "g.plan", "--errors-only"]);
    assert!(built.status.success(), "{built:?}");
    let bytes = std::fs::read(app.0.join("g.plan")).unwrap();
    assert_eq!(Plan::decode(&bytes).unwrap().surface, "gui");

    for refused in [
        &["app.contract", "-o", "x.plan", "--surface", "print"][..],
        &[
            "app.contract",
            "-o",
            "x.plan",
            "--surface",
            "terminal",
            "--entry",
            "w",
        ],
    ] {
        assert_eq!(app.run(refused).status.code(), Some(2), "{refused:?}");
    }
}
