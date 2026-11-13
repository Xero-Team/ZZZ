use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    marker::PhantomData,
    path::Path,
    ptr,
};

use anyhow::Result;
use libsqlite3_sys::*;

pub struct Connection {
    pub(crate) sqlite3: *mut sqlite3,
    persistent: bool,
    pub(crate) write: RefCell<bool>,
    _sqlite: PhantomData<sqlite3>,
}

// SAFETY: a `Connection` owns its `sqlite3` handle outright, so moving the struct moves the
// only handle to it. The type is deliberately not `Sync`, which is what makes the move sound:
// SQLite opened with `SQLITE_OPEN_NOMUTEX` may only be used by one thread at a time, and
// without `Sync` no second thread can reach the handle through a shared reference.
// `ThreadSafeConnection` is the layer that serializes cross-thread use.
unsafe impl Send for Connection {}

impl Connection {
    fn open_with_flags(uri: &str, persistent: bool, flags: i32) -> Result<Self> {
        let mut connection = Self {
            sqlite3: ptr::null_mut(),
            persistent,
            write: RefCell::new(true),
            _sqlite: PhantomData,
        };
        // Bound to a local so the NUL-terminated bytes are provably alive for the whole call
        // instead of relying on temporary lifetime extension inside the statement.
        let uri = CString::new(uri)?;

        // SAFETY: `uri` is a NUL-terminated string alive for the duration of the call, and
        // `connection.sqlite3` is a local field that SQLite may write to. On failure SQLite
        // still stores a handle in it, which `last_error` below reports and `Drop` closes.
        unsafe {
            sqlite3_open_v2(
                uri.as_ptr(),
                &raw mut connection.sqlite3,
                flags,
                ptr::null(),
            );
        }

        if connection.sqlite3.is_null() {
            anyhow::bail!("sqlite3_open_v2 returned no connection handle for {uri:?}");
        }

        // SAFETY: the handle was just checked non-null and is owned by `connection`, which is
        // not shared yet.
        unsafe {
            sqlite3_extended_result_codes(connection.sqlite3, 1);
        }

        // Wait for the database lock to be released instead of failing
        // immediately with SQLITE_BUSY. Some databases (e.g. the agent
        // threads database) are shared between ZZZ instances, so transient
        // lock contention is expected; failing fast turns it into a
        // storm of dropped saves and retry churn.
        //
        // SAFETY: same live, exclusively owned handle as above; `sqlite3_busy_timeout` only
        // stores a timeout on it.
        unsafe {
            sqlite3_busy_timeout(connection.sqlite3, 5000);
        }

        connection.last_error()?;

        Ok(connection)
    }

    pub(crate) fn open(uri: &str, persistent: bool) -> Result<Self> {
        Self::open_with_flags(
            uri,
            persistent,
            SQLITE_OPEN_CREATE | SQLITE_OPEN_NOMUTEX | SQLITE_OPEN_READWRITE,
        )
    }

    /// Attempts to open the database at uri. If it fails, a shared memory db will be opened
    /// instead.
    pub fn open_file(uri: &str) -> Self {
        Self::open(uri, true).unwrap_or_else(|error| {
            log::error!(
                "failed to open database at {uri}: {error:#}; falling back to an in-memory database"
            );
            Self::open_memory(Some(uri))
        })
    }

    pub fn open_memory(uri: Option<&str>) -> Self {
        if let Some(uri) = uri {
            let in_memory_path = format!("file:{}?mode=memory&cache=shared", uri);
            return Self::open_with_flags(
                &in_memory_path,
                false,
                SQLITE_OPEN_CREATE | SQLITE_OPEN_NOMUTEX | SQLITE_OPEN_READWRITE | SQLITE_OPEN_URI,
            )
            .expect("Could not create fallback in memory db");
        }

        Self::open(":memory:", false).expect("Could not create fallback in memory db")
    }

    pub fn persistent(&self) -> bool {
        self.persistent
    }

    pub fn can_write(&self) -> bool {
        *self.write.borrow()
    }

