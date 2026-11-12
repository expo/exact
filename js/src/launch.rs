//! A setting a launch may give (LLP 1153.002): the value of a name app.json
//! marks `"launch": true`, from the process's environment, where a test, a
//! benchmark or a person sets it (an XCUITest's `launchEnvironment`,
//! `devicectl`'s `--environment-variables`, `SIMCTL_CHILD_<NAME>`, a desktop
//! process's environment). Read once per process, so every instance of the
//! module in one launch (the owner's, a worker's, a reload's, an update's)
//! reads the same value whatever later changes the environment.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Door 27's answer for `name`: the launch's value, or none where the
/// module takes the build's. None for a name the grants the module was
/// loaded with do not carry (`env.launch`, which only the bake writes), so
/// no other variable of the process is read whatever code asks. None at
/// bake, whose evaluation is the build's (D2): what it compiles is the first
/// frame, and a launch asks every compiled answer again once its module
/// loads (LLP 1048.003 D6).
pub(crate) fn value(state: &super::HostState, name: &str) -> Option<String> {
    if state.baking || !state.launch.iter().any(|granted| granted == name) {
        return None;
    }
    let mut read = READ
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read.entry(name.to_owned())
        .or_insert_with(|| given(name, |n| std::env::var(n).ok()))
        .clone()
}

/// What this process's launch gave, by name, once asked.
static READ: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

/// The value `environment` holds for `name` when a launch gave it: a
/// setting's name (`[A-Z][A-Z0-9_]*`) and not the host's (`EXACT_…`), as the
/// bake requires of a marked name; set and not empty; and not the value Bun
/// loaded from an `.env` file into the command that started this process
/// (`EXACT_BUN_LOADED`'s `NAME=<SHA-256 of the value>`,
/// js/bake/src/__exact_env.mjs), which is the build's source, while another
/// value is one a command or a launcher set below it. Android starts an app
/// with no environment a launcher sets (LLP 1153.002 Q1): the build's value
/// there.
fn given(name: &str, environment: impl Fn(&str) -> Option<String>) -> Option<String> {
    let setting = name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !name.starts_with("EXACT_");
    if cfg!(target_os = "android") || !setting {
        return None;
    }
    use sha2::{Digest, Sha256};
    let value = environment(name).filter(|value| !value.is_empty())?;
    let bun = format!("{name}={:x}", Sha256::digest(value.as_bytes()));
    let loaded = environment("EXACT_BUN_LOADED").unwrap_or_default();
    (!loaded.split(',').any(|loaded| loaded == bun)).then_some(value)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn a_launch_gives_a_set_nonempty_setting_that_bun_did_not_load() {
        // `FROM_FILE` still holds the value Bun loaded; `OVERRIDDEN` holds
        // another, which a command or a launcher below set.
        let loaded = format!(
            "OTHER={:x},FROM_FILE={:x},OVERRIDDEN={:x}",
            Sha256::digest(b"o"),
            Sha256::digest(b"A"),
            Sha256::digest(b"A")
        );
        let environment = |name: &str| match name {
            "API_BASE_URL" => Some("http://launch.test".to_owned()),
            "EMPTY" | "EXACT_AGENT" => Some(String::new()),
            "EXACT_HOST" => Some("x".to_owned()),
            "FROM_FILE" => Some("A".to_owned()),
            "OVERRIDDEN" => Some("B".to_owned()),
            "lower" => Some("y".to_owned()),
            "EXACT_BUN_LOADED" => Some(loaded.clone()),
            _ => None,
        };
        let given = |name| super::given(name, environment);
        assert_eq!(given("API_BASE_URL").as_deref(), Some("http://launch.test"));
        assert_eq!(given("OVERRIDDEN").as_deref(), Some("B"));
        for name in [
            "EMPTY",
            "UNSET",
            "EXACT_HOST",
            "FROM_FILE",
            "lower",
            "",
            "1A",
        ] {
            assert_eq!(given(name), None, "{name}");
        }
    }
}
