//! Read-only MCP server for the database explorer.
//!
//! Speaks the Model Context Protocol over stdin/stdout as newline-delimited
//! JSON-RPC 2.0. It exposes saved SQLite connections to an agent as tools:
//! `list_connections`, `describe_table` and `run_read_query`. `run_write_query`
//! only exists when the process is started with `--allow-writes`.
//!
//! Connections come from the normal ZZZ settings file
//! (`database.connections`), so this server never has its own credential store.
//! File-backed SQLite needs no secret; the settings file never contains one
//! (D-09), and nothing in a tool result includes a path, user or password.
//!
//! Read-only is enforced twice: a statement classifier rejects anything that is
//! not a single read statement, and the SQLite connection itself is switched to
//! `PRAGMA query_only = 1` before the caller's statement runs.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use database_core::{
    ConnectionConfig, ConnectionId, DataSource, DatabaseDriver, DriverConnection, RowStream,
    SqliteDriver, TableRef, Value, is_read_only, split_statements,
};
use serde::Deserialize;
use serde_json::{Value as Json, json};

/// Rows returned by `run_read_query` when the caller does not ask for more.
const DEFAULT_ROW_LIMIT: usize = 500;
/// Upper bound for `run_read_query`, so one call cannot materialize a huge set.
const MAX_ROW_LIMIT: usize = 5_000;
const PROTOCOL_VERSION: &str = "2025-11-25";

/// Only driver bundled here. Connections for other drivers stay listed but are
/// not openable, which keeps the tool honest instead of failing at connect time.
const BUNDLED_DRIVER_ID: &str = database_core::sqlite::SQLITE_DRIVER_ID;

#[derive(Debug, Default)]
struct Args {
    settings: Option<PathBuf>,
    allow_writes: bool,
}

#[derive(Debug, Deserialize, Default)]
struct SettingsFile {
    #[serde(default)]
    database: Option<DatabaseSection>,
}

#[derive(Debug, Deserialize, Default)]
struct DatabaseSection {
    #[serde(default)]
    connections: Vec<SavedConnection>,
}

/// The subset of a saved connection this server understands. Unknown settings
/// (including anything credential-shaped) are ignored by serde and never
/// echoed back.
#[derive(Debug, Deserialize, Clone)]
struct SavedConnection {
    id: String,
    name: String,
    driver: String,
    #[serde(default)]
    path: Option<String>,
}

struct ServerState {
    connections: Vec<SavedConnection>,
    /// Set when the settings file could not be read, so `list_connections` can
    /// say why it is empty instead of pretending there are no connections.
    settings_error: Option<String>,
    allow_writes: bool,
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("database-mcp-server: {error}");
            eprintln!("usage: database-mcp-server [--settings <path>] [--allow-writes]");
            std::process::exit(2);
        }
    };

    let state = load_state(&args);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("database-mcp-server: failed to read stdin: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Json = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_response(
                    &mut stdout,
                    json!({
                        "jsonrpc": "2.0",
                        "id": Json::Null,
                        "error": { "code": -32700, "message": format!("parse error: {error}") },
                    }),
                );
                continue;
            }
        };
        if let Some(response) = handle_request(&state, &request) {
            write_response(&mut stdout, response);
        }
    }
}

fn write_response(stdout: &mut impl Write, response: Json) {
    match serde_json::to_string(&response) {
        Ok(text) => {
            if let Err(error) = writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
                eprintln!("database-mcp-server: failed to write a response: {error}");
            }
        }
        Err(error) => eprintln!("database-mcp-server: failed to encode a response: {error}"),
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--allow-writes" => parsed.allow_writes = true,
            "--settings" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--settings needs a path".to_owned())?;
                parsed.settings = Some(PathBuf::from(path));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

fn load_state(args: &Args) -> ServerState {
    let settings_path = args
        .settings
        .clone()
        .unwrap_or_else(|| paths::config_dir().join("settings.json"));
    match read_settings(&settings_path) {
        Ok(connections) => ServerState {
            connections,
            settings_error: None,
            allow_writes: args.allow_writes,
        },
        Err(error) => {
            // Still serve the protocol so an agent gets a readable tool error
            // rather than a connection failure with no explanation.
            eprintln!(
                "database-mcp-server: could not read {}: {error}",
                settings_path.display()
            );
            ServerState {
                connections: Vec::new(),
                settings_error: Some(format!("could not read the settings file: {error}")),
                allow_writes: args.allow_writes,
            }
        }
    }
}

fn read_settings(path: &Path) -> Result<Vec<SavedConnection>, String> {
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let settings: SettingsFile =
        settings_json::parse_json_with_comments(&content).map_err(|error| error.to_string())?;
    Ok(settings
        .database
        .map(|database| database.connections)
        .unwrap_or_default())
}

fn handle_request(state: &ServerState, request: &Json) -> Option<Json> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Json::as_str)?;
    // Notifications (no id) must not be answered.
    let id = id?;

    let outcome = match method {
        "initialize" => Ok(initialize_result(request)),
        "tools/list" => Ok(tools_list(state)),
        "tools/call" => Ok(smol::block_on(call_tool(state, request))),
        "ping" => Ok(json!({})),
        other => Err((-32601, format!("unknown method: {other}"))),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    })
}