    pub fn backup_main(&self, destination: &Connection) -> Result<()> {
        // Both database names are bound to locals so the NUL-terminated bytes outlive the
        // `backup_init` call rather than depending on temporary lifetime extension.
        let destination_name = CString::new("main")?;
        let source_name = CString::new("main")?;
        // SAFETY: both handles are live connections owned by the callers, and both name
        // strings are alive for the duration of the call. `backup_init` only reads them.
        let backup = unsafe {
            sqlite3_backup_init(
                destination.sqlite3,
                destination_name.as_ptr(),
                self.sqlite3,
                source_name.as_ptr(),
            )
        };

        // `sqlite3_backup_init` reports failure by returning NULL and records the reason on the
        // destination connection, so the error is read from there instead of stepping a null
        // backup handle.
        if backup.is_null() {
            return destination.last_error();
        }

        // SAFETY: `backup` is a non-null handle created above; `-1` copies every page, and
        // `backup_finish` releases the handle exactly once.
        unsafe {
            sqlite3_backup_step(backup, -1);
        }
        // SAFETY: same handle, still owned here, and this call is what frees it.
        unsafe {
            sqlite3_backup_finish(backup);
        }
        destination.last_error()
    }

    pub fn backup_main_to(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = Self::open_file(destination.as_ref().to_string_lossy().as_ref());
        self.backup_main(&destination)
    }

    pub fn sql_has_syntax_error(&self, sql: &str) -> Option<(String, usize)> {
        let sql = CString::new(sql).ok()?;
        let mut remaining_sql = sql.as_c_str();
        let sql_start = remaining_sql.as_ptr();

        let mut alter_table = None;
        while {
            let remaining_sql_str = remaining_sql.to_str().ok()?.trim();
            let any_remaining_sql = remaining_sql_str != ";" && !remaining_sql_str.is_empty();
            if any_remaining_sql {
                alter_table = parse_alter_table(remaining_sql_str);
            }
            any_remaining_sql
        } {
            let mut raw_statement = ptr::null_mut::<sqlite3_stmt>();
            let mut remaining_sql_ptr = ptr::null();

            // An ALTER TABLE statement is prepared against a throwaway database that already
            // contains the table, because `prepare` checks the table's existence before it
            // checks the rest of the syntax. The stub stays bound for the whole iteration: the
            // error message read below points into the connection that produced it.
            let stub_connection = alter_table
                .as_ref()
                .and_then(|(table_to_alter, column)| alter_table_stub(table_to_alter, column));
            let prepare_connection = stub_connection.as_ref().unwrap_or(self);

            // SAFETY: `prepare_connection` is either `self` or the stub bound above, so it is
            // alive for the rest of the iteration; `remaining_sql` points into `sql`, which is
            // alive for the whole function; and both out-parameters are local bindings.
            unsafe {
                sqlite3_prepare_v2(
                    prepare_connection.sqlite3,
                    remaining_sql.as_ptr(),
                    -1,
                    &raw mut raw_statement,
                    &raw mut remaining_sql_ptr,
                )
            };

            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            // SAFETY: same live connection as above; this only reads its last error offset.
            let offset = unsafe { sqlite3_error_offset(prepare_connection.sqlite3) };

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let offset = 0;

            // SAFETY: same live connection; both calls only read connection-owned state, and
            // the returned message stays valid until the next call on that connection.
            let errcode = unsafe { sqlite3_errcode(prepare_connection.sqlite3) };
            // SAFETY: as above.
            let errmsg = unsafe { sqlite3_errmsg(prepare_connection.sqlite3) };

            // SAFETY: `raw_statement` is whatever `prepare` produced; `sqlite3_finalize`
            // accepts a null handle and frees a compiled statement exactly once.
            unsafe { sqlite3_finalize(raw_statement) };

            if errcode == 1 && offset >= 0 {
                // The caller reports an offset into the SQL it passed in, so the position of
                // this statement inside that buffer is added to the offset SQLite reported.
                // Only the two addresses are subtracted here; nothing is dereferenced.
                let sub_statement_correction = remaining_sql.as_ptr() as usize - sql_start as usize;
                // SAFETY: `errmsg` points into memory owned by `prepare_connection`, which is
                // still alive, and it is NUL-terminated by SQLite.
                let err_msg =
                    String::from_utf8_lossy(unsafe { CStr::from_ptr(errmsg.cast()).to_bytes() })
                        .into_owned();

                return Some((err_msg, offset as usize + sub_statement_correction));
            }

            // `prepare` leaves the tail unset only for a statement it refused, which the error
            // branch above already reported. Stopping here keeps the pointer out of
            // `CStr::from_ptr`.
            if remaining_sql_ptr.is_null() {
                log::error!(
                    "sqlite3_prepare_v2 reported neither an error nor the remaining SQL while \
                     syntax checking"
                );
                return None;
            }
            // SAFETY: non-null (checked above) and NUL-terminated; the tail points into `sql`,
            // which outlives this borrow.
            remaining_sql = unsafe { CStr::from_ptr(remaining_sql_ptr) };
            alter_table = None;
        }
        None
    }

