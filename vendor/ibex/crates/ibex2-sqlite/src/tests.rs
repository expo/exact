use super::*;
use ibex2::stdlib::sqlite::{Database, ForeignKeys, Provider};
fn open(dir: &tempfile::TempDir) -> Database {
    let path = dir.path().canonicalize().unwrap().join("app.db");
    Database::new(SqliteProvider.open(Location { path }).unwrap())
}
#[test]
fn persists_typed_values_and_prepared_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.execute(
        "CREATE TABLE items (id INTEGER, name TEXT, data BLOB, nothing_value, real_value REAL)",
        &[],
    )
    .unwrap();
    let statement = db
        .prepare("INSERT INTO items VALUES (?1,?2,?3,?4,?5)")
        .unwrap();
    assert_eq!(statement.info.parameter_count, 5);
    let params = [
        Value::Integer(i64::MAX),
        Value::Text("'); DROP TABLE items; --".into()),
        Value::Blob(vec![0, 255]),
        Value::Null,
        Value::Real(1.5),
    ];
    assert_eq!(statement.execute(&params).unwrap().changes, 1);
    statement.close();
    assert!(statement.execute(&params).is_err());
    db.close().unwrap();
    let db = open(&dir);
    let result = db.query("SELECT * FROM items", &[]).unwrap();
    assert_eq!(result.rows, vec![params.to_vec()]);
    assert_eq!(
        db.query("SELECT id AS x,id AS x FROM items", &[])
            .unwrap()
            .columns,
        ["x", "x"]
    );
    let statement = db.prepare("SELECT * FROM items").unwrap();
    db.close().unwrap();
    db.close().unwrap();
    assert!(statement.query(&[]).is_err());
}
#[test]
fn transactions_roll_back_and_deny_manual_control() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.execute("CREATE TABLE items(id INTEGER UNIQUE)", &[])
        .unwrap();
    let batch = vec![
        Command {
            sql: "INSERT INTO items VALUES (?)".into(),
            params: vec![Value::Integer(1)]
        };
        2
    ];
    assert!(db.transaction(&batch).is_err());
    assert_eq!(
        db.query("SELECT COUNT(*) FROM items", &[]).unwrap().rows[0][0],
        Value::Integer(0)
    );
    assert_eq!(db.transaction(&batch[..1]).unwrap()[0].changes, 1);
    for sql in ["BEGIN", "COMMIT", "SAVEPOINT x", "ROLLBACK"] {
        assert!(db.execute(sql, &[]).is_err(), "{sql}");
    }
}
#[test]
fn native_authorizer_prevents_sql_filesystem_escapes() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    for sql in [
        "ATTACH DATABASE '/tmp/ibex-escape.db' AS escaped",
        "DETACH main",
        "VACUUM INTO '/tmp/ibex-escape.db'",
        "PRAGMA temp_store_directory='/tmp'",
        "PRAGMA writable_schema=ON",
        "PRAGMA journal_mode=OFF",
        "SELECT load_extension('/tmp/evil.so')",
        "CREATE VIRTUAL TABLE x USING fts5(body)",
    ] {
        assert!(
            db.prepare(sql).is_err() || db.execute(sql, &[]).is_err(),
            "{sql}"
        );
    }
    assert!(db.prepare("SELECT 1; SELECT 2").is_err());
    assert!(db.prepare("").is_err());
    assert!(db.execute("PRAGMA user_version=3", &[]).is_ok());
    assert!(db.execute("SELECT 1", &[]).is_err());
    assert!(db.query("CREATE TABLE bad(id)", &[]).is_err());
}
#[test]
fn concurrent_transactions_never_interleave() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.execute("CREATE TABLE counter(value INTEGER)", &[])
        .unwrap();
    db.execute("INSERT INTO counter VALUES (0)", &[]).unwrap();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    db.transaction(&[
                        Command {
                            sql: "UPDATE counter SET value=value+1".into(),
                            params: vec![],
                        },
                        Command {
                            sql: "UPDATE counter SET value=value+1".into(),
                            params: vec![],
                        },
                    ])
                    .unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        db.query("SELECT value FROM counter", &[]).unwrap().rows[0][0],
        Value::Integer(160)
    );
}
#[test]
fn separate_connections_use_sqlite_locking() {
    let dir = tempfile::tempdir().unwrap();
    let first = open(&dir);
    first
        .execute("CREATE TABLE items(value INTEGER)", &[])
        .unwrap();
    let second = open(&dir);
    let worker = std::thread::spawn(move || {
        for _ in 0..20 {
            second.execute("INSERT INTO items VALUES (2)", &[]).unwrap();
        }
    });
    for _ in 0..20 {
        first.execute("INSERT INTO items VALUES (1)", &[]).unwrap();
    }
    worker.join().unwrap();
    assert_eq!(
        first.query("SELECT COUNT(*) FROM items", &[]).unwrap().rows[0][0],
        Value::Integer(40)
    );
}
#[test]
fn bounds_results_and_validates_parameters() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    assert!(db.query("SELECT ?", &[]).is_err());
    assert!(db.query("SELECT ?", &[Value::Real(f64::NAN)]).is_err());
    assert!(db.query("SELECT zeroblob(16777216)", &[]).is_err());
    assert!(db.query("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100001) SELECT x FROM n", &[]).is_err());
}
#[cfg(unix)]
#[test]
fn rejects_database_and_parent_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();
    assert!(SqliteProvider
        .open(Location {
            path: root.join("linked/app.db")
        })
        .is_err());
    std::os::unix::fs::symlink(outside.path().join("outside.db"), root.join("app.db")).unwrap();
    assert!(SqliteProvider
        .open(Location {
            path: root.join("app.db")
        })
        .is_err());
    assert!(!outside.path().join("outside.db").exists());
}

