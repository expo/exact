//! Optional native SQLite provider. The embedder owns stable database parents;
//! SQLite's maintained VFS owns journaling, locking and crash recovery.
use ibex2::{
    boundary::HostError,
    stdlib::sqlite::{
        self, Command, ExecuteResult, ForeignKeys, Location, Rows, StatementInfo, Value,
    },
};
use rusqlite::{
    config::DbConfig,
    fallible_iterator::FallibleIterator,
    hooks::{AuthAction, AuthContext, Authorization},
    limits::Limit,
    types::{Value as SqlValue, ValueRef},
    Batch, Connection, OpenFlags, Transaction, TransactionBehavior,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

mod held;
pub use held::{reserve, Refused, Reservation};

const MAX_RESULT_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_COMMANDS: usize = 10_000;
#[derive(Default)]
pub struct SqliteProvider;
/// Register this provider from an app shell that links SQL storage.
pub fn link() {
    sqlite::link(Arc::new(SqliteProvider));
}

struct Session {
    connection: Mutex<Option<Connection>>,
    internal_transaction: Arc<AtomicBool>,
    /// A `ForeignKeys::Check` batch's statements are running: the
    /// migrators' own `PRAGMA foreign_keys` lines are accepted as no-ops.
    checking: Arc<AtomicBool>,
    /// After the connection, so a dropped session closes its file first.
    held: held::Count,
}
fn error(e: impl std::fmt::Display) -> HostError {
    HostError::Failed(format!("SQLite: {e}"))
}
/// The prefix of a failure SQLite reports as busy or locked. A storage
/// error's code is read from its message's prefix (exact2's prelude), which
/// the provider writes before any SQL or name the app chose.
const BUSY: &str = "SQLite EBUSY: ";
/// The PRAGMAs app SQL may run; the rest would change what the host set at
/// open (foreign keys, journaling, temporary files) or reach other files.
const PRAGMAS: [&str; 10] = [
    "user_version",
    "application_id",
    "table_info",
    "table_xinfo",
    "index_list",
    "index_info",
    "foreign_key_list",
    "foreign_key_check",
    "integrity_check",
    "quick_check",
];
const FOREIGN_KEYS_CHECK: &str = "to rebuild a table other rows reference, run the batch as transaction(statements, { foreignKeys: \"check\" })";
thread_local! {
    /// What the authorizer last refused, and why. SQLite calls the
    /// authorizer on the thread that prepares the statement, while it
    /// prepares it.
    static REFUSED: std::cell::Cell<Option<String>> = const { std::cell::Cell::new(None) };
}
/// `e` as a host error; a refusal by the authorizer names what it refused.
/// SQLite reports most refusals as `SQLITE_AUTH` and a function's as
/// `SQLITE_ERROR`, so a refusal recorded while preparing is the cause.
fn sql_error(e: rusqlite::Error) -> HostError {
    use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked};
    match (REFUSED.take(), e.sqlite_error_code()) {
        (Some(why), _) => error(format!("not authorized: {why}")),
        (None, Some(DatabaseBusy | DatabaseLocked)) => HostError::Failed(format!("{BUSY}{e}")),
        // System SQLite may omit the function and fail before calling the authorizer.
        (None, _)
            if e.to_string()
                .eq_ignore_ascii_case("no such function: load_extension") =>
        {
            error("not authorized: load_extension: extensions are not loaded")
        }
        (None, _) => error(e),
    }
}

