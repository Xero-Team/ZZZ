//! Minimal database driver process used to exercise the extension protocol.
//!
//! It implements the newline-delimited JSON protocol over stdin/stdout with an
//! in-memory data model. It exists so the host can be tested against a real
//! subprocess instead of an in-process mock of the transport.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use database_core::{
    Capabilities, Catalog, Column, ColumnType, ExplainNode, ExplainPlan, Index, ObjectKind,
    PrimaryKey, Row, Schema, Table, TableRef, TableSchema, Value,
};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value as Json, json};

#[derive(Deserialize)]
struct Frame {
    id: u64,
    method: String,
    #[serde(default)]
    params: Json,
}

#[derive(Deserialize)]
struct ExecuteParams {
    sql: String,
    #[serde(default)]
    params: Vec<Value>,
}

type Output = Arc<Mutex<std::io::Stdout>>;

/// Write one newline-delimited frame. A write failure means the host is gone;
/// report it on stderr instead of discarding it silently.
fn write_frame(output: &Output, frame: &Json) {
    let mut output = output.lock();
    let outcome = serde_json::to_writer(&mut *output, frame)
        .map_err(std::io::Error::other)
        .and_then(|()| {
            output.write_all(b"\n")?;
            output.flush()
        });
    if let Err(error) = outcome {
        eprintln!("mock database driver: failed to write response: {error}");
    }
}

fn respond(output: &Output, id: u64, result: Json) {
    write_frame(output, &json!({ "id": id, "result": result }));
}

fn respond_error(output: &Output, id: u64, code: &str, message: &str) {
    write_frame(
        output,
        &json!({ "id": id, "error": { "code": code, "message": message } }),
    );
}

fn widgets_schema() -> TableRef {
    TableRef::new("widgets")
}

fn main() {
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--pid-file"
            && let Some(path) = args.next()
            && let Err(error) = std::fs::write(&path, std::process::id().to_string())
        {
            eprintln!("mock database driver: failed to write {path}: {error}");
        }
    }
    let output: Output = Arc::new(Mutex::new(std::io::stdout()));
    let running: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let frame: Frame = match serde_json::from_str(&line) {
            Ok(frame) => frame,
            Err(_) => continue,
        };

        match frame.method.as_str() {
            "connect" | "test" | "commit" | "rollback" => respond(&output, frame.id, json!({})),
            "capabilities" => respond(
                &output,
                frame.id,
                serde_json::to_value(Capabilities::sqlite()).unwrap_or(Json::Null),
            ),
            "catalogs" => respond(
                &output,
                frame.id,
                json!([Catalog {
                    name: "mock".to_owned(),
                    schemas: vec![Schema {
                        name: "main".to_owned(),
                        tables: vec![Table {
                            reference: widgets_schema(),
                            kind: ObjectKind::Table,
                        }],
                    }],
                }]),
            ),
            "describe_table" => {
                let reference = frame
                    .params
                    .get("table")
                    .cloned()
                    .and_then(|table| serde_json::from_value::<TableRef>(table).ok())
                    .unwrap_or_else(widgets_schema);
                let mut id = Column::new("id", ColumnType::Integer);
                id.primary_key = true;
                id.nullable = false;
                respond(
                    &output,
                    frame.id,
                    json!(TableSchema {
                        reference,
                        kind: ObjectKind::Table,
                        columns: vec![id, Column::new("name", ColumnType::Text)],
                        primary_key: Some(PrimaryKey {
                            name: None,
                            columns: vec!["id".to_owned()],
                        }),
                        indexes: Vec::<Index>::new(),
                        foreign_keys: Vec::new(),
                        definition: None,
                    }),
                );
            }
            "execute" => {
                let Ok(params) = serde_json::from_value::<ExecuteParams>(frame.params) else {
                    respond_error(&output, frame.id, "driver", "invalid execute params");
                    continue;
                };
                let id = frame.id;
                let output = output.clone();
                let flag = Arc::new(AtomicBool::new(false));
                if params.sql.to_ascii_lowercase().contains("slow") {
                    running.lock().insert(id, flag.clone());
                }
                let running = running.clone();
                std::thread::spawn(move || {
                    if params.sql.to_ascii_lowercase().contains("hang") {
                        // Deliberately never respond; the host must time out.
                        return;
                    }
                    if params.sql.to_ascii_lowercase().contains("syntax") {
                        respond_error(&output, id, "syntax", "mock syntax error");
                        running.lock().remove(&id);
                        return;
                    }
                    if params.sql.to_ascii_lowercase().contains("slow") {
                        for _ in 0..400 {
                            if flag.load(Ordering::SeqCst) {
                                respond_error(&output, id, "cancelled", "query cancelled");
                                running.lock().remove(&id);
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    let lower = params.sql.to_ascii_lowercase();
                    let is_dml = lower.starts_with("update")
                        || lower.starts_with("insert")
                        || lower.starts_with("delete");
                    let rows = if is_dml || lower.contains("slow") {
                        Vec::new()
                    } else if let Some(first) = params.params.first() {
                        vec![Row(vec![first.clone()])]
                    } else {
                        vec![Row(vec![Value::Text(params.sql.clone())])]
                    };
                    respond(
                        &output,
                        id,
                        json!({
                            "columns": if rows.is_empty() {
                                Vec::<Column>::new()
                            } else {
                                vec![Column::new("value", ColumnType::Other {
                                    declared: "MOCK".to_owned(),
                                })]
                            },
                            "rows": rows,
                            "affected_rows": is_dml.then_some(1usize),
                        }),
                    );
                    running.lock().remove(&id);
                });
            }
            "begin" => respond(&output, frame.id, json!({ "transaction": 1 })),
            "explain" => respond(
                &output,
                frame.id,
                json!(ExplainPlan {
                    nodes: vec![ExplainNode {
                        id: 1,
                        parent: 0,
                        detail: "mock scan".to_owned(),
                    }],
                }),
            ),
            "cancel" => {
                let request = frame.params.get("request").and_then(Json::as_u64);
                let running = running.lock();
                match request {
                    Some(0) | None => {
                        for flag in running.values() {
                            flag.store(true, Ordering::SeqCst);
                        }
                    }
                    Some(request) => {
                        if let Some(flag) = running.get(&request).cloned() {
                            flag.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
            "close" => {
                respond(&output, frame.id, json!({}));
                return;
            }
            other => respond_error(
                &output,
                frame.id,
                "driver",
                &format!("unknown method: {other}"),
            ),
        }
    }
}