fn initialize_result(request: &Json) -> Json {
    let requested = request
        .pointer("/params/protocolVersion")
        .and_then(Json::as_str);
    json!({
        "protocolVersion": requested.unwrap_or(PROTOCOL_VERSION),
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "zzz-database",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "Database connections are SQLite files. Call list_connections first, \
                         then describe_table or run_read_query with the connection id. Results \
                         are JSON text; BLOB values are digest strings and NULL stays null.",
    })
}

fn tools_list(state: &ServerState) -> Json {
    let mut tools = vec![
        json!({
            "name": "list_connections",
            "description": "List the saved database connections this server can open. \
                            No paths or credentials are returned.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "describe_table",
            "description": "Describe one table: columns, declared types, nullability, primary \
                            key and foreign keys.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connection": { "type": "string", "description": "Connection id from list_connections." },
                    "table": { "type": "string", "description": "Table name, optionally schema-qualified." },
                },
                "required": ["connection", "table"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "run_read_query",
            "description": "Run one read-only statement (SELECT, VALUES, TABLE or EXPLAIN) and \
                            return columns and rows as JSON. Writes are rejected.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connection": { "type": "string", "description": "Connection id from list_connections." },
                    "sql": { "type": "string", "description": "A single read-only statement." },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_ROW_LIMIT,
                        "description": "Maximum rows to return (default 500).",
                    },
                },
                "required": ["connection", "sql"],
                "additionalProperties": false,
            },
        }),
    ];
    if state.allow_writes {
        tools.push(json!({
            "name": "run_write_query",
            "description": "Run one statement that changes data. Only available when the server \
                            was started with --allow-writes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "connection": { "type": "string", "description": "Connection id from list_connections." },
                    "sql": { "type": "string", "description": "A single statement." },
                },
                "required": ["connection", "sql"],
                "additionalProperties": false,
            },
        }));
    }
    json!({ "tools": tools })
}

async fn call_tool(state: &ServerState, request: &Json) -> Json {
    let params = request.get("params");
    let Some(name) = params
        .and_then(|params| params.get("name"))
        .and_then(Json::as_str)
    else {
        return tool_error("missing tool name");
    };
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    let outcome = match name {
        "list_connections" => list_connections(state),
        "describe_table" => describe_table(state, &arguments).await,
        "run_read_query" => run_read_query(state, &arguments).await,
        "run_write_query" if state.allow_writes => run_write_query(state, &arguments).await,
        "run_write_query" => Err(
            "run_write_query is disabled; start the server with --allow-writes to enable it"
                .to_owned(),
        ),
        other => Err(format!("unknown tool: {other}")),
    };

    match outcome {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": value }],
            "isError": false,
        }),
        Err(message) => tool_error(&message),
    }
}