#[cfg(unix)]
#[test]
fn journal_symlinks_cannot_overwrite_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.execute("CREATE TABLE items(value INTEGER)", &[])
        .unwrap();
    let outside = dir.path().join("outside");
    std::fs::write(&outside, b"untouched").unwrap();
    std::os::unix::fs::symlink(&outside, dir.path().join("app.db-journal")).unwrap();
    assert!(db.execute("INSERT INTO items VALUES (1)", &[]).is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
}

#[cfg(windows)]
#[test]
fn reparse_database_parents_and_sidecars_are_refused_before_open() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"private").unwrap();
    for (index, suffix) in ["", "-journal", "-wal", "-shm"].iter().enumerate() {
        let link = root.path().join(format!("db{index}{suffix}"));
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path $env:IBEX_SQLITE_TEST_LINK -Value $env:IBEX_SQLITE_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("IBEX_SQLITE_TEST_LINK", &link)
            .env("IBEX_SQLITE_TEST_TARGET", outside.path())
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(SqliteProvider
            .open(Location {
                path: root.path().join(format!("db{index}"))
            })
            .is_err());
    }
    assert!(SqliteProvider
        .open(Location {
            path: root.path().join("db0/nested.db")
        })
        .is_err());
    assert_eq!(
        std::fs::read(outside.path().join("secret")).unwrap(),
        b"private"
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
}
#[test]
fn a_refused_statement_names_what_was_refused_and_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    for (sql, said) in [
        (
            "ATTACH DATABASE '/tmp/ibex-escape.db' AS escaped",
            "ATTACH and DETACH",
        ),
        ("PRAGMA journal_mode=OFF", "PRAGMA journal_mode: "),
        (
            "SELECT load_extension('/tmp/evil.so')",
            "load_extension: extensions are not loaded",
        ),
        (
            "SELECT LOAD_EXTENSION('/tmp/evil.so')",
            "load_extension: extensions are not loaded",
        ),
        (
            "CREATE VIRTUAL TABLE x USING fts5(body)",
            "a virtual table: ",
        ),
        ("SAVEPOINT x", "SAVEPOINT: use transaction([...])"),
        (
            "BEGIN",
            "BEGIN, COMMIT and ROLLBACK: use transaction([...])",
        ),
    ] {
        let got = format!("{:?}", db.execute(sql, &[]).unwrap_err());
        assert!(
            got.contains(&format!("not authorized: {said}")),
            "{sql}: {got}"
        );
    }
    let got = format!(
        "{:?}",
        db.execute("SELECT missing_function()", &[]).unwrap_err()
    );
    assert!(got.contains("no such function: missing_function"), "{got}");
    // A table-valued PRAGMA prepares its PRAGMA while the query steps.
    let got = format!(
        "{:?}",
        db.query("SELECT * FROM pragma_foreign_keys", &[])
            .unwrap_err()
    );
    assert!(
        got.contains("not authorized: PRAGMA foreign_keys: "),
        "{got}"
    );
    // Refused while it steps: `VACUUM INTO` attaches its target then.
    let got = format!(
        "{:?}",
        db.execute("VACUUM INTO 'copy.db'", &[]).unwrap_err()
    );
    assert!(got.contains("not authorized: ATTACH and DETACH"), "{got}");
    // A batch names the statement by its place and its SQL, and runs none.
    let batch = [
        Command {
            sql: "CREATE TABLE IF NOT EXISTS a (x)".into(),
            params: vec![],
        },
        Command {
            sql: "PRAGMA foreign_keys=OFF".into(),
            params: vec![],
        },
    ];
    let got = format!("{:?}", db.transaction(&batch).unwrap_err());
    assert!(
        got.contains("SQLite: statement 2 of 2 (PRAGMA foreign_keys=OFF): not authorized: PRAGMA foreign_keys: ")
            && got.contains("PRAGMA user_version, application_id"),
        "{got}"
    );
    let tables = db
        .query("SELECT COUNT(*) FROM sqlite_schema WHERE name = 'a'", &[])
        .unwrap();
    assert_eq!(tables.rows[0][0], Value::Integer(0));
    // Any failure in a batch names its statement.
    let missing = Command {
        sql: "INSERT INTO missing VALUES (1)".into(),
        params: vec![],
    };
    let got = format!(
        "{:?}",
        db.transaction(&[batch[0].clone(), missing]).unwrap_err()
    );
    assert!(
        got.contains("statement 2 of 2 (INSERT INTO missing VALUES (1)): no such table"),
        "{got}"
    );
}
#[test]
fn only_sqlite_s_own_result_marks_a_failure_busy() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.execute("CREATE TABLE words(w TEXT NOT NULL UNIQUE)", &[])
        .unwrap();
    // The app's words come after the provider's prefix, never in it.
    let words = "busy, database is locked, denied: ";
    let insert = Command {
        sql: format!("INSERT INTO words VALUES ('{words}') /* {words} */"),
        params: vec![],
    };
    let batch = vec![insert; 2];
    let got = format!("{:?}", db.transaction(&batch).unwrap_err());
    assert!(
        got.starts_with("Failed(\"SQLite: statement 2 of 2 (INSERT INTO words"),
        "{got}"
    );
    // Another connection writing: SQLite's own busy, after its 5 s wait.
    let path = dir.path().canonicalize().unwrap().join("app.db");
    let held = rusqlite::Connection::open(path).unwrap();
    held.execute_batch("BEGIN IMMEDIATE").unwrap();
    let got = format!("{:?}", db.execute("INSERT INTO words VALUES ('x')", &[]));
    held.execute_batch("ROLLBACK").unwrap();
    assert!(got.starts_with("Err(Failed(\"SQLite EBUSY: "), "{got}");
}
fn run(db: &Database, statements: &[(&str, &[Value])], mode: ForeignKeys) -> Result<(), String> {
    let commands: Vec<Command> = statements
        .iter()
        .map(|(sql, params)| Command {
            sql: sql.to_string(),
            params: params.to_vec(),
        })
        .collect();
    db.transaction_with(&commands, mode)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}