impl sqlite::Provider for SqliteProvider {
    fn open(&self, location: Location) -> Result<Arc<dyn sqlite::Connection>, HostError> {
        let path = location.path;
        if !path.is_absolute()
            || path.to_str().is_none()
            || path.as_os_str().as_encoded_bytes().contains(&0)
        {
            return Err(HostError::InvalidArgument(
                "SQLite requires an absolute native filename".into(),
            ));
        }
        // Reject existing symlinks at every component, including the final file.
        // The host keeps parents stable after this check; this is not an OS sandbox.
        #[cfg(windows)]
        ibex2::stdlib::app_fs::validate_sqlite_location(&path)?;
        #[cfg(not(windows))]
        {
            let mut ancestor = std::path::PathBuf::new();
            for part in path.components() {
                if matches!(part, std::path::Component::ParentDir) {
                    return Err(error("parent traversal is not allowed"));
                }
                ancestor.push(part);
                match std::fs::symlink_metadata(&ancestor) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        return Err(error("symbolic links are not allowed"))
                    }
                    Ok(meta) if ancestor == path && !meta.is_file() => {
                        return Err(error("database must be a regular file"));
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound && ancestor == path => {}
                    Err(e) => return Err(error(e)),
                }
            }
        }
        // Counted before SQLite opens it: a reserved file is never opened here.
        let held = held::open(&path).map_err(|refused| match refused {
            held::Refused::Create(why) => error(why),
            _ => error(format!(
                "{} is reserved for another SQLite library in this process, such as a native module's, until it lets it go; a file is open in one SQLite at a time",
                path.display()
            )),
        })?;
        REFUSED.take();
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(sql_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql_error)?;
        connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(sql_error)?;
        connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
            .map_err(sql_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY;")
            .map_err(sql_error)?;
        connection
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, MAX_RESULT_BYTES as i32)
            .map_err(sql_error)?;
        connection
            .set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024)
            .map_err(sql_error)?;
        connection
            .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
            .map_err(sql_error)?;
        let internal_transaction = Arc::new(AtomicBool::new(false));
        let internal = internal_transaction.clone();
        let checking = Arc::new(AtomicBool::new(false));
        let checked = checking.clone();
        connection
            .authorizer(Some(move |ctx: AuthContext<'_>| {
                let refused = match ctx.action {
                    AuthAction::Attach { .. } | AuthAction::Detach { .. } => {
                        "ATTACH and DETACH: a connection reaches only its own file".into()
                    }
                    AuthAction::CreateVtable { .. } | AuthAction::DropVtable { .. } => {
                        "a virtual table: no virtual table modules are loaded".into()
                    }
                    AuthAction::Savepoint { .. } => {
                        "SAVEPOINT: use transaction([...]) for statements that commit together"
                            .into()
                    }
                    AuthAction::Transaction { .. } if !internal.load(Ordering::Relaxed) => {
                        "BEGIN, COMMIT and ROLLBACK: use transaction([...]) for statements that commit together".into()
                    }
                    AuthAction::Function { function_name }
                        if function_name.eq_ignore_ascii_case("load_extension") =>
                    {
                        "load_extension: extensions are not loaded".into()
                    }
                    AuthAction::Pragma { pragma_name, .. }
                        if pragma_name.eq_ignore_ascii_case("foreign_keys") =>
                    {
                        if checked.load(Ordering::Relaxed) {
                            return Authorization::Ignore;
                        }
                        format!(
                            "PRAGMA {pragma_name}: the host sets the connection's options; app SQL may run only PRAGMA {}; {FOREIGN_KEYS_CHECK}",
                            PRAGMAS.join(", ")
                        )
                    }
                    AuthAction::Pragma { pragma_name, .. }
                        if !PRAGMAS.contains(&pragma_name.to_ascii_lowercase().as_str()) =>
                    {
                        format!(
                            "PRAGMA {pragma_name}: the host sets the connection's options; app SQL may run only PRAGMA {}",
                            PRAGMAS.join(", ")
                        )
                    }
                    AuthAction::Unknown { code, .. } => {
                        format!("an operation SQLite names {code}")
                    }
                    _ => return Authorization::Allow,
                };
                REFUSED.set(Some(refused));
                Authorization::Deny
            }))
            .map_err(error)?;
        Ok(Arc::new(Session {
            connection: Mutex::new(Some(connection)),
            internal_transaction,
            checking,
            held,
        }))
    }
}