fn tool_error(message: &str) -> Json {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

fn list_connections(state: &ServerState) -> Result<String, String> {
    let connections: Vec<Json> = state
        .connections
        .iter()
        .map(|connection| {
            let available = connection.driver == BUNDLED_DRIVER_ID && connection.path.is_some();
            let mut entry = json!({
                "id": connection.id,
                "name": connection.name,
                "driver": connection.driver,
                "available": available,
            });
            if !available {
                entry["reason"] = Json::String(
                    "only file-backed SQLite connections are exposed by this server".to_owned(),
                );
            }
            entry
        })
        .collect();
    let mut result = json!({ "connections": connections });
    if let Some(error) = &state.settings_error {
        result["warning"] = Json::String(error.clone());
    }
    encode(&result)
}

async fn describe_table(state: &ServerState, arguments: &Json) -> Result<String, String> {
    let connection_id = required_str(arguments, "connection")?;
    let table = required_str(arguments, "table")?;
    let connection = open_connection(state, &connection_id).await?;
    let schema = connection
        .describe_table(&table_ref(&table))
        .await
        .map_err(|error| error.to_string())?;
    // SQLite describes a missing table as a schema with no columns, which
    // would otherwise look like a valid answer to the agent.
    if schema.columns.is_empty() {
        return Err(format!("table `{table}` was not found"));
    }

    let columns: Vec<Json> = schema
        .columns
        .iter()
        .map(|column| {
            json!({
                "name": column.name,
                "type": format!("{:?}", column.declared_type),
                "nullable": column.nullable,
                "primary_key": column.primary_key,
            })
        })
        .collect();
    let foreign_keys: Vec<Json> = schema
        .foreign_keys
        .iter()
        .map(|foreign_key| {
            json!({
                "columns": foreign_key.columns,
                "referenced_table": foreign_key.referenced_table,
                "referenced_columns": foreign_key.referenced_columns,
            })
        })
        .collect();
    encode(&json!({
        "table": schema.reference.qualified_name(),
        "kind": match schema.kind {
            database_core::ObjectKind::Table => "table",
            database_core::ObjectKind::View => "view",
        },
        "columns": columns,
        "primary_key": schema
            .primary_key
            .as_ref()
            .map(|key| key.columns.clone())
            .unwrap_or_default(),
        "foreign_keys": foreign_keys,
    }))
}

async fn run_read_query(state: &ServerState, arguments: &Json) -> Result<String, String> {
    let connection_id = required_str(arguments, "connection")?;
    let sql = required_str(arguments, "sql")?;
    let limit = arguments
        .get("limit")
        .and_then(Json::as_u64)
        .map_or(DEFAULT_ROW_LIMIT, |limit| {
            (limit as usize).clamp(1, MAX_ROW_LIMIT)
        });

    let statements = split_statements(&sql);
    if statements.len() != 1 {
        return Err("send exactly one statement per call".to_owned());
    }
    let statement = &statements[0].text;
    if !is_read_only(statement) {
        return Err(
            "run_read_query is read-only and accepts only SELECT, VALUES, TABLE or EXPLAIN \
             statements"
                .to_owned(),
        );
    }

    let connection = open_connection(state, &connection_id).await?;
    // Defense in depth: even a classification mistake cannot write on this
    // connection.
    let pragma = connection
        .execute("PRAGMA query_only = 1", &[])
        .await
        .map_err(|error| error.to_string())?;
    while pragma.rows.next().await.is_some() {}

    let executed = connection
        .execute(statement, &[])
        .await
        .map_err(|error| error.to_string())?;
    let columns: Vec<Json> = executed
        .metadata
        .columns
        .iter()
        .map(|column| {
            json!({
                "name": column.name,
                "type": format!("{:?}", column.declared_type),
            })
        })
        .collect();
    let (rows, truncated) = collect_rows(executed.rows, &executed.handle, limit).await?;
    encode(&json!({
        "columns": columns,
        "rows": rows,
        "truncated": truncated,
    }))
}

async fn run_write_query(state: &ServerState, arguments: &Json) -> Result<String, String> {
    let connection_id = required_str(arguments, "connection")?;
    let sql = required_str(arguments, "sql")?;
    let statements = split_statements(&sql);
    if statements.len() != 1 {
        return Err("send exactly one statement per call".to_owned());
    }
    let statement = &statements[0].text;
    if matches!(
        database_core::classify(statement),
        database_core::StatementKind::Empty | database_core::StatementKind::Transaction
    ) {
        return Err("run_write_query needs a data or schema statement".to_owned());
    }

    let connection = open_connection(state, &connection_id).await?;
    let executed = connection
        .execute(statement, &[])
        .await
        .map_err(|error| error.to_string())?;
    // Drain so step errors surface here and the statement completes.
    let rows = executed.rows;
    while let Some(item) = rows.next().await {
        item.map_err(|error| error.to_string())?;
    }
    // SQLite reports metadata before stepping, so the changed count comes from
    // the connection's own `changes()`.
    let changes = connection
        .execute("SELECT changes() AS changed", &[])
        .await
        .map_err(|error| error.to_string())?;
    let mut affected_rows = 0_i64;
    if let Some(item) = changes.rows.next().await {
        let row = item.map_err(|error| error.to_string())?;
        if let Some(Value::Integer(changed)) = row.get(0) {
            affected_rows = *changed;
        }
    }
    encode(&json!({ "affected_rows": affected_rows }))
}

async fn collect_rows(
    rows: RowStream,
    handle: &database_core::QueryHandle,
    limit: usize,
) -> Result<(Vec<Json>, bool), String> {
    let mut collected = Vec::new();
    let mut truncated = false;
    while let Some(item) = rows.next().await {
        let row = item.map_err(|error| error.to_string())?;
        if collected.len() == limit {
            truncated = true;
            // Stop the producer instead of materializing the rest.
            handle.cancel();
            break;
        }
        collected.push(Json::Array(
            row.0.iter().map(Value::to_json).collect::<Vec<_>>(),
        ));
    }
    Ok((collected, truncated))
}

async fn open_connection(
    state: &ServerState,
    connection_id: &str,
) -> Result<Arc<dyn DriverConnection>, String> {
    let saved = state
        .connections
        .iter()
        .find(|connection| connection.id == connection_id)
        .ok_or_else(|| format!("unknown connection: {connection_id}"))?;
    if saved.driver != BUNDLED_DRIVER_ID {
        return Err(format!(
            "connection `{connection_id}` uses driver `{}`, which this server does not bundle",
            saved.driver
        ));
    }
    let path = saved
        .path
        .as_ref()
        .ok_or_else(|| format!("connection `{connection_id}` has no database file"))?;
    let config = ConnectionConfig {
        id: ConnectionId(saved.id.clone()),
        display_name: saved.name.clone(),
        driver_id: saved.driver.clone(),
        source: DataSource::File {
            path: PathBuf::from(path),
        },
        host: None,
        port: None,
        user: None,
        database: None,
        options: Default::default(),
    };
    // File-backed SQLite needs no secret; nothing here touches the keychain.
    SqliteDriver
        .connect(&config, None)
        .await
        .map_err(|error| error.to_string())
}

fn required_str(arguments: &Json, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing `{key}`"))
}

fn table_ref(name: &str) -> TableRef {
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        [name] => TableRef::new(*name),
        [schema, name] => TableRef {
            catalog: None,
            schema: Some((*schema).to_owned()),
            name: (*name).to_owned(),
        },
        [catalog, schema, name] => TableRef {
            catalog: Some((*catalog).to_owned()),
            schema: Some((*schema).to_owned()),
            name: (*name).to_owned(),
        },
        _ => TableRef::new(name),
    }
}

