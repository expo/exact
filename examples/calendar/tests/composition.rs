use exact_data::storage;
use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Store};
use serde_json::json;

/// Removes its directory when dropped, so a failing test leaves nothing behind.
struct TempDir(std::path::PathBuf);
impl std::ops::Deref for TempDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}
impl AsRef<std::path::Path> for TempDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Fixture;
impl DataSource for Fixture {
    fn app_id(&self) -> &str {
        "test.calendar.composition"
    }
    fn grants(&self) -> &str {
        "sqlite.open app:/data/test.db"
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(storage::request(
            "sqlite",
            json!({"path":"app:/data/test.db", "commands":[
                {"kind":"query", "sql":"SELECT 42", "params":[]}
            ]}),
        )))
    }
}

mod sqlite {
    include!(concat!(env!("OUT_DIR"), "/composition-sqlite.rs"));
    pub(super) fn data() -> impl exact_runner::DataSource {
        AppData::default()
    }
}
mod empty {
    include!(concat!(env!("OUT_DIR"), "/composition-empty.rs"));
    pub(super) fn data() -> impl exact_runner::DataSource {
        AppData::default()
    }
}

fn query(mut data: impl DataSource, root: &std::path::Path) -> Result<serde_json::Value, String> {
    data.configure_storage(
        root.join("data"),
        root.join("cache"),
        root.join("temporary"),
    )
    .unwrap();
    data.activate().unwrap();
    let Answer::Later(request) = data.answer(&mut Store::default(), "sql", &[]).unwrap() else {
        panic!("expected a storage continuation")
    };
    let job = data.continuation(request.continuation.unwrap()).unwrap();
    storage::response(job())
}

// A separate binary keeps other tests from registering this process's provider.
#[test]
fn a_hosts_default_constructor_installs_only_its_selected_sqlite_provider() {
    let root =
        TempDir(std::env::temp_dir().join(format!("calendar-composition-{}", std::process::id())));
    assert_eq!(
        query(empty::data(), &root).unwrap_err(),
        "Unlinked(\"sqlite\")"
    );
    let rows = query(sqlite::data(), &root).unwrap();
    assert_eq!(rows[0]["rows"][0][0], json!({"integer":"42"}));
    std::fs::remove_dir_all(root).unwrap();
}
