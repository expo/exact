use super::{Fixture, Json, SOURCE};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};

#[test]
fn cli_refuses_bad_arguments() {
    let status = Command::new(env!("CARGO_BIN_EXE_exact-js-bake"))
        .output()
        .unwrap();
    assert_eq!(status.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&status.stderr).contains("usage:"));
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        self.0.wait().unwrap();
    }
}

#[test]
fn serve_replies_carry_warnings_and_a_clean_edit_clears_them_without_restarting() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let f = Fixture::new();
    f.write(
        "app.ts",
        &format!("{SOURCE}\nexport const compile = () => new Function('return 1');\n"),
    );
    let expected = f.bake().warnings;
    assert!(expected.iter().any(|w| w.contains("[code-from-string]")));
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_exact-js-bake"))
            .arg(&f.0)
            .arg("--serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut replies = BufReader::new(server.0.stdout.take().unwrap());
    for id in 1..=2 {
        if id == 2 {
            f.write("app.ts", SOURCE);
        }
        let out = f.0.join(format!("generation-{id}"));
        let request = serde_json::json!({"id": id, "out": out});
        writeln!(server.0.stdin.as_mut().unwrap(), "{request}").unwrap();
        let mut line = String::new();
        replies.read_line(&mut line).unwrap();
        let reply: Json = serde_json::from_str(&line).expect("stdout stays JSON lines");
        assert_eq!(reply["id"], id);
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(reply["receipt"].is_object());
        assert!(reply["design"].is_object());
        assert_eq!(
            reply["warnings"],
            if id == 1 {
                serde_json::json!(expected)
            } else {
                serde_json::json!([])
            }
        );
        assert!(server.0.try_wait().unwrap().is_none());
    }
    drop(server.0.stdin.take());
    assert!(server.0.wait().unwrap().success());
    let mut stderr = String::new();
    server
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    for warning in &expected {
        assert!(!stderr.contains(warning), "warnings belong to their reply");
    }
    let mut remaining = String::new();
    replies.read_to_string(&mut remaining).unwrap();
    assert!(remaining.is_empty(), "{remaining}");
}