fn encode(value: &Json) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Args, parse_args, read_settings, table_ref};

    fn args(arguments: &[&str]) -> Result<Args, String> {
        parse_args(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn flags_are_parsed_and_unknown_ones_rejected() {
        let parsed = args(&["--allow-writes", "--settings", "/tmp/s.json"]).expect("parse");
        assert!(parsed.allow_writes);
        assert_eq!(
            parsed.settings.expect("settings path"),
            std::path::PathBuf::from("/tmp/s.json")
        );
        assert!(args(&["--settings"]).is_err());
        assert!(args(&["--nope"]).is_err());
    }

    #[test]
    fn settings_allow_comments_and_ignore_unknown_fields() {
        let path = std::env::temp_dir().join(format!(
            "zzz-mcp-settings-{}-{}.json",
            std::process::id(),
            line!()
        ));
        std::fs::write(
            &path,
            r#"{
                // Saved by the database panel.
                "database": {
                    "connections": [{
                        "id": "work",
                        "name": "Work",
                        "driver": "sqlite",
                        "path": "/tmp/work.sqlite",
                        "password": "not-used",
                    }],
                },
            }"#,
        )
        .expect("write settings");
        let connections = read_settings(&path).expect("parse settings");
        std::fs::remove_file(&path).expect("remove settings");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].id, "work");
        assert_eq!(connections[0].path.as_deref(), Some("/tmp/work.sqlite"));
    }

    #[test]
    fn table_names_can_be_qualified() {
        assert_eq!(table_ref("users").name, "users");
        let qualified = table_ref("main.users");
        assert_eq!(qualified.schema.as_deref(), Some("main"));
        assert_eq!(qualified.name, "users");
        let full = table_ref("db.main.users");
        assert_eq!(full.catalog.as_deref(), Some("db"));
        assert_eq!(full.schema.as_deref(), Some("main"));
        assert_eq!(full.name, "users");
    }
}