fn sql(statements: &[&'static str]) -> Vec<(&'static str, &'static [Value])> {
    statements.iter().map(|s| (*s, &[][..])).collect()
}
fn rows(db: &Database, sql: &str) -> Vec<Vec<Value>> {
    db.query(sql, &[]).unwrap().rows
}
/// `t` with a child of each delete action, each with a row that names it.
fn family(db: &Database) {
    run(
        db,
        &sql(&[
            "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE plain(id INTEGER PRIMARY KEY, p INTEGER REFERENCES t(id))",
            "CREATE TABLE gone(id INTEGER PRIMARY KEY, p INTEGER REFERENCES t(id) ON DELETE CASCADE)",
            "CREATE TABLE nulled(id INTEGER PRIMARY KEY, p INTEGER REFERENCES t(id) ON DELETE SET NULL)",
            "CREATE TABLE strict(id INTEGER PRIMARY KEY, p INTEGER NOT NULL REFERENCES t(id) ON DELETE SET NULL)",
            "INSERT INTO t VALUES (1, 'a'), (2, 'b')",
            "INSERT INTO plain VALUES (10, 1)",
            "INSERT INTO gone VALUES (20, 1)",
            "INSERT INTO nulled VALUES (30, 1)",
            "INSERT INTO strict VALUES (40, 2)",
        ]),
        ForeignKeys::Enforce,
    )
    .unwrap();
}
fn children(db: &Database) -> Vec<Vec<Vec<Value>>> {
    ["plain", "gone", "nulled", "strict"]
        .map(|table| rows(db, &format!("SELECT * FROM {table} ORDER BY id")))
        .to_vec()
}
/// Drizzle's rebuild of `t`, statement for statement; `copied` picks the rows.
fn drizzle(copied: &'static str) -> Vec<&'static str> {
    vec![
        "PRAGMA foreign_keys=OFF",
        "CREATE TABLE `__new_t` (\n\t`id` integer PRIMARY KEY,\n\t`name` text,\n\t`extra` text\n)",
        copied,
        "DROP TABLE `t`",
        "ALTER TABLE `__new_t` RENAME TO `t`",
        "PRAGMA foreign_keys=ON",
    ]
}
const ALL: &str = "INSERT INTO `__new_t`(\"id\", \"name\") SELECT \"id\", \"name\" FROM `t`";
const FAILED: &str = "Failed(\"SQLite: foreign key check failed, the batch is rolled back: ";
/// An ordinary batch still refuses an orphan, as before any migration.
fn enforced(db: &Database) {
    let orphan = [(
        "INSERT INTO plain VALUES (?, ?)",
        &[Value::Integer(99), Value::Integer(99)][..],
    )];
    let got = run(db, &orphan, ForeignKeys::Enforce).unwrap_err();
    assert!(
        got.contains(
            "statement 1 of 1 (INSERT INTO plain VALUES (?, ?)): FOREIGN KEY constraint failed"
        ),
        "{got}"
    );
}
#[test]
fn a_foreign_key_check_batch_runs_drizzle_s_rebuild_and_keeps_every_child_row() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    family(&db);
    let before = children(&db);
    // Enforced, the same batch is refused at its first statement.
    let got = run(&db, &sql(&drizzle(ALL)), ForeignKeys::Enforce).unwrap_err();
    assert!(
        got.contains(
            "statement 1 of 6 (PRAGMA foreign_keys=OFF): not authorized: PRAGMA foreign_keys: "
        ),
        "{got}"
    );
    assert!(
        got.contains("transaction(statements, { foreignKeys: \\\"check\\\" })"),
        "{got}"
    );
    // An insert compiled while enforcement is off, reused after it.
    let mut batch = sql(&drizzle(ALL));
    batch.push((
        "INSERT INTO plain VALUES (?, ?)",
        &[Value::Integer(11), Value::Integer(2)],
    ));
    run(&db, &batch, ForeignKeys::Check).unwrap();
    assert_eq!(
        rows(&db, "SELECT name FROM pragma_table_info('t') ORDER BY cid"),
        [
            [Value::Text("id".into())],
            [Value::Text("name".into())],
            [Value::Text("extra".into())]
        ]
    );
    // With enforcement off, DROP TABLE ran no ON DELETE action: the
    // CASCADE child keeps its row, the SET NULL children their references.
    let mut after = before.clone();
    after[0].push(vec![Value::Integer(11), Value::Integer(2)]);
    assert_eq!(children(&db), after);
    assert!(rows(&db, "PRAGMA foreign_key_check").is_empty());
    enforced(&db);
    // Enforced, the dropped table's delete would run those actions.
    let got = run(
        &db,
        &sql(&["DELETE FROM t WHERE id = 1"]),
        ForeignKeys::Enforce,
    )
    .unwrap_err();
    assert!(got.contains("FOREIGN KEY constraint failed"), "{got}");
}
#[test]
fn a_foreign_key_check_batch_that_leaves_an_orphan_rolls_back_and_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    family(&db);
    let before = children(&db);
    let only_b =
        "INSERT INTO `__new_t`(\"id\", \"name\") SELECT \"id\", \"name\" FROM `t` WHERE id = 2";
    let got = run(&db, &sql(&drizzle(only_b)), ForeignKeys::Check).unwrap_err();
    assert_eq!(
        got,
        format!("{FAILED}3 foreign-key violations: gone row 20 (references t), nulled row 30 (references t), plain row 10 (references t)\")")
    );
    assert_eq!(
        rows(&db, "SELECT COUNT(*) FROM pragma_table_info('t')"),
        [[Value::Integer(2)]]
    );
    assert_eq!(rows(&db, "SELECT COUNT(*) FROM t"), [[Value::Integer(2)]]);
    assert_eq!(children(&db), before);
    enforced(&db);
    let got = run(
        &db,
        &sql(&["INSERT INTO plain VALUES (11, 7)"]),
        ForeignKeys::Check,
    )
    .unwrap_err();
    assert_eq!(
        got,
        format!("{FAILED}1 foreign-key violation: plain row 11 (references t)\")")
    );
    enforced(&db);
    // A parent dropped and never made again: every row naming it.
    let got = run(&db, &sql(&["DROP TABLE t"]), ForeignKeys::Check).unwrap_err();
    assert_eq!(
        got,
        format!("{FAILED}4 foreign-key violations: gone row 20 (references t), nulled row 30 (references t), plain row 10 (references t), and 1 more\")")
    );
    assert_eq!(children(&db), before);
    enforced(&db);
}
#[test]
fn a_foreign_key_check_batch_leaves_enforcement_on_and_its_pragmas_stay_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    family(&db);
    // The migrators' lines are no-ops in the batch, whatever their value;
    // defer_foreign_keys and the rest stay refused.
    run(
        &db,
        &sql(&[
            "PRAGMA foreign_keys=ON",
            "PRAGMA foreign_keys = 1",
            "PRAGMA main.foreign_keys=off",
            "PRAGMA foreign_keys",
        ]),
        ForeignKeys::Check,
    )
    .unwrap();
    enforced(&db);
    for pragma in ["PRAGMA defer_foreign_keys=ON", "PRAGMA journal_mode=OFF"] {
        let got = run(&db, &sql(&[pragma]), ForeignKeys::Check).unwrap_err();
        assert!(got.contains("not authorized: PRAGMA "), "{got}");
        enforced(&db);
    }
    // A failing statement.
    let got = run(
        &db,
        &sql(&[
            "INSERT INTO plain VALUES (12, 77)",
            "INSERT INTO missing VALUES (1)",
        ]),
        ForeignKeys::Check,
    )
    .unwrap_err();
    assert!(
        got.contains("statement 2 of 2 (INSERT INTO missing VALUES (1)): no such table"),
        "{got}"
    );
    assert_eq!(
        rows(&db, "SELECT COUNT(*) FROM plain"),
        [[Value::Integer(1)]]
    );
    enforced(&db);
    // Outside a batch the line is refused, as before.
    let got = format!(
        "{:?}",
        db.execute("PRAGMA foreign_keys=OFF", &[]).unwrap_err()
    );
    assert!(
        got.contains("not authorized: PRAGMA foreign_keys: "),
        "{got}"
    );
    // A batch on another thread waits for the connection: it never runs
    // while a migration has enforcement off.
    let migrations = {
        let db = db.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                run(&db, &sql(&drizzle(ALL)), ForeignKeys::Check).unwrap();
            }
        })
    };
    for _ in 0..50 {
        enforced(&db);
    }
    migrations.join().unwrap();
    enforced(&db);
}
#[test]
fn a_foreign_key_check_batch_checks_rows_not_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    family(&db);
    // A parent made again without a key its children can name: SQLite's
    // check itself fails, so the batch does.
    let got = run(
        &db,
        &sql(&[
            "CREATE TABLE __new_t(id INTEGER, name TEXT)",
            "INSERT INTO __new_t SELECT * FROM t",
            "DROP TABLE t",
            "ALTER TABLE __new_t RENAME TO t",
        ]),
        ForeignKeys::Check,
    )
    .unwrap_err();
    assert!(
        got.starts_with(&format!("{FAILED}foreign key mismatch - \\\""))
            && got.contains("referencing \\\"t\\\""),
        "{got}"
    );
    enforced(&db);
    // A parent dropped while no row names it passes the check, and leaves
    // its child table refusing every write.
    run(
        &db,
        &sql(&[
            "CREATE TABLE lone(id INTEGER PRIMARY KEY)",
            "CREATE TABLE orphaned(p INTEGER REFERENCES lone(id))",
        ]),
        ForeignKeys::Enforce,
    )
    .unwrap();
    run(&db, &sql(&["DROP TABLE lone"]), ForeignKeys::Check).unwrap();
    let got = format!(
        "{:?}",
        db.execute("INSERT INTO orphaned VALUES (NULL)", &[])
            .unwrap_err()
    );
    assert!(got.contains("no such table: main.lone"), "{got}");
}
#[test]
fn a_foreign_key_check_batch_checks_temporary_tables_too() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    family(&db);
    let got = run(
        &db,
        &sql(&[
            "CREATE TEMP TABLE p(id INTEGER PRIMARY KEY)",
            "CREATE TEMP TABLE c(pid INTEGER REFERENCES p(id))",
            "INSERT INTO c VALUES (1)",
        ]),
        ForeignKeys::Check,
    )
    .unwrap_err();
    assert_eq!(
        got,
        format!("{FAILED}1 foreign-key violation: c row 1 (references p)\")")
    );
    assert_eq!(
        rows(&db, "SELECT COUNT(*) FROM temp.sqlite_schema"),
        [[Value::Integer(0)]]
    );
    enforced(&db);
}
/// Bytes allocated on each thread, so a test can bound what one call costs.
struct Counting;
thread_local! {
    static ALLOCATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn count(bytes: usize) {
    let _ = ALLOCATED.try_with(|a| a.set(a.get().saturating_add(bytes)));
}
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        count(layout.size());
        std::alloc::System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        std::alloc::System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        count(size);
        std::alloc::System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static COUNTING: Counting = Counting;
#[test]
fn a_large_foreign_key_check_failure_names_three_rows_and_counts_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    // More orphans than a query may return (MAX_ROWS), each in a table with
    // a long name. The rows are made whole in one batch; the second deletes
    // their parents, so the check fails on all of them.
    let name = format!("c{}", "x".repeat(1_000));
    let fill = |table: &str| {
        format!(
            "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < {}) INSERT INTO {table} SELECT x FROM n",
            MAX_ROWS + 1
        )
    };
    let create = format!("CREATE TABLE {name}(pid INTEGER REFERENCES p(id))");
    let (parents, children) = (fill("p"), fill(&name));
    run(
        &db,
        &[
            ("CREATE TABLE p(id INTEGER PRIMARY KEY)", &[][..]),
            (&create, &[][..]),
            (&parents, &[][..]),
            (&children, &[][..]),
        ],
        ForeignKeys::Enforce,
    )
    .unwrap();
    // Every byte Rust allocates on this thread while the failing batch runs,
    // frees not subtracted: an upper bound on what the check and its message
    // hold at once (SQLite's own memory is not Rust's). Kept to the three
    // rows named, it stays under what a query result may hold
    // (MAX_RESULT_BYTES); collecting every row took about 120 MB.
    let before = ALLOCATED.with(|a| a.get());
    let got = run(&db, &sql(&["DELETE FROM p"]), ForeignKeys::Check).unwrap_err();
    let allocated = ALLOCATED.with(|a| a.get()) - before;
    assert!(allocated < MAX_RESULT_BYTES, "{allocated} bytes");
    let row = |id: usize| format!("{name} row {id} (references p)");
    assert_eq!(
        got,
        format!(
            "{FAILED}{} foreign-key violations: {}, {}, {}, and {} more\")",
            MAX_ROWS + 1,
            row(1),
            row(2),
            row(3),
            MAX_ROWS - 2
        )
    );
    assert_eq!(
        rows(&db, "SELECT COUNT(*) FROM p"),
        [[Value::Integer(MAX_ROWS as i64 + 1)]]
    );
}