fn validate(conn: &Connection, sql: &str) -> Result<StatementInfo, HostError> {
    REFUSED.take();
    let mut batch = Batch::new(conn, sql);
    let statement = batch
        .next()
        .map_err(sql_error)?
        .ok_or_else(|| error("expected one SQL statement"))?;
    let info = StatementInfo {
        parameter_count: statement.parameter_count(),
        columns: statement
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    if batch.next().map_err(sql_error)?.is_some() {
        return Err(error("expected one SQL statement"));
    }
    Ok(info)
}
fn parameters(params: &[Value]) -> Result<Vec<SqlValue>, HostError> {
    let bytes = params.iter().try_fold(0usize, |size, value| {
        size.checked_add(std::mem::size_of::<Value>())?
            .checked_add(match value {
                Value::Text(v) => v.len(),
                Value::Blob(v) => v.len(),
                _ => 0,
            })
    });
    if bytes.is_none_or(|bytes| bytes > MAX_RESULT_BYTES) {
        return Err(error("parameters exceed 16 MiB limit"));
    }
    params
        .iter()
        .map(|value| {
            Ok(match value {
                Value::Null => SqlValue::Null,
                Value::Integer(v) => SqlValue::Integer(*v),
                Value::Real(v) if v.is_finite() => SqlValue::Real(*v),
                Value::Real(_) => {
                    return Err(HostError::InvalidArgument(
                        "SQLite real must be finite".into(),
                    ))
                }
                Value::Text(v) => SqlValue::Text(v.clone()),
                Value::Blob(v) => SqlValue::Blob(v.clone()),
            })
        })
        .collect()
}
fn execute(conn: &Connection, sql: &str, params: &[Value]) -> Result<ExecuteResult, HostError> {
    validate(conn, sql)?;
    let mut statement = conn.prepare_cached(sql).map_err(sql_error)?;
    if statement.column_count() != 0 {
        return Err(error(
            "execute does not accept a statement returning rows; use query",
        ));
    }
    let changes = statement
        .execute(rusqlite::params_from_iter(parameters(params)?))
        .map_err(sql_error)?;
    Ok(ExecuteResult {
        changes: changes as u64,
        last_insert_rowid: conn.last_insert_rowid(),
    })
}
/// `e`, naming the statement of a batch it came from.
fn in_statement(e: HostError, at: &str, sql: &str) -> HostError {
    const SHOWN: usize = 200;
    let sql = sql.trim();
    let sql = match sql.char_indices().nth(SHOWN) {
        Some((end, _)) => format!("{}…", &sql[..end]),
        None => sql.to_string(),
    };
    match e {
        HostError::Failed(message) => match message.strip_prefix(BUSY) {
            Some(message) => HostError::Failed(format!("{BUSY}{at} ({sql}): {message}")),
            None => {
                let message = message.strip_prefix("SQLite: ").unwrap_or(&message);
                error(format!("{at} ({sql}): {message}"))
            }
        },
        HostError::InvalidArgument(message) => {
            HostError::InvalidArgument(format!("{at} ({sql}): {message}"))
        }
        e => e,
    }
}
/// Turns enforcement back on when a `ForeignKeys::Check` batch ends, on
/// every path, an unwinding one included.
struct Restore<'a> {
    conn: &'a Connection,
    checking: &'a AtomicBool,
}
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.checking.store(false, Ordering::Relaxed);
        let _ = self
            .conn
            .set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true);
    }
}
/// `PRAGMA foreign_key_check` over every schema the batch could write:
/// `main` and `temp` (an unqualified check reads only `main`; nothing else
/// can be attached). A row whose parent row or table is missing fails the
/// batch. The rows are stepped through and counted; only the three to name
/// are kept, so a large failure costs no more than a small one.
fn check_foreign_keys(conn: &Connection) -> Result<(), HostError> {
    const FAILED: &str = "foreign key check failed, the batch is rolled back";
    let fail = |e: rusqlite::Error| match sql_error(e) {
        HostError::Failed(m) => error(format!(
            "{FAILED}: {}",
            m.strip_prefix("SQLite: ").unwrap_or(&m)
        )),
        e => e,
    };
    let mut count = 0usize;
    let mut named: Vec<(String, Option<i64>, String)> = Vec::with_capacity(4);
    for schema in ["main", "temp"] {
        let mut statement = conn
            .prepare(&format!("PRAGMA {schema}.foreign_key_check"))
            .map_err(fail)?;
        let mut rows = statement.query([]).map_err(fail)?;
        while let Some(row) = rows.next().map_err(fail)? {
            count += 1;
            let rowid: Option<i64> = row.get(1).map_err(fail)?;
            let table = row.get_ref(0).map_err(fail)?.as_str().map_err(error)?;
            let parent = row.get_ref(2).map_err(fail)?.as_str().map_err(error)?;
            if named.len() == 3
                && (table, rowid, parent) >= (named[2].0.as_str(), named[2].1, named[2].2.as_str())
            {
                continue;
            }
            named.push((table.to_string(), rowid, parent.to_string()));
            named.sort();
            named.truncate(3);
        }
    }
    if count == 0 {
        return Ok(());
    }
    let named: Vec<String> = named
        .iter()
        .map(|(table, rowid, parent)| match rowid {
            Some(rowid) => format!("{table} row {rowid} (references {parent})"),
            None => format!("a {table} row (references {parent})"),
        })
        .collect();
    let more = match count {
        n if n > 3 => format!(", and {} more", n - 3),
        _ => String::new(),
    };
    Err(error(match count {
        1 => format!("{FAILED}: 1 foreign-key violation: {}", named[0]),
        n => format!(
            "{FAILED}: {n} foreign-key violations: {}{more}",
            named.join(", ")
        ),
    }))
}
impl Session {
    /// One transaction over `commands`; `check` runs `check_foreign_keys`
    /// before the commit.
    fn batch(
        &self,
        conn: &Connection,
        commands: &[Command],
        check: bool,
    ) -> Result<Vec<ExecuteResult>, HostError> {
        self.internal_transaction.store(true, Ordering::Relaxed);
        let transaction = Transaction::new_unchecked(conn, TransactionBehavior::Immediate);
        self.internal_transaction.store(false, Ordering::Relaxed);
        let transaction = transaction.map_err(sql_error)?;
        self.checking.store(check, Ordering::Relaxed);
        let results: Result<Vec<_>, _> = commands
            .iter()
            .enumerate()
            .map(|(i, command)| {
                execute(&transaction, &command.sql, &command.params).map_err(|e| {
                    let at = format!("statement {} of {}", i + 1, commands.len());
                    in_statement(e, &at, &command.sql)
                })
            })
            .collect();
        self.checking.store(false, Ordering::Relaxed);
        let results = results.and_then(|values| match check {
            true => check_foreign_keys(&transaction).map(|()| values),
            false => Ok(values),
        });
        self.internal_transaction.store(true, Ordering::Relaxed);
        let outcome = match results {
            Ok(values) => transaction.commit().map(|()| values).map_err(sql_error),
            Err(e) => {
                drop(transaction);
                Err(e)
            }
        };
        self.internal_transaction.store(false, Ordering::Relaxed);
        outcome
    }
    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T, HostError>) -> Result<T, HostError> {
        let guard = self
            .connection
            .lock()
            .map_err(|_| error("connection lock poisoned"))?;
        f(guard.as_ref().ok_or_else(|| error("database is closed"))?)
    }
}
impl sqlite::Connection for Session {
    fn prepare(&self, sql: &str) -> Result<StatementInfo, HostError> {
        self.with(|conn| validate(conn, sql))
    }
    fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, HostError> {
        self.with(|conn| execute(conn, sql, params))
    }
    fn query(&self, sql: &str, params: &[Value]) -> Result<Rows, HostError> {
        self.with(|conn| {
            validate(conn, sql)?;
            let mut statement = conn.prepare_cached(sql).map_err(sql_error)?;
            // Read-only queries prevent a result-limit failure from committing a
            // partially consumed INSERT/UPDATE ... RETURNING.
            if !statement.readonly() {
                return Err(error("query requires a read-only statement"));
            }
            let columns: Vec<String> = statement
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let mut budget = columns.iter().map(String::len).sum::<usize>();
            let mut cursor = statement
                .query(rusqlite::params_from_iter(parameters(params)?))
                .map_err(sql_error)?;
            let mut rows = Vec::new();
            while let Some(row) = cursor.next().map_err(sql_error)? {
                if rows.len() == MAX_ROWS {
                    return Err(error("query result exceeds row limit"));
                }
                let mut values = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    let value = row.get_ref(i).map_err(error)?;
                    budget = budget.saturating_add(
                        std::mem::size_of::<Value>()
                            + match value {
                                ValueRef::Text(v) | ValueRef::Blob(v) => v.len(),
                                _ => 0,
                            },
                    );
                    if budget > MAX_RESULT_BYTES {
                        return Err(error("query result exceeds 16 MiB limit"));
                    }
                    values.push(match value {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(v) => Value::Integer(v),
                        ValueRef::Real(v) => Value::Real(v),
                        ValueRef::Text(v) => {
                            Value::Text(std::str::from_utf8(v).map_err(error)?.to_string())
                        }
                        ValueRef::Blob(v) => Value::Blob(v.to_vec()),
                    });
                }
                rows.push(values);
            }
            Ok(Rows { columns, rows })
        })
    }
    fn transaction(
        &self,
        commands: &[Command],
        foreign_keys: ForeignKeys,
    ) -> Result<Vec<ExecuteResult>, HostError> {
        if commands.len() > MAX_COMMANDS {
            return Err(error("transaction exceeds 10000 commands"));
        }
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| error("connection lock poisoned"))?;
        let conn = guard.as_ref().ok_or_else(|| error("database is closed"))?;
        REFUSED.take();
        if foreign_keys == ForeignKeys::Enforce {
            return self.batch(conn, commands, false);
        }
        // SQLite ignores a change to enforcement inside a transaction, so it
        // goes off before BEGIN ("Making Other Kinds Of Table Schema
        // Changes", https://sqlite.org/lang_altertable.html#otheralter).
        conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, false)
            .map_err(sql_error)?;
        let restore = Restore {
            conn,
            checking: &self.checking,
        };
        let outcome = self.batch(conn, commands, true);
        drop(restore);
        // The lock is held throughout, so no other batch on this connection
        // ran without enforcement; one that cannot have it back never runs.
        if !matches!(
            conn.db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY),
            Ok(true)
        ) {
            *guard = None;
            return Err(error(
                "foreign keys could not be turned back on, so the database is closed",
            ));
        }
        outcome
    }
    fn close(&self) -> Result<(), HostError> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| error("connection lock poisoned"))?;
        if let Some(conn) = guard.take() {
            if let Err((conn, e)) = conn.close() {
                *guard = Some(conn);
                return Err(error(e));
            }
        }
        self.held.release();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
