//! A setting a launch may give (LLP 1153.002): the value of a name app.json
//! marks `"launch": true`, from the process's environment, where a test, a
//! benchmark or a person sets it (an XCUITest's `launchEnvironment`,
//! `devicectl`'s `--environment-variables`, `SIMCTL_CHILD_<NAME>`, a desktop
//! process's environment). Read once per process, so every instance of the
//! module in one launch (the owner's, a worker's, a reload's, an update's)
//! reads the same value whatever later changes the environment.
/// Door 28's answer for `name`: the launch's value, or none where the
/// module takes the build's. None for a name the grants the module was
/// loaded with do not carry (`env.launch`, which only the bake writes), so
/// no other variable of the process is read whatever code asks. None at
/// bake, whose evaluation is the build's (D2): what it compiles is the first
/// frame, and a launch asks every compiled answer again once its module
/// loads (LLP 1048.003 D6), but for an `else` placeholder.
pub(crate) fn value(state: &super::HostState, name: &str) -> Option<String> {
    if state.baking || !state.launch.iter().any(|granted| granted == name) {
        return None;
    }
    exact_runner::launch_env::value(name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn preload_and_module_read_the_same_record() {
        const TEST: &str = "launch::tests::preload_and_module_read_the_same_record";
        let Ok(case) = std::env::var("EONE_MODULE_CASE") else {
            for (case, value) in [
                ("unset", None),
                ("empty", Some("")),
                ("launch", Some("https://launch.example")),
            ] {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args([TEST, "--exact", "--nocapture"])
                    .env("EONE_MODULE_CASE", case)
                    .env_remove("EONE_MODULE_URL");
                if let Some(value) = value {
                    command.env("EONE_MODULE_URL", value);
                }
                let output = command.output().unwrap();
                assert!(
                    output.status.success(),
                    "{case}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return;
        };
        use exact_runner::{
            request::{Preload, PreloadPart, PreloadWhen},
            Request,
        };
        let record = serde_json::json!({"EONE_MODULE_URL":{"value":"https://built.example","source":"default","launch":true}});
        let build: &'static str = record["EONE_MODULE_URL"]["value"]
            .as_str()
            .unwrap()
            .to_owned()
            .leak();
        let module = crate::Module::new(
            Vec::<u8>::new(),
            "test.preload-setting",
            "net.fetch https:\nenv.launch EONE_MODULE_URL",
        );
        let value = super::value(&module.host, "EONE_MODULE_URL").unwrap_or_else(|| build.into());
        let expected = if case == "launch" {
            "https://launch.example"
        } else {
            build
        };
        assert_eq!(value, expected);
        let mut request = Request::get(&format!("{value}/feed"));
        request.reach.origin = exact_runner::CallOrigin::App;
        request.method = "POST".into();
        request.headers = vec![("X-Note".into(), value.clone())];
        request.body = value.into_bytes();
        let parts: &'static [PreloadPart] = vec![PreloadPart::Env("EONE_MODULE_URL", build)].leak();
        let preload = Preload {
            name: "feed",
            method: "POST",
            url: vec![parts[0], PreloadPart::Literal("/feed")].leak(),
            headers: vec![("X-Note", parts)].leak(),
            body: Some(parts),
            timeout_ms: None,
            when: PreloadWhen::Start,
            secrets: &[],
        };
        assert_eq!(
            preload
                .fill(
                    |_| panic!("no secrets"),
                    exact_executor::preload::preload_url,
                )
                .unwrap(),
            request
        );
        let mut rendering = crate::Module::new(
            Vec::<u8>::new(),
            "test.preload-setting",
            "env.launch EONE_MODULE_URL",
        );
        rendering.host.launch.clear();
        assert_eq!(super::value(&rendering.host, "EONE_MODULE_URL"), None);
    }
}