    pub(crate) fn last_error(&self) -> Result<()> {
        // SAFETY: `self.sqlite3` is a live handle owned by this `Connection`; the calls below
        // only read connection-owned state.
        let code = unsafe { sqlite3_errcode(self.sqlite3) };
        const NON_ERROR_CODES: &[i32] = &[SQLITE_OK, SQLITE_ROW];
        if NON_ERROR_CODES.contains(&code) {
            return Ok(());
        }

        // SAFETY: same live handle. SQLite documents the returned message pointer as valid
        // until the next call on the connection, and the bytes are copied before that.
        let message = unsafe { sqlite3_errmsg(self.sqlite3) };
        let message = if message.is_null() {
            None
        } else {
            // SAFETY: non-null (checked above) and NUL-terminated by SQLite.
            Some(
                String::from_utf8_lossy(unsafe { CStr::from_ptr(message.cast()).to_bytes() })
                    .into_owned(),
            )
        };

        anyhow::bail!("Sqlite call failed with code {code} and message: {message:?}")
    }

    pub(crate) fn with_write<T>(&self, callback: impl FnOnce(&Connection) -> T) -> T {
        /// Clears the write flag however the scope ends, so a panicking callback cannot leave
        /// the connection marked writable for the next user of the queue.
        struct WriteGuard<'a>(&'a RefCell<bool>);

        impl Drop for WriteGuard<'_> {
            fn drop(&mut self) {
                *self.0.borrow_mut() = false;
            }
        }

        *self.write.borrow_mut() = true;
        let write_guard = WriteGuard(&self.write);
        let result = callback(self);
        drop(write_guard);
        result
    }
}

/// Builds a throwaway in-memory database holding the table an `ALTER TABLE` statement names, so
/// the statement can be syntax-checked without touching the schema of the connection under test.
///
/// Returns `None` when the stub cannot be created, which happens when the table or column name
/// taken from the statement is not a bare identifier. The caller then checks the statement
/// against its own connection instead of failing a migration over the probe.
fn alter_table_stub(table_to_alter: &str, column: &str) -> Option<Connection> {
    let connection = Connection::open_memory(None);
    let create_table = format!("CREATE TABLE {table_to_alter}({column})");
    let result = connection
        .exec(&create_table)
        .and_then(|mut create_table| create_table());
    match result {
        Ok(()) => Some(connection),
        Err(error) => {
            log::error!("Could not build the ALTER TABLE probe table: {error:#}");
            None
        }
    }
}

