//! The module receipt records the Android permissions the generation's
//! source derives (LLP 1030.006 D4a): an installed APK cannot see the source,
//! so an update carries them, signed, to be compared with what the APK
//! declared.
use exact_js_bake::{bake, Tools};

const READS: &str = r#"use calendarStatus, CalendarStatus, CalendarEvent from "@exact/calendar"

component App
  resource status = calendarStatus() as shape CalendarStatus
  view
    column
      text status.access
"#;

#[test]
fn a_generation_that_starts_calling_save_event_records_write_calendar() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let permissions = |name: &str, contract: &str| {
        let dir = std::env::temp_dir().join(format!("exact-perms-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("node_modules/@exact")).unwrap();
        let package =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/calendar");
        std::os::unix::fs::symlink(
            package.canonicalize().unwrap(),
            dir.join("node_modules/@exact/calendar"),
        )
        .unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
        write(
            "app.json",
            r#"{"app": {"id": "test.exact.perms", "name": "Perms"}}"#,
        );
        write("app.contract", contract);
        write("app.ts", "import type { Sources, Answer } from './app.contract.d.ts';\nexport const grants = 'device.calendar calendarPurpose';\nconst sources: Sources = {};\nexport const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);\n");
        let baked = bake(&dir, &Tools::default()).unwrap_or_else(|e| panic!("{e}"));
        let receipt: serde_json::Value = serde_json::from_str(&baked.receipt).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        receipt["androidPermissions"].clone()
    };
    assert_eq!(permissions("reads", READS), serde_json::json!([]));
    let writes = READS.replace("use calendarStatus,", "use calendarStatus, saveEvent,").replace(
        "  view\n",
        "  mutation saved as shape CalendarEvent\n  action save\n    send saved = saveEvent(event=CalendarEvent(id=none, uid=none, calendarId=none, title=\"Tea\", description=none, start=\"2026-09-16T15:00:00\", timeZone=none, duration=\"PT30M\", showWithoutTime=false, location=none, url=none, status=\"confirmed\", freeBusyStatus=\"busy\", recurrenceId=none, utcStart=none, utcEnd=none, apple=none))\n  view\n",
    );
    assert_eq!(
        permissions("writes", &writes),
        serde_json::json!(["android.permission.WRITE_CALENDAR"])
    );
}
