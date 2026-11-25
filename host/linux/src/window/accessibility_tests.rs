//! Drive the Wayland window through the desktop's accessibility bus.
use serde_json::Value;
use std::{
    process::{Child, Command},
    time::{Duration, Instant},
};

const ROOT: &str = "/org/a11y/atspi/accessible/root";
const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const BUTTON: u32 = 43; // ATSPI_ROLE_PUSH_BUTTON

fn bus(connection: &str, args: &[&str]) -> Result<Value, String> {
    let output = Command::new("busctl")
        .args(["--json=short", "--timeout=2s", connection, "--"])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn until<T>(mut read: impl FnMut() -> Result<T, String>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match read() {
            Ok(answer) => return answer,
            Err(error) => assert!(
                Instant::now() < deadline,
                "AT-SPI publication timed out: {error}"
            ),
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

struct Probe(Child);
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "build Carousel and run with --profile host-dev in a Wayland session with AT-SPI enabled"]
fn wayland_publishes_and_activates_the_named_button() {
    let test = std::env::current_exe().unwrap();
    let app = test
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("carousel-linux");
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("app.plan");
    let bytes = contract::compile(
        r#"component App
  state message = "Ready"
  action save
    message = "Saved"
  view
    column
      button "Save" press=save
      input aria-label="Draft" value="A😀Z"
      text message role="status"
"#,
    )
    .unwrap()
    .encode();
    std::fs::write(&plan, bytes).unwrap();
    let probe = Probe(Command::new(app).env("EXACT_PLAN", plan).spawn().unwrap());
    let address = bus(
        "--user",
        &[
            "call",
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
        ],
    )
    .unwrap();
    let address = format!("--address={}", address["data"][0].as_str().unwrap());
    let children = |owner: &str, path: &str| {
        bus(&address, &["call", owner, path, ACCESSIBLE, "GetChildren"])
            .map(|reply| reply["data"][0].as_array().unwrap().clone())
    };
    let name = until(|| {
        let list = bus(&address, &["list"])?;
        let apps = children("org.a11y.atspi.Registry", ROOT)?;
        apps.iter()
            .filter_map(|app| app[0].as_str())
            .find(|name| {
                list.as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["pid"] == probe.0.id() && row["name"] == *name)
            })
            .map(str::to_owned)
            .ok_or_else(|| format!("registry has no application for PID {}", probe.0.id()))
    });
    let frame = until(|| {
        children(&name, ROOT)?
            .first()
            .and_then(|child| child[1].as_str())
            .map(str::to_owned)
            .ok_or_else(|| "application has no frame".into())
    });
    const FRAME: u32 = 23; // ATSPI_ROLE_FRAME
    assert_eq!(
        bus(&address, &["call", &name, &frame, ACCESSIBLE, "GetRole"]).unwrap()["data"][0],
        FRAME
    );
    let named = |text: &str| {
        for child in children(&name, &frame)? {
            let path = child[1].as_str().unwrap();
            let label = bus(&address, &["get-property", &name, path, ACCESSIBLE, "Name"])?;
            if label["data"] == text {
                return Ok(path.to_owned());
            }
        }
        Err(format!("frame has no child named {text}"))
    };
    let save = until(|| named("Save"));
    assert_eq!(
        bus(&address, &["call", &name, &save, ACCESSIBLE, "GetRole"]).unwrap()["data"][0],
        BUTTON
    );
    until(|| named("Ready"));
    let action = "org.a11y.atspi.Action";
    let click = until(|| {
        bus(
            &address,
            &["call", &name, &save, action, "GetName", "i", "0"],
        )
    });
    assert_eq!(click["data"][0], "click");
    assert_eq!(
        bus(
            &address,
            &["call", &name, &save, action, "DoAction", "i", "0"]
        )
        .unwrap()["data"][0],
        true
    );
    until(|| named("Saved"));
    let field = until(|| named("Draft"));
    let component = "org.a11y.atspi.Component";
    assert_eq!(
        bus(&address, &["call", &name, &field, component, "GrabFocus"]).unwrap()["data"][0],
        true
    );
    until(|| {
        let state = bus(&address, &["call", &name, &field, ACCESSIBLE, "GetState"])?;
        let bits = state["data"][0][0].as_u64().unwrap();
        (bits & (1 << 12) != 0)
            .then_some(())
            .ok_or_else(|| "Draft lacks ATSPI_STATE_FOCUSED".into())
    });
    let text = "org.a11y.atspi.Text";
    assert_eq!(
        bus(
            &address,
            &["call", &name, &field, text, "GetText", "ii", "0", "-1"]
        )
        .unwrap()["data"][0],
        "A😀Z"
    );
    assert_eq!(
        bus(
            &address,
            &["call", &name, &field, text, "SetCaretOffset", "i", "2"]
        )
        .unwrap()["data"][0],
        true
    );
    until(|| {
        let caret = bus(
            &address,
            &["get-property", &name, &field, text, "CaretOffset"],
        )?;
        (caret["data"] == 2)
            .then_some(())
            .ok_or_else(|| format!("expected AT-SPI character offset 2, got {caret}"))
    });
}
