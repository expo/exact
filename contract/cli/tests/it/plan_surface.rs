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
