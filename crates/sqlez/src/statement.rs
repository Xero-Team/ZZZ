use std::ffi::{CStr, CString, c_int};
use std::marker::PhantomData;
use std::{ptr, slice, str};

use anyhow::{Context as _, Result, bail};
use libsqlite3_sys::*;

use crate::bindable::{Bind, Column};
use crate::connection::Connection;

pub struct Statement<'a> {
    /// vector of pointers to the raw SQLite statement objects.
    /// it holds the actual prepared statements that will be executed.
    pub raw_statements: Vec<*mut sqlite3_stmt>,
    /// Index of the current statement being executed from the `raw_statements` vector.
    current_statement: usize,
    /// A reference to the database connection.
    /// This is used to execute the statements and check for errors.
    connection: &'a Connection,
    ///Indicates that the `Statement` struct is tied to the lifetime of the SQLite statement
    phantom: PhantomData<sqlite3_stmt>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepResult {
    Row,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SqlType {
    Text,
    Integer,
    Blob,
    Float,
    Null,
}

/// A value bound into every prepared statement that has a parameter at the bound index.
///
/// The variants keep the pointer handling in `Statement::bind_index_with`, so each public
/// `bind_*` method stays a single sentence and no caller has to write `unsafe`.
enum BindValue<'a> {
    Blob(&'a [u8]),
    Text(&'a str),
    Double(f64),
    Int(i32),
    Int64(i64),
    Null,
}

impl<'a> Statement<'a> {
    pub fn prepare<T: AsRef<str>>(connection: &'a Connection, query: T) -> Result<Self> {
        let mut statement = Self {
            raw_statements: Default::default(),
            current_statement: 0,
            connection,
            phantom: PhantomData,
        };
        let sql = CString::new(query.as_ref()).context("Error creating cstr")?;
        let mut remaining_sql = sql.as_c_str();
        while {
            let remaining_sql_str = remaining_sql
                .to_str()
                .context("Parsing remaining sql")?
                .trim();
            remaining_sql_str != ";" && !remaining_sql_str.is_empty()
        } {
            let mut raw_statement = ptr::null_mut::<sqlite3_stmt>();
            let mut remaining_sql_ptr = ptr::null();
            // SAFETY: `connection.sqlite3` is a live handle borrowed for `'a`, so it outlives
            // this loop. `remaining_sql` points into `sql`, which is alive for the whole
            // function. Both out-parameters are local `let mut` bindings, so SQLite writes
            // through valid, aligned, exclusive pointers.
            unsafe {
                sqlite3_prepare_v2(
                    connection.sqlite3,
                    remaining_sql.as_ptr(),
                    -1,
                    &raw mut raw_statement,
                    &raw mut remaining_sql_ptr,
                )
            };

            connection
                .last_error()
                .with_context(|| format!("Prepare call failed for query:\n{}", query.as_ref()))?;

            // SQLite sets the tail pointer to NULL when it reports an error and does not
            // guarantee a tail for a statement it refused to compile, so the pointer is
            // checked instead of assumed. Dereferencing it unguarded would build a `CStr`
            // from a null pointer.
            if remaining_sql_ptr.is_null() {
                bail!(
                    "Prepare call did not report the remaining SQL for query:\n{}",
                    query.as_ref()
                );
            }
            // SAFETY: non-null (checked above) and NUL-terminated; the tail points at or
            // before the end of `sql`, which outlives this borrow.
            remaining_sql = unsafe { CStr::from_ptr(remaining_sql_ptr) };

            // SQL text that holds no statement (only whitespace or comments) compiles to a
            // null handle with SQLITE_OK. Recording that handle would hand a null statement
            // to `sqlite3_step` later, so the loop keeps only real statements and the
            // emptiness check below turns such input into an error.
            if raw_statement.is_null() {
                continue;
            }
            statement.raw_statements.push(raw_statement);

            if !connection.can_write() {
                // SAFETY: `raw_statement` was checked non-null above and is now owned by
                // `statement`; `sqlite3_stmt_readonly` only inspects the statement.
                let is_write_statement = unsafe { sqlite3_stmt_readonly(raw_statement) == 0 };
                if is_write_statement {
                    // SAFETY: `sqlite3_sql` returns a NUL-terminated pointer into the
                    // SQLite-owned copy of the statement text, which stays valid until the
                    // statement is finalized. It only returns null for a null statement.
                    let statement_sql = unsafe { sqlite3_sql(raw_statement) };
                    let text = unsafe { CStr::from_ptr(statement_sql) }.to_str()?;

                    bail!(
                        "Write statement prepared with connection that is not write capable. SQL:\n{} ",
                        text
                    )
                }
            }
        }

        if statement.raw_statements.is_empty() {
            bail!("Query produced no SQL statement:\n{}", query.as_ref());
        }

        Ok(statement)
    }

    fn current_statement(&self) -> *mut sqlite3_stmt {
        *self
            .raw_statements
            .get(self.current_statement)
            .expect("invariant: `prepare` rejects SQL that yields no statement")
    }

    pub fn reset(&mut self) {
        // SAFETY: every pointer in `raw_statements` was returned by `sqlite3_prepare_v2` and
        // is finalized exactly once, in `Drop`, so it is still owned here. `sqlite3_reset`
        // only re-arms the statement for reuse.
        unsafe {
            for raw_statement in &self.raw_statements {
                sqlite3_reset(*raw_statement);
            }
        }
        self.current_statement = 0;
    }

    pub fn parameter_count(&self) -> i32 {
        // SAFETY: as in `reset`; `sqlite3_bind_parameter_count` reads statement metadata and
        // does not take ownership.
        unsafe {
            self.raw_statements
                .iter()
                .map(|raw_statement| sqlite3_bind_parameter_count(*raw_statement))
                .max()
                .unwrap_or(0)
        }
    }

    fn bind_index_with(&self, index: i32, value: BindValue<'_>) -> Result<()> {
        let mut any_succeed = false;
        for raw_statement in &self.raw_statements {
            // SAFETY: every pointer in `raw_statements` is a live statement owned by this
            // `Statement`. `index` is compared against that statement's own parameter count
            // before it is used.
            let parameter_count = unsafe { sqlite3_bind_parameter_count(*raw_statement) };
            if index > parameter_count {
                continue;
            }
            // SAFETY: each arm performs exactly one bind on the live statement checked above.
            // `SQLITE_TRANSIENT` makes SQLite copy the bound bytes, so a slice or string
            // argument does not have to outlive the call.
            match value {
                BindValue::Blob(blob) => unsafe {
                    sqlite3_bind_blob(
                        *raw_statement,
                        index,
                        blob.as_ptr().cast(),
                        blob.len() as c_int,
                        SQLITE_TRANSIENT(),
                    )
                },
                BindValue::Text(text) => unsafe {
                    sqlite3_bind_text(
                        *raw_statement,
                        index,
                        text.as_ptr().cast(),
                        text.len() as c_int,
                        SQLITE_TRANSIENT(),
                    )
                },
                BindValue::Double(double) => unsafe {
                    sqlite3_bind_double(*raw_statement, index, double)
                },
                BindValue::Int(int) => unsafe { sqlite3_bind_int(*raw_statement, index, int) },
                BindValue::Int64(int) => unsafe { sqlite3_bind_int64(*raw_statement, index, int) },
                BindValue::Null => unsafe { sqlite3_bind_null(*raw_statement, index) },
            };
            self.connection
                .last_error()
                .with_context(|| format!("Failed to bind value at index {index}"))?;
            any_succeed = true;
        }
        if any_succeed {
            Ok(())
        } else {
            anyhow::bail!("Failed to bind parameters")
        }
    }

    pub fn bind_blob(&self, index: i32, blob: &[u8]) -> Result<()> {
        self.bind_index_with(index, BindValue::Blob(blob))
    }

    pub fn column_blob(&mut self, index: i32) -> Result<&[u8]> {
        let index = index as c_int;
        // SAFETY: `current_statement` is a live statement owned by this `Statement`. The
        // function accepts a NULL handle and reports it by returning null, which the check
        // below turns into an empty slice.
        let pointer = unsafe { sqlite3_column_blob(self.current_statement(), index) };

        self.connection
            .last_error()
            .with_context(|| format!("Failed to read blob at index {index}"))?;
        if pointer.is_null() {
            return Ok(&[]);
        }
        // SAFETY: same live statement; `sqlite3_column_bytes` reports the length of the blob
        // that the non-null pointer above refers to.
        let len = unsafe { sqlite3_column_bytes(self.current_statement(), index) as usize };
        self.connection
            .last_error()
            .with_context(|| format!("Failed to read length of blob at index {index}"))?;

        // SAFETY: SQLite owns the blob bytes until the statement leaves this row, and the
        // `&mut self` borrow keeps the statement from advancing while the slice is alive. The
        // pointer is non-null (checked above) and `len`, read for the same column, is the
        // number of initialized bytes it points at.
        unsafe { Ok(slice::from_raw_parts(pointer.cast::<u8>(), len)) }
    }

    pub fn bind_double(&self, index: i32, double: f64) -> Result<()> {
        self.bind_index_with(index, BindValue::Double(double))
    }

    pub fn column_double(&self, index: i32) -> Result<f64> {
        let index = index as c_int;
        // SAFETY: `current_statement` is a live statement owned by this `Statement`, and the
        // function only reads the column value.
        let result = unsafe { sqlite3_column_double(self.current_statement(), index) };
        self.connection
            .last_error()
            .with_context(|| format!("Failed to read double at index {index}"))?;
        Ok(result)
    }

    pub fn bind_int(&self, index: i32, int: i32) -> Result<()> {
        self.bind_index_with(index, BindValue::Int(int))
    }

    pub fn column_int(&self, index: i32) -> Result<i32> {
        let index = index as c_int;
        // SAFETY: as in `column_double`; the call only reads the column value.
        let result = unsafe { sqlite3_column_int(self.current_statement(), index) };
        self.connection
            .last_error()
            .with_context(|| format!("Failed to read int at index {index}"))?;
        Ok(result)
    }

    pub fn bind_int64(&self, index: i32, int: i64) -> Result<()> {
        self.bind_index_with(index, BindValue::Int64(int))
    }

    pub fn column_int64(&self, index: i32) -> Result<i64> {
        let index = index as c_int;
        // SAFETY: as in `column_double`; the call only reads the column value.
        let result = unsafe { sqlite3_column_int64(self.current_statement(), index) };
        self.connection
            .last_error()
            .with_context(|| format!("Failed to read i64 at index {index}"))?;
        Ok(result)
    }

    pub fn bind_null(&self, index: i32) -> Result<()> {
        self.bind_index_with(index, BindValue::Null)
    }

    pub fn bind_text(&self, index: i32, text: &str) -> Result<()> {
        self.bind_index_with(index, BindValue::Text(text))
    }

    pub fn column_text(&mut self, index: i32) -> Result<&str> {
        let index = index as c_int;
        // SAFETY: `current_statement` is a live statement owned by this `Statement`. The
        // function accepts a NULL handle and reports it by returning null, which the check
        // below turns into an empty string.
        let pointer = unsafe { sqlite3_column_text(self.current_statement(), index) };

        self.connection
            .last_error()
            .with_context(|| format!("Failed to read text from column {index}"))?;
        if pointer.is_null() {
            return Ok("");
        }
        // SAFETY: same live statement; `sqlite3_column_bytes` reports the length of the text
        // that the non-null pointer above refers to.
        let len = unsafe { sqlite3_column_bytes(self.current_statement(), index) as usize };
        self.connection
            .last_error()
            .with_context(|| format!("Failed to read text length at {index}"))?;

        // SAFETY: SQLite owns the text bytes until the statement leaves this row, and the
        // `&mut self` borrow keeps the statement from advancing while the slice is alive. The
        // pointer is non-null (checked above) and `len`, read for the same column, is the
        // number of initialized bytes it points at. `str::from_utf8` rejects any content that
        // is not valid UTF-8, so the produced reference is well-defined.
        let slice = unsafe { slice::from_raw_parts(pointer, len) };
        Ok(str::from_utf8(slice)?)
    }

    pub fn bind<T: Bind>(&self, value: &T, index: i32) -> Result<i32> {
        debug_assert!(index > 0);
        value.bind(self, index)
    }

    pub fn column<T: Column>(&mut self) -> Result<T> {
        Ok(T::column(self, 0)?.0)
    }

    pub fn column_type(&mut self, index: i32) -> Result<SqlType> {
        // SAFETY: `current_statement` is a live statement owned by this `Statement`, and the
        // call only reads the column's storage class.
        let result = unsafe { sqlite3_column_type(self.current_statement(), index) };
        self.connection.last_error()?;
        match result {
            SQLITE_INTEGER => Ok(SqlType::Integer),
            SQLITE_FLOAT => Ok(SqlType::Float),
            SQLITE_TEXT => Ok(SqlType::Text),
            SQLITE_BLOB => Ok(SqlType::Blob),
            SQLITE_NULL => Ok(SqlType::Null),
            _ => anyhow::bail!("Column type returned was incorrect"),
        }
    }

    pub fn with_bindings(&mut self, bindings: &impl Bind) -> Result<&mut Self> {
        self.bind(bindings, 1)?;
        Ok(self)
    }

    fn step(&mut self) -> Result<StepResult> {
        // SAFETY: `current_statement` is a live statement owned by this `Statement`, and
        // `sqlite3_step` is the only operation that advances it.
        match unsafe { sqlite3_step(self.current_statement()) } {
            SQLITE_ROW => Ok(StepResult::Row),
            SQLITE_DONE => {
                if self.current_statement >= self.raw_statements.len() - 1 {
                    Ok(StepResult::Done)
                } else {
                    self.current_statement += 1;
                    self.step()
                }
            }
            SQLITE_MISUSE => anyhow::bail!("Statement step returned SQLITE_MISUSE"),
            _other_error => {
                self.connection.last_error()?;
                unreachable!("Step returned error code and last error failed to catch it");
            }
        }
    }

    pub fn exec(&mut self) -> Result<()> {
        fn logic(this: &mut Statement) -> Result<()> {
            while this.step()? == StepResult::Row {}
            Ok(())
        }
        let result = logic(self);
        self.reset();
        result
    }

    pub fn map<R>(&mut self, callback: impl FnMut(&mut Statement) -> Result<R>) -> Result<Vec<R>> {
        fn logic<R>(
            this: &mut Statement,
            mut callback: impl FnMut(&mut Statement) -> Result<R>,
        ) -> Result<Vec<R>> {
            let mut mapped_rows = Vec::new();
            while this.step()? == StepResult::Row {
                mapped_rows.push(callback(this)?);
            }
            Ok(mapped_rows)
        }

        let result = logic(self, callback);
        self.reset();
        result
    }

    pub fn rows<R: Column>(&mut self) -> Result<Vec<R>> {
        self.map(|s| s.column::<R>())
    }

    pub fn single<R>(&mut self, callback: impl FnOnce(&mut Statement) -> Result<R>) -> Result<R> {
        fn logic<R>(
            this: &mut Statement,
            callback: impl FnOnce(&mut Statement) -> Result<R>,
        ) -> Result<R> {
            anyhow::ensure!(
                this.step()? == StepResult::Row,
                "single called with query that returns no rows."
            );
            let result = callback(this)?;

            anyhow::ensure!(
                this.step()? == StepResult::Done,
                "single called with a query that returns more than one row."
            );

            Ok(result)
        }
        let result = logic(self, callback);
        self.reset();
        result
    }

    pub fn row<R: Column>(&mut self) -> Result<R> {
        self.single(|this| this.column::<R>())
    }

    pub fn maybe<R>(
        &mut self,
        callback: impl FnOnce(&mut Statement) -> Result<R>,
    ) -> Result<Option<R>> {
        fn logic<R>(
            this: &mut Statement,
            callback: impl FnOnce(&mut Statement) -> Result<R>,
        ) -> Result<Option<R>> {
            if this.step().context("Failed on step call")? != StepResult::Row {
                return Ok(None);
            }

            let result = callback(this)
                .map(|r| Some(r))
                .context("Failed to parse row result")?;

            anyhow::ensure!(
                this.step().context("Second step call")? == StepResult::Done,
                "maybe called with a query that returns more than one row."
            );

            Ok(result)
        }
        let result = logic(self, callback);
        self.reset();
        result
    }

    pub fn maybe_row<R: Column>(&mut self) -> Result<Option<R>> {
        self.maybe(|this| this.column::<R>())
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        // SAFETY: `raw_statements` holds exactly the handles returned by
        // `sqlite3_prepare_v2`; finalizing each one here is the single owner releasing them,
        // and nothing can observe the statement afterwards because `Drop` takes `&mut self`.
        unsafe {
            for raw_statement in &self.raw_statements {
                sqlite3_finalize(*raw_statement);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use indoc::indoc;

    use crate::{
        connection::Connection,
        statement::{Statement, StepResult},
    };

    #[test]
    fn prepare_rejects_sql_without_a_statement() {
        let connection = Connection::open_memory(Some("prepare_rejects_sql_without_a_statement"));

        // Empty input never enters the prepare loop, and input that holds only comments
        // compiles to a null handle. Both must be reported as errors instead of producing a
        // `Statement` that `current_statement` cannot answer for.
        for query in ["", "   ", "-- nothing but a comment"] {
            assert!(
                Statement::prepare(&connection, query).is_err(),
                "expected {query:?} to be rejected"
            );
        }
    }

    #[test]
    fn binding_multiple_statements_with_parameter_gaps() {
        let connection =
            Connection::open_memory(Some("binding_multiple_statements_with_parameter_gaps"));

        connection
            .exec(indoc! {"
            CREATE TABLE test (
                col INTEGER
            )"})
            .unwrap()()
        .unwrap();

        let statement = Statement::prepare(
            &connection,
            indoc! {"
                INSERT INTO test(col) VALUES (?3);
                SELECT * FROM test WHERE col = ?1"},
        )
        .unwrap();

        statement
            .bind_int(1, 1)
            .expect("Could not bind parameter to first index");
        statement
            .bind_int(2, 2)
            .expect("Could not bind parameter to second index");
        statement
            .bind_int(3, 3)
            .expect("Could not bind parameter to third index");
    }

    #[test]
    fn blob_round_trips() {
        let connection1 = Connection::open_memory(Some("blob_round_trips"));
        connection1
            .exec(indoc! {"
                CREATE TABLE blobs (
                    data BLOB
                )"})
            .unwrap()()
        .unwrap();

        let blob = &[0, 1, 2, 4, 8, 16, 32, 64];

        let mut write =
            Statement::prepare(&connection1, "INSERT INTO blobs (data) VALUES (?)").unwrap();
        write.bind_blob(1, blob).unwrap();
        assert_eq!(write.step().unwrap(), StepResult::Done);

        // Read the blob from the
        let connection2 = Connection::open_memory(Some("blob_round_trips"));
        let mut read = Statement::prepare(&connection2, "SELECT * FROM blobs").unwrap();
        assert_eq!(read.step().unwrap(), StepResult::Row);
        assert_eq!(read.column_blob(0).unwrap(), blob);
        assert_eq!(read.step().unwrap(), StepResult::Done);

        // Delete the added blob and verify its deleted on the other side
        connection2.exec("DELETE FROM blobs").unwrap()().unwrap();
        let mut read = Statement::prepare(&connection1, "SELECT * FROM blobs").unwrap();
        assert_eq!(read.step().unwrap(), StepResult::Done);
    }

    #[test]
    pub fn maybe_returns_options() {
        let connection = Connection::open_memory(Some("maybe_returns_options"));
        connection
            .exec(indoc! {"
                CREATE TABLE texts (
                    text TEXT
                )"})
            .unwrap()()
        .unwrap();

        assert!(
            connection
                .select_row::<String>("SELECT text FROM texts")
                .unwrap()()
            .unwrap()
            .is_none()
        );

        let text_to_insert = "This is a test";

        connection
            .exec_bound("INSERT INTO texts VALUES (?)")
            .unwrap()(text_to_insert)
        .unwrap();

        assert_eq!(
            connection.select_row("SELECT text FROM texts").unwrap()().unwrap(),
            Some(text_to_insert.to_string())
        );
    }
}
