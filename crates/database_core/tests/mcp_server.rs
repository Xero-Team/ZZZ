//! End-to-end tests for the read-only MCP server.
//!
//! These spawn the real `database-mcp-server` binary and talk JSON-RPC over
//! stdin/stdout; nothing here calls the request handlers directly. The settings
//! file contains deliberately credential-shaped fields so the tests can prove
//! that no tool output echoes them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use database_core::{
    ConnectionConfig, ConnectionId, DataSource, DatabaseDriver, SQLITE_DRIVER_ID, SqliteDriver,
};
use serde_json::{Value as Json, json};

fn tool_names(response: &Json) -> Vec<String> {
    response
        .pointer("/result/tools")
        .and_then(Json::as_array)
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Json::as_str))
        .map(str::to_owned)
        .collect()
}

fn server_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_database-mcp-server"))
}

/// A simple bounds check so a wedged server fails the test instead of hanging.
///
/// A detached thread sends the deadline through an `async_channel`, because the
/// workspace forbids `smol::Timer` in tests and this crate has no executor.
async fn with_timeout<F: std::future::Future>(future: F) -> F::Output {
    use futures::future::Either;

    let (deadline, reached) = async_channel::bounded(1);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        if deadline.try_send(()).is_err() {
            // The test finished before the deadline; nothing to do.
        }
    });

    let future = std::pin::pin!(future);
    let reached = std::pin::pin!(reached.recv());
    match futures::future::select(future, reached).await {
        Either::Left((output, _)) => output,
        Either::Right(_) => panic!("the MCP server did not respond within 20 seconds"),
    }
}

struct McpServer {
    /// Kept for `kill_on_drop`: dropping the server kills the child.
    _child: smol::process::Child,
    stdin: smol::process::ChildStdin,
    stdout: futures::io::BufReader<smol::process::ChildStdout>,
    /// Every raw response line, so a test can assert a secret never appears.
    seen: Vec<String>,
}

impl McpServer {
    fn spawn(settings: &Path, allow_writes: bool) -> Self {
        use smol::process::{Command, Stdio};

        let mut command = Command::new(server_binary());
        command.arg("--settings").arg(settings);
        if allow_writes {
            command.arg("--allow-writes");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().expect("spawn the MCP server");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = futures::io::BufReader::new(child.stdout.take().expect("piped stdout"));
        Self {
            _child: child,
            stdin,
            stdout,
            seen: Vec::new(),
        }
    }

    async fn request(&mut self, id: u64, method: &str, params: Json) -> Json {
        use futures::AsyncWriteExt as _;

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&message).expect("encode the request");
        with_timeout(async {
            self.stdin
                .write_all(line.as_bytes())
                .await
                .expect("write the request");
            self.stdin
                .write_all(b"\n")
                .await
                .expect("write the newline");
            self.stdin.flush().await.expect("flush the request");
        })
        .await;
        self.read_response().await
    }

    async fn read_response(&mut self) -> Json {
        use futures::AsyncBufReadExt as _;

        let mut line = String::new();
        let read = with_timeout(self.stdout.read_line(&mut line)).await;
        read.expect("read a response line");
        assert!(!line.is_empty(), "the MCP server closed stdout");
        self.seen.push(line.clone());
        serde_json::from_str(&line).expect("decode the response")
    }

    async fn call_tool(&mut self, id: u64, name: &str, arguments: Json) -> Json {
        let response = self
            .request(
                id,
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        assert_eq!(
            response.pointer("/result/isError"),
            Some(&Json::Bool(false)),
            "{response}"
        );
        let text = response
            .pointer("/result/content/0/text")
            .and_then(Json::as_str)
            .expect("tool content text");
        serde_json::from_str(text).expect("decode the tool result")
    }

    async fn call_tool_error(&mut self, id: u64, name: &str, arguments: Json) -> String {
        let response = self
            .request(
                id,
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        assert_eq!(
            response.pointer("/result/isError"),
            Some(&Json::Bool(true)),
            "{response}"
        );
        response
            .pointer("/result/content/0/text")
            .and_then(Json::as_str)
            .expect("tool error text")
            .to_owned()
    }

    fn shutdown(self) {
        // Dropping the child runs its `kill_on_drop` path.
    }
}

const SETTINGS_SECRET: &str = "mcp-test-password-do-not-leak";

fn write_settings(directory: &Path, database: &Path) -> PathBuf {
    let path = directory.join("settings.json");
    let settings = json!({
        "database": {
            "connections": [
                {
                    "id": "main",
                    "name": "Main",
                    "driver": SQLITE_DRIVER_ID,
                    "path": database,
                    // Credential-shaped fields the server must never echo.
                    "password": SETTINGS_SECRET,
                    "token": SETTINGS_SECRET,
                },
                {
                    "id": "warehouse",
                    "name": "Warehouse",
                    "driver": "postgres",
                    "host": "db.example.com",
                    "port": 5432,
                },
            ],
        },
    });
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&settings).expect("encode settings"),
    )
    .expect("write settings");
    path
}

