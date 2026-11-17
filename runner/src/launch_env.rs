//! Process-wide launch settings shared by modules and preloads (LLP 1162.000 D2).
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A nonempty launch value, fixed on the first read of this name in the process.
/// Callers admit only names declared by the bake. Android and the web use build values.
pub fn value(name: &str) -> Option<String> {
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
/// bake requires of a marked name, set and not empty. However it came to be
/// there (a test, a launcher, what Bun loaded from `.env` into the command
/// that started this process), it is the launch's (D2, "A drive"). Android
/// starts an app with no environment a launcher sets (LLP 1153.002 Q1): the
/// build's value there.
fn given(name: &str, environment: impl Fn(&str) -> Option<String>) -> Option<String> {
    let setting = name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !name.starts_with("EXACT_");
    if cfg!(any(target_os = "android", target_arch = "wasm32")) || !setting {
        return None;
    }
    environment(name).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_launch_gives_a_set_nonempty_setting() {
        let environment = |name: &str| match name {
            "API_BASE_URL" => Some("http://launch.test".to_owned()),
            "EMPTY" | "EXACT_AGENT" => Some(String::new()),
            "EXACT_HOST" => Some("x".to_owned()),
            // Whatever names Bun noted (LLP 1153 D4.2), the value is read.
            "FROM_FILE" => Some("A".to_owned()),
            "EXACT_BUN_LOADED" => Some("FROM_FILE".to_owned()),
            "lower" => Some("y".to_owned()),
            _ => None,
        };
        let given = |name| super::given(name, environment);
        assert_eq!(given("API_BASE_URL").as_deref(), Some("http://launch.test"));
        assert_eq!(given("FROM_FILE").as_deref(), Some("A"));
        for name in ["EMPTY", "UNSET", "EXACT_HOST", "lower", "", "1A"] {
            assert_eq!(given(name), None, "{name}");
        }
    }
}