fn parse_alter_table(remaining_sql_str: &str) -> Option<(String, String)> {
    let remaining_sql_str = remaining_sql_str.to_lowercase();
    if remaining_sql_str.starts_with("alter")
        && let Some(table_offset) = remaining_sql_str.find("table")
    {
        let after_table_offset = table_offset + "table".len();
        let table_to_alter = remaining_sql_str
            .chars()
            .skip(after_table_offset)
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| !c.is_whitespace())
            .collect::<String>();
        if !table_to_alter.is_empty() {
            let column_name = if let Some(rename_offset) = remaining_sql_str.find("rename column") {
                let after_rename_offset = rename_offset + "rename column".len();
                remaining_sql_str
                    .chars()
                    .skip(after_rename_offset)
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| !c.is_whitespace())
                    .collect::<String>()
            } else if let Some(drop_offset) = remaining_sql_str.find("drop column") {
                let after_drop_offset = drop_offset + "drop column".len();
                remaining_sql_str
                    .chars()
                    .skip(after_drop_offset)
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| !c.is_whitespace())
                    .collect::<String>()
            } else {
                "__place_holder_column_for_syntax_checking".to_owned()
            };
            return Some((table_to_alter, column_name));
        }
    }
    None
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: `sqlite3` is the handle `open_with_flags` created and checked non-null, and
        // `Drop` takes `&mut self`, so this is the only place that closes it. Every statement
        // borrows the connection (`Statement<'a>`), so none can outlive this call.
        unsafe { sqlite3_close(self.sqlite3) };
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use indoc::indoc;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::connection::Connection;

    static NEXT_NAMED_MEMORY_DB_ID: AtomicUsize = AtomicUsize::new(0);

    fn unique_named_memory_db(prefix: &str) -> String {
        format!(
            "{prefix}_{}_{}",
            std::process::id(),
            NEXT_NAMED_MEMORY_DB_ID.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn literal_named_memory_paths(name: &str) -> [String; 3] {
        let main = format!("file:{name}?mode=memory&cache=shared");
        [main.clone(), format!("{main}-wal"), format!("{main}-shm")]
    }

    struct NamedMemoryPathGuard {
        paths: [String; 3],
    }

    impl NamedMemoryPathGuard {
        fn new(name: &str) -> Self {
            let paths = literal_named_memory_paths(name);
            for path in &paths {
                let _ = fs::remove_file(path);
            }
            Self { paths }
        }
    }

    impl Drop for NamedMemoryPathGuard {
        fn drop(&mut self) {
            for path in &self.paths {
                let _ = fs::remove_file(path);
            }
        }
    }

    #[test]
    fn string_round_trips() -> Result<()> {
        let connection = Connection::open_memory(Some("string_round_trips"));
        connection
            .exec(indoc! {"
            CREATE TABLE text (
                text TEXT
            );"})
            .unwrap()()
        .unwrap();

        let text = "Some test text";

        connection
            .exec_bound("INSERT INTO text (text) VALUES (?);")
            .unwrap()(text)
        .unwrap();

        assert_eq!(
            connection.select_row("SELECT text FROM text;").unwrap()().unwrap(),
            Some(text.to_string())
        );

        Ok(())
    }

    #[test]
    fn tuple_round_trips() {
        let connection = Connection::open_memory(Some("tuple_round_trips"));
        connection
            .exec(indoc! {"
                CREATE TABLE test (
                    text TEXT,
                    integer INTEGER,
                    blob BLOB
                );"})
            .unwrap()()
        .unwrap();

        let tuple1 = ("test".to_string(), 64, vec![0, 1, 2, 4, 8, 16, 32, 64]);
        let tuple2 = ("test2".to_string(), 32, vec![64, 32, 16, 8, 4, 2, 1, 0]);

        let mut insert = connection
            .exec_bound::<(String, usize, Vec<u8>)>(
                "INSERT INTO test (text, integer, blob) VALUES (?, ?, ?)",
            )
            .unwrap();

        insert(tuple1.clone()).unwrap();
        insert(tuple2.clone()).unwrap();

        assert_eq!(
            connection
                .select::<(String, usize, Vec<u8>)>("SELECT * FROM test")
                .unwrap()()
            .unwrap(),
            vec![tuple1, tuple2]
        );
    }

    #[test]
    fn bool_round_trips() {
        let connection = Connection::open_memory(Some("bool_round_trips"));
        connection
            .exec(indoc! {"
                CREATE TABLE bools (
                    t INTEGER,
                    f INTEGER
                );"})
            .unwrap()()
        .unwrap();

        connection
            .exec_bound("INSERT INTO bools(t, f) VALUES (?, ?)")
            .unwrap()((true, false))
        .unwrap();

        assert_eq!(
            connection
                .select_row::<(bool, bool)>("SELECT * FROM bools;")
                .unwrap()()
            .unwrap(),
            Some((true, false))
        );
    }

    #[test]
    fn backup_works() {
        let connection1 = Connection::open_memory(Some("backup_works"));
        connection1
            .exec(indoc! {"
                CREATE TABLE blobs (
                    data BLOB
                );"})
            .unwrap()()
        .unwrap();
        let blob = vec![0, 1, 2, 4, 8, 16, 32, 64];
        connection1
            .exec_bound::<Vec<u8>>("INSERT INTO blobs (data) VALUES (?);")
            .unwrap()(blob.clone())
        .unwrap();

        // Backup connection1 to connection2
        let connection2 = Connection::open_memory(Some("backup_works_other"));
        connection1.backup_main(&connection2).unwrap();

        // Delete the added blob and verify its deleted on the other side
        let read_blobs = connection1
            .select::<Vec<u8>>("SELECT * FROM blobs;")
            .unwrap()()
        .unwrap();
        assert_eq!(read_blobs, vec![blob]);
    }

    #[test]
    fn named_memory_connections_do_not_create_literal_backing_files() {
        let name = unique_named_memory_db("named_memory_connections_do_not_create_backing_files");
        let guard = NamedMemoryPathGuard::new(&name);

        let connection1 = Connection::open_memory(Some(&name));
        connection1
            .exec(indoc! {"
                CREATE TABLE shared (
                    value INTEGER
                )"})
            .unwrap()()
        .unwrap();
        connection1
            .exec("INSERT INTO shared (value) VALUES (7)")
            .unwrap()()
        .unwrap();

        let connection2 = Connection::open_memory(Some(&name));
        assert_eq!(
            connection2
                .select_row::<i64>("SELECT value FROM shared")
                .unwrap()()
            .unwrap(),
            Some(7)
        );

        for path in &guard.paths {
            assert!(
                fs::metadata(path).is_err(),
                "named in-memory database unexpectedly created backing file {path}"
            );
        }
    }

    #[test]
    fn multi_step_statement_works() {
        let connection = Connection::open_memory(Some("multi_step_statement_works"));

        connection
            .exec(indoc! {"
                CREATE TABLE test (
                    col INTEGER
                )"})
            .unwrap()()
        .unwrap();

        connection
            .exec(indoc! {"
            INSERT INTO test(col) VALUES (2)"})
            .unwrap()()
        .unwrap();

        assert_eq!(
            connection
                .select_row::<usize>("SELECT * FROM test")
                .unwrap()()
            .unwrap(),
            Some(2)
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    #[test]
    fn test_sql_has_syntax_errors() {
        let connection = Connection::open_memory(Some("test_sql_has_syntax_errors"));
        let first_stmt =
            "CREATE TABLE kv_store(key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT ;";
        let second_stmt = "SELECT FROM";

        let second_offset = connection.sql_has_syntax_error(second_stmt).unwrap().1;

        let res = connection
            .sql_has_syntax_error(&format!("{}\n{}", first_stmt, second_stmt))
            .map(|(_, offset)| offset);

        assert_eq!(res, Some(first_stmt.len() + second_offset + 1));
    }

    #[test]
    fn test_alter_table_syntax() {
        let connection = Connection::open_memory(Some("test_alter_table_syntax"));

        assert!(
            connection
                .sql_has_syntax_error("ALTER TABLE test ADD x TEXT")
                .is_none()
        );

        assert!(
            connection
                .sql_has_syntax_error("ALTER TABLE test AAD x TEXT")
                .is_some()
        );
    }

    #[test]
    fn alter_table_with_quoted_identifier_does_not_panic() {
        let connection = Connection::open_memory(Some("alter_table_with_quoted_identifier"));

        // `parse_alter_table` keeps the next whitespace-delimited token, so a quoted table name
        // yields a fragment that `CREATE TABLE` rejects on the probe database. The probe has to
        // fall back to the real connection rather than failing that statement outright.
        assert!(
            connection
                .sql_has_syntax_error(r#"ALTER TABLE "my table" RENAME COLUMN a TO b"#)
                .is_some()
        );
    }
}