async fn create_database(path: &Path) {
    let config = ConnectionConfig {
        id: ConnectionId("main".to_owned()),
        display_name: "Main".to_owned(),
        driver_id: SQLITE_DRIVER_ID.to_owned(),
        source: DataSource::File {
            path: path.to_path_buf(),
        },
        host: None,
        port: None,
        user: None,
        database: None,
        options: Default::default(),
    };
    let connection = SqliteDriver
        .connect(&config, None)
        .await
        .expect("open the fixture database");
    for sql in [
        "CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE books (\
            id INTEGER PRIMARY KEY, \
            author_id INTEGER REFERENCES authors(id), \
            title TEXT, \
            payload BLOB\
        )",
        "INSERT INTO authors (id, name) VALUES (1, 'Ada')",
        "INSERT INTO books (id, author_id, title, payload) \
         VALUES (10, 1, 'Notes', X'00FF10')",
        "INSERT INTO books (id, author_id, title, payload) VALUES (11, NULL, NULL, NULL)",
    ] {
        let executed = connection.execute(sql, &[]).await.expect(sql);
        let rows = executed.rows;
        while let Some(item) = rows.next().await {
            item.expect("drain the statement");
        }
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    settings: PathBuf,
}

async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("create a temporary directory");
    let database = directory.path().join("library.sqlite");
    // The driver opens with READWRITE, not CREATE, so the file must exist.
    std::fs::File::create(&database).expect("create the database file");
    create_database(&database).await;
    let settings = write_settings(directory.path(), &database);
    Fixture {
        _directory: directory,
        settings,
    }
}

#[test]
fn initialize_and_tools_list_cover_the_read_only_surface() {
    smol::block_on(async {
        let fixture = fixture().await;
        let mut server = McpServer::spawn(&fixture.settings, false);

        let initialized = server
            .request(
                1,
                "initialize",
                json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
            )
            .await;
        assert_eq!(
            initialized.pointer("/result/protocolVersion"),
            Some(&Json::String("2025-06-18".to_owned()))
        );
        assert_eq!(
            initialized.pointer("/result/capabilities/tools"),
            Some(&json!({}))
        );
        assert_eq!(
            initialized.pointer("/result/serverInfo/name"),
            Some(&Json::String("zzz-database".to_owned()))
        );

        let tools = server.request(2, "tools/list", json!({})).await;
        assert_eq!(
            tool_names(&tools),
            vec!["list_connections", "describe_table", "run_read_query"]
        );

        server.shutdown();
    });
}

#[test]
fn connections_are_listed_without_paths_or_credentials() {
    smol::block_on(async {
        let fixture = fixture().await;
        let mut server = McpServer::spawn(&fixture.settings, false);

        let listed = server.call_tool(1, "list_connections", json!({})).await;
        let connections = listed
            .get("connections")
            .and_then(Json::as_array)
            .expect("connections array");
        assert_eq!(connections.len(), 2);

        let main = &connections[0];
        assert_eq!(main.get("id"), Some(&Json::String("main".to_owned())));
        assert_eq!(main.get("name"), Some(&Json::String("Main".to_owned())));
        assert_eq!(main.get("available"), Some(&Json::Bool(true)));
        // The saved path is not part of the tool contract.
        assert!(main.get("path").is_none(), "{main}");

        let warehouse = &connections[1];
        assert_eq!(
            warehouse.get("available"),
            Some(&Json::Bool(false)),
            "a driver this server does not bundle is listed but not openable"
        );
        assert!(warehouse.get("reason").is_some());

        // Every response line, including the settings warning field, is clean.
        let transcript = server.seen.join("\n");
        assert!(!transcript.contains(SETTINGS_SECRET), "{transcript}");

        server.shutdown();
    });
}

