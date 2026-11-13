//! Beacons' stored binding preference and tilt readout (LLP 1133.013 §10).

use exact_data::storage;
use exact_runner::{Answer, DataError, DataSource, Outcome, Store, Value};
use serde_json::json;

const PREFERENCE_FILE: &str = "bindings.txt";
const PREFERENCE_PATH: &str = "app:/data/bindings.txt";
const JUMP_J: &str = "jump KeyJ GamepadSouth";

fn jump_binding(args: &[Value]) -> Result<&'static str, DataError> {
    match args {
        [Value::Bool(on)] => Ok(if *on { JUMP_J } else { "" }),
        _ => Err(DataError::BadArguments(
            "saveBindingPreference(useJ: bool)".into(),
        )),
    }
}

#[derive(Default)]
pub struct BeaconsData;

impl DataSource for BeaconsData {
    fn app_id(&self) -> &str {
        "com.exact.beacons"
    }

    fn grants(&self) -> &str {
        "device.motion purpose.motion\nfs.read app:/data\nfs.write app:/data/bindings.txt"
    }

    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match source {
            "bindingPreference" | "saveBindingPreference" => Err(DataError::Unavailable(
                "binding preferences need app storage".into(),
            )),
            "tiltAngles" => {
                let n: Vec<f64> = args.iter().map(|a| a.as_number().unwrap_or(0.0)).collect();
                let [x, y, z, lx, ly, lz] = n[..] else {
                    return Err(DataError::BadArguments(
                        "tiltAngles(x, y, z, levelX, levelY, levelZ)".into(),
                    ));
                };
                Ok(Value::str(&label([x, y, z], [lx, ly, lz])))
            }
            other => Err(DataError::UnknownSource(other.into())),
        }
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        match source {
            "bindingPreference" => {
                store.observe_external_read();
                Ok(Answer::Later(storage::request(
                    "fs.readdir",
                    json!({"path":"app:/data"}),
                )))
            }
            "saveBindingPreference" => Ok(Answer::Later(storage::request(
                "fs.atomicWriteFile",
                json!({"path":PREFERENCE_PATH, "text":jump_binding(args)?}),
            ))),
            _ => self.query(source, args).map(Answer::Now),
        }
    }

    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let value = storage::response(outcome).map_err(DataError::Unavailable)?;
        let binding = match source {
            "bindingPreference" => {
                if let Some(files) = value.as_array() {
                    if files.iter().any(|name| name == PREFERENCE_FILE) {
                        return Ok(Answer::Later(storage::request(
                            "fs.readFile",
                            json!({"path":PREFERENCE_PATH}),
                        )));
                    }
                    String::new()
                } else {
                    let text = value["base64"]
                        .as_str()
                        .and_then(exact_runner::agent::unbase64)
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .ok_or_else(|| {
                            DataError::Unavailable("invalid binding preference file".into())
                        })?;
                    if !text.is_empty() && text != JUMP_J {
                        return Err(DataError::Unavailable(
                            "invalid binding preference file".into(),
                        ));
                    }
                    text
                }
            }
            "saveBindingPreference" => jump_binding(args)?.into(),
            other => return Err(DataError::UnknownSource(other.into())),
        };
        Ok(Answer::Now(Value::str(&binding)))
    }
}

/// Degrees of roll and pitch of gravity `g` from the level `n`, as the
/// engine's tilt stick reads them (LLP 1133.013 D6): roll positive to the
/// right, pitch positive forward, each rounded to a whole degree.
pub fn angles(g: [f64; 3], n: [f64; 3]) -> (f64, f64) {
    let along = |v: [f64; 3], i: usize| {
        let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if length > 0.0 {
            (v[i] / length).clamp(-1.0, 1.0).asin().to_degrees()
        } else {
            0.0
        }
    };
    let roll = -(along(g, 0) - along(n, 0));
    let pitch = along(g, 1) - along(n, 1);
    (roll.round() + 0.0, pitch.round() + 0.0)
}

/// "12° right · 30° forward"; "0° · 0°" level.
pub fn label(g: [f64; 3], n: [f64; 3]) -> String {
    let (roll, pitch) = angles(g, n);
    let side = |d: f64, plus: &str, minus: &str| match d {
        d if d > 0.0 => format!("{d}° {plus}"),
        d if d < 0.0 => format!("{}° {minus}", -d),
        _ => "0°".to_owned(),
    };
    format!(
        "{} · {}",
        side(roll, "right", "left"),
        side(pitch, "forward", "back")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: [f64; 3] = [0.0, 0.0, 9.80665];

    #[test]
    fn the_readout_is_the_tilt_from_the_level() {
        assert_eq!(label(LEVEL, LEVEL), "0° · 0°");
        assert_eq!(label([0.0, 4.9, 8.49], LEVEL), "0° · 30° forward");
        assert_eq!(label([3.0, 4.9, 7.9], [0.0, 4.9, 8.49]), "18° left · 0°");
        assert_eq!(label([-3.0, -4.9, 7.9], LEVEL), "18° right · 30° back");
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use exact_data_host::Storage;
    use exact_runner::{Dispatch, Work};
    use std::path::PathBuf;

    struct Files(PathBuf);
    impl Files {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../target/binding-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }

        fn call(&self, source: &str, args: &[Value]) -> Result<Value, DataError> {
            let mut host = Storage::new(BeaconsData);
            host.configure_storage(
                self.0.join("data"),
                self.0.join("cache"),
                self.0.join("tmp"),
            )?;
            host.activate()?;
            let mut store = Store::default();
            let mut answer = host.answer(&mut store, source, args)?;
            for _ in 0..3 {
                match answer {
                    Answer::Now(value) => return Ok(value),
                    Answer::Later(request) => {
                        let Dispatch::Run(Work::Now(job)) =
                            host.dispatch(request.continuation.unwrap(), &store)
                        else {
                            panic!("expected a storage worker");
                        };
                        let outcome = std::thread::spawn(job).join().unwrap();
                        answer = host.parse(&mut store, source, args, outcome)?;
                    }
                }
            }
            panic!("binding preference did not finish");
        }
    }
    impl Drop for Files {
        fn drop(&mut self) {
            if self.0.exists() {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }

    #[test]
    fn binding_preference_is_read_from_app_storage() {
        let answer = BeaconsData
            .answer(&mut Store::default(), "bindingPreference", &[])
            .unwrap();
        assert!(matches!(answer, Answer::Later(_)));
        let files = Files::new();
        assert_eq!(
            files.call("bindingPreference", &[]).unwrap(),
            Value::str("")
        );
        assert!(
            !files.0.join("data/bindings.txt").exists(),
            "reading defaults writes nothing"
        );
        // Each call creates a new source and executor over the same app storage.
        for (on, binding) in [(true, JUMP_J), (false, ""), (true, JUMP_J)] {
            assert_eq!(
                files
                    .call("saveBindingPreference", &[Value::Bool(on)])
                    .unwrap(),
                Value::str(binding)
            );
            assert_eq!(
                std::fs::read_to_string(files.0.join("data/bindings.txt")).unwrap(),
                binding
            );
            assert_eq!(
                files.call("bindingPreference", &[]).unwrap(),
                Value::str(binding)
            );
        }
        assert!(files
            .call("saveBindingPreference", &[Value::str("KeyX")])
            .is_err());
        assert_eq!(
            files.call("bindingPreference", &[]).unwrap(),
            Value::str(JUMP_J)
        );
        std::fs::write(files.0.join("data/bindings.txt"), "jump KeyX").unwrap();
        assert!(files.call("bindingPreference", &[]).is_err());
    }

    #[test]
    fn a_failed_write_does_not_report_a_saved_preference() {
        let result = BeaconsData.parse(
            &mut Store::default(),
            "saveBindingPreference",
            &[Value::Bool(true)],
            Outcome::Storage(br#"{"error":"disk full"}"#.to_vec()),
        );
        assert!(matches!(result, Err(DataError::Unavailable(message)) if message == "disk full"));
    }
}