#[test]
fn describe_table_reports_columns_keys_and_foreign_keys() {
    smol::block_on(async {
        let fixture = fixture().await;
        let mut server = McpServer::spawn(&fixture.settings, false);

        let schema = server
            .call_tool(
                1,
                "describe_table",
                json!({ "connection": "main", "table": "books" }),
            )
            .await;
        assert_eq!(schema.get("table"), Some(&Json::String("books".to_owned())));
        assert_eq!(schema.get("kind"), Some(&Json::String("table".to_owned())));
        assert_eq!(schema.get("primary_key"), Some(&json!(["id"])));

        let columns = schema
            .get("columns")
            .and_then(Json::as_array)
            .expect("columns array");
        let names: Vec<&str> = columns
            .iter()
            .filter_map(|column| column.get("name").and_then(Json::as_str))
            .collect();
        assert_eq!(names, vec!["id", "author_id", "title", "payload"]);
        assert_eq!(
            columns[0].get("primary_key"),
            Some(&Json::Bool(true)),
            "{columns:?}"
        );

        let foreign_keys = schema
            .get("foreign_keys")
            .and_then(Json::as_array)
            .expect("foreign keys array");
        assert_eq!(foreign_keys.len(), 1, "{foreign_keys:?}");
        assert_eq!(
            foreign_keys[0].get("referenced_table"),
            Some(&Json::String("authors".to_owned()))
        );
        assert_eq!(
            foreign_keys[0].get("referenced_columns"),
            Some(&json!(["id"]))
        );

        let missing = server
            .call_tool_error(
                2,
                "describe_table",
                json!({ "connection": "main", "table": "missing" }),
            )
            .await;
        assert!(!missing.is_empty());

        server.shutdown();
    });
}

#[test]
fn read_queries_return_rows_with_blob_digests_and_writes_are_refused() {
    smol::block_on(async {
        let fixture = fixture().await;
        let mut server = McpServer::spawn(&fixture.settings, false);

        let result = server
            .call_tool(
                1,
                "run_read_query",
                json!({
                    "connection": "main",
                    "sql": "SELECT id, title, payload FROM books ORDER BY id",
                }),
            )
            .await;
        assert_eq!(result.get("truncated"), Some(&Json::Bool(false)));
        let rows = result
            .get("rows")
            .and_then(Json::as_array)
            .expect("rows array");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], json!([10, "Notes", "[blob 3B: 00ff10]"]));
        assert_eq!(rows[1], json!([11, Json::Null, Json::Null]));

        let refused = server
            .call_tool_error(
                2,
                "run_read_query",
                json!({ "connection": "main", "sql": "DELETE FROM books" }),
            )
            .await;
        assert!(refused.contains("read-only"), "{refused}");

        let multiple = server
            .call_tool_error(
                3,
                "run_read_query",
                json!({ "connection": "main", "sql": "SELECT 1; SELECT 2" }),
            )
            .await;
        assert!(multiple.contains("one statement"), "{multiple}");

        let still_there = server
            .call_tool(
                4,
                "run_read_query",
                json!({ "connection": "main", "sql": "SELECT count(*) FROM books" }),
            )
            .await;
        assert_eq!(still_there.get("rows"), Some(&json!([[2]])));

        let transcript = server.seen.join("\n");
        assert!(!transcript.contains(SETTINGS_SECRET), "{transcript}");

        server.shutdown();
    });
}

#[test]
fn write_queries_need_the_explicit_flag() {
    smol::block_on(async {
        let fixture = fixture().await;
        let mut server = McpServer::spawn(&fixture.settings, true);

        let tools = server.request(1, "tools/list", json!({})).await;
        assert_eq!(
            tool_names(&tools),
            vec![
                "list_connections",
                "describe_table",
                "run_read_query",
                "run_write_query"
            ]
        );

        let written = server
            .call_tool(
                2,
                "run_write_query",
                json!({
                    "connection": "main",
                    "sql": "INSERT INTO books (id, title) VALUES (12, 'Added')",
                }),
            )
            .await;
        assert_eq!(written.get("affected_rows"), Some(&json!(1)));

        let read = server
            .call_tool(
                3,
                "run_read_query",
                json!({
                    "connection": "main",
                    "sql": "SELECT title FROM books WHERE id = 12",
                }),
            )
            .await;
        assert_eq!(read.get("rows"), Some(&json!([["Added"]])));

        server.shutdown();
    });
}
