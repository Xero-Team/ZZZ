//! Subprocess driver protocol (D-04).
//!
//! An extension declares executables in `[[database_drivers]]`. The host starts
//! one process per connection and exchanges newline-delimited JSON over its
//! stdin/stdout. The protocol is deliberately independent of WIT so it can
//! evolve and can be implemented in any language.
//!
//! Lifecycle: `QuerySession` (through `ConnectionManager`) owns the connection;
//! `close` asks the process to exit, waits briefly, then kills it. Dropping the
//! connection also kills the child, so a failed handshake cannot leave an
//! orphan process behind.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_channel::{Sender, bounded};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use smol::stream::StreamExt;

use crate::driver::{
    DatabaseDriver, DriverConnection, DriverResult, ExecutedQuery, ExplainPlan, Interrupt,
    QueryHandle, QueryMetadata, RowStream, TransactionId,
};
use crate::error::{DatabaseError, DatabaseErrorKind};
use crate::model::{Capabilities, Catalog, Column, ConnectionConfig, Row, TableRef, TableSchema};

/// How long a control request may take before the host gives up. `execute` gets
/// more room because a legitimate query can run for a while; cancellation is
/// the intended way to stop it earlier.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
const EXECUTE_TIMEOUT: Duration = Duration::from_secs(300);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Everything the host needs to start and advertise one extension driver.
#[derive(Clone, Debug)]
pub struct ExtensionDriverConfig {
    /// Namespaced, stable driver id (`<extension-id>.<manifest-id>`).
    pub id: String,
    pub display_name: String,
    /// Absolute path to the driver executable inside the extension directory.
    pub command: PathBuf,
    pub args: Vec<String>,
    pub capabilities: Capabilities,
    /// Timeout for control requests (`connect`, `catalogs`, `begin`, ...).
    pub control_timeout: Duration,
    /// Timeout for `execute`; cancellation is preferred over waiting this out.
    pub execute_timeout: Duration,
}

impl ExtensionDriverConfig {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        command: PathBuf,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            command,
            args: Vec::new(),
            capabilities,
            control_timeout: CONTROL_TIMEOUT,
            execute_timeout: EXECUTE_TIMEOUT,
        }
    }
}

/// A `DatabaseDriver` backed by an extension subprocess.
pub struct ExtensionDriver {
    config: ExtensionDriverConfig,
}

impl ExtensionDriver {
    pub fn new(config: ExtensionDriverConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ExtensionDriverConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for ExtensionDriver {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn display_name(&self) -> &str {
        &self.config.display_name
    }

    fn capabilities(&self) -> Capabilities {
        self.config.capabilities.clone()
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
        secret: Option<&[u8]>,
    ) -> DriverResult<Arc<dyn DriverConnection>> {
        let credentials = match secret {
            Some(secret) => Some(
                std::str::from_utf8(secret)
                    .map_err(|_| {
                        DatabaseError::authentication(
                            "extension driver credentials must be valid UTF-8",
                        )
                    })?
                    .to_owned(),
            ),
            None => None,
        };
        let connection = ExtensionConnection::spawn(&self.config)?;
        connection
            .request(
                "connect",
                Some(json!({ "config": config, "credentials": credentials })),
                self.config.control_timeout,
            )
            .await?;
        Ok(Arc::new(connection))
    }
}

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Json>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    result: Option<Json>,
    #[serde(default)]
    error: Option<WireError>,
}

#[derive(Deserialize)]
struct WireError {
    code: String,
    message: String,
}

/// Translate a protocol error code. Codes are a stable enum, never a message
/// prefix: drivers must not be able to change classification by rephrasing text.
fn wire_error(error: WireError) -> DatabaseError {
    let kind = match error.code.as_str() {
        "connection" => DatabaseErrorKind::Connection,
        "authentication" => DatabaseErrorKind::Authentication,
        "syntax" => DatabaseErrorKind::Syntax,
        "not_found" => DatabaseErrorKind::NotFound,
        "permission" => DatabaseErrorKind::Permission,
        "cancelled" => DatabaseErrorKind::Cancelled,
        "constraint" => DatabaseErrorKind::Constraint,
        "timeout" => DatabaseErrorKind::Timeout,
        "driver" => DatabaseErrorKind::Driver,
        _ => DatabaseErrorKind::Other,
    };
    DatabaseError::new(kind, error.message)
}

#[derive(Deserialize)]
struct ExecuteResponse {
    #[serde(default)]
    columns: Vec<Column>,
    #[serde(default)]
    rows: Vec<Row>,
    #[serde(default)]
    affected_rows: Option<usize>,
}

#[derive(Deserialize)]
struct BeginResponse {
    transaction: u64,
}

type Pending = Arc<Mutex<HashMap<u64, Sender<DriverResult<Response>>>>>;

/// The child process itself. Kept separate from the response plumbing so the
/// reader thread never holds a reference that would prevent `Drop` from
/// terminating the child.
struct ExtensionProcess {
    child: Mutex<smol::process::Child>,
    stdin: Mutex<Option<smol::process::ChildStdin>>,
}

impl ExtensionProcess {
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut stdin = self.stdin.lock();
        let Some(stdin) = stdin.as_mut() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "extension driver stdin is closed",
            ));
        };
        smol::block_on(async {
            stdin.write_all(line.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await
        })
    }

    /// Close stdin, give the child a moment to exit on its own, then kill it.
    fn terminate(&self) {
        self.stdin.lock().take();
        let mut child = self.child.lock();
        for _ in 0..20 {
            match child.try_status() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
        child.kill().ok();
        for _ in 0..20 {
            match child.try_status() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

impl Drop for ExtensionProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

struct ExtensionConnection {
    process: Arc<ExtensionProcess>,
    pending: Pending,
    next_request: Arc<AtomicU64>,
    closed: AtomicBool,
    control_timeout: Duration,
    execute_timeout: Duration,
}

impl ExtensionConnection {
    fn spawn(config: &ExtensionDriverConfig) -> DriverResult<Self> {
        let mut command = smol::process::Command::new(&config.command);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            DatabaseError::connection(format!(
                "failed to start extension driver {}: {error}",
                config.command.display()
            ))
        })?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DatabaseError::driver("extension driver has no stdout pipe"))?;
        let stderr = child.stderr.take();
        let Some(stdin) = stdin else {
            let mut child = child;
            child.kill().ok();
            return Err(DatabaseError::driver("extension driver has no stdin pipe"));
        };

        let process = Arc::new(ExtensionProcess {
            child: Mutex::new(child),
            stdin: Mutex::new(Some(stdin)),
        });
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

        let reader_pending = pending.clone();
        let reader = std::thread::Builder::new()
            .name(format!("extension-db-reader-{}", config.id))
            .spawn(move || read_responses(stdout, reader_pending));
        if let Err(error) = reader {
            return Err(DatabaseError::driver(format!(
                "failed to start extension driver reader: {error}"
            )));
        }
        if let Some(stderr) = stderr {
            let driver_id = config.id.clone();
            std::thread::Builder::new()
                .name(format!("extension-db-stderr-{driver_id}"))
                .spawn(move || {
                    smol::block_on(async {
                        let mut lines = BufReader::new(stderr).lines();
                        while let Some(Ok(line)) = lines.next().await {
                            log::warn!("extension database driver {driver_id}: {line}");
                        }
                    });
                })
                .map_err(|error| {
                    DatabaseError::driver(format!(
                        "failed to start extension driver stderr reader: {error}"
                    ))
                })?;
        }

        Ok(Self {
            process,
            pending,
            next_request: Arc::new(AtomicU64::new(1)),
            closed: AtomicBool::new(false),
            control_timeout: config.control_timeout,
            execute_timeout: config.execute_timeout,
        })
    }

    async fn request(
        &self,
        method: &str,
        params: Option<Json>,
        timeout: Duration,
    ) -> DriverResult<Json> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(DatabaseError::connection(
                "extension driver connection is closed",
            ));
        }
        let id = self.next_request.fetch_add(1, Ordering::SeqCst);
        let line = serde_json::to_string(&Request { id, method, params })
            .map_err(|error| DatabaseError::driver(error.to_string()))?;
        let (sender, receiver) = bounded(1);
        self.pending.lock().insert(id, sender);
        if let Err(error) = self.process.write_line(&line) {
            self.pending.lock().remove(&id);
            return Err(DatabaseError::connection(format!(
                "failed to write to extension driver: {error}"
            )));
        }

        let wait = async {
            match receiver.recv().await {
                Ok(outcome) => outcome,
                Err(_) => Err(DatabaseError::connection(
                    "extension driver response channel closed",
                )),
            }
        };
        // database_core has no GPUI executor; this timer only enforces the
        // subprocess protocol deadline, so the workspace-wide GPUI timer
        // replacement does not apply here.
        #[allow(clippy::disallowed_methods)]
        let timer = async_io::Timer::after(timeout);
        futures::pin_mut!(wait, timer);
        let outcome = match futures::future::select(wait, timer).await {
            futures::future::Either::Left((outcome, _)) => outcome,
            futures::future::Either::Right(_) => {
                self.pending.lock().remove(&id);
                self.notify_cancel(id);
                return Err(DatabaseError::timeout(format!(
                    "extension driver did not respond to {method}"
                )));
            }
        };
        self.pending.lock().remove(&id);
        let response = outcome?;
        if let Some(error) = response.error {
            return Err(wire_error(error));
        }
        Ok(response.result.unwrap_or(Json::Null))
    }

    /// Fire-and-forget `cancel`. The child answers with an error for the
    /// in-flight request id, which the caller observes as a cancelled error.
    fn notify_cancel(&self, request_id: u64) {
        let line = match serde_json::to_string(&Request {
            id: self.next_request.fetch_add(1, Ordering::SeqCst),
            method: "cancel",
            params: Some(json!({ "request": request_id })),
        }) {
            Ok(line) => line,
            Err(error) => {
                log::warn!("failed to encode extension driver cancel: {error}");
                return;
            }
        };
        if let Err(error) = self.process.write_line(&line) {
            log::warn!("failed to send extension driver cancel: {error}");
        }
    }
}

fn read_responses(stdout: smol::process::ChildStdout, pending: Pending) {
    smol::block_on(async {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(Ok(line)) = lines.next().await {
            let response: Response = match serde_json::from_str(&line) {
                Ok(response) => response,
                Err(error) => {
                    log::warn!("invalid frame from extension database driver: {error}");
                    continue;
                }
            };
            if let Some(sender) = pending.lock().remove(&response.id)
                && let Err(error) = sender.try_send(Ok(response))
            {
                log::warn!("failed to deliver extension driver response: {error}");
            }
        }
    });
    // The process exited. Fail every request instead of making each one wait
    // for its own timeout.
    let error = DatabaseError::connection("extension driver process exited");
    for (_, sender) in pending.lock().drain() {
        if let Err(send_error) = sender.try_send(Err(error.clone())) {
            log::warn!("failed to report extension driver exit: {send_error}");
        }
    }
}

struct ExtensionInterrupt {
    process: Arc<ExtensionProcess>,
    next_request: Arc<AtomicU64>,
}

impl Interrupt for ExtensionInterrupt {
    fn interrupt(&self) {
        let line = match serde_json::to_string(&Request {
            id: self.next_request.fetch_add(1, Ordering::SeqCst),
            method: "cancel",
            params: None,
        }) {
            Ok(line) => line,
            Err(error) => {
                log::warn!("failed to encode extension driver interrupt: {error}");
                return;
            }
        };
        if let Err(error) = self.process.write_line(&line) {
            log::warn!("failed to send extension driver interrupt: {error}");
        }
    }
}

#[async_trait::async_trait]
impl DriverConnection for ExtensionConnection {
    async fn test(&self) -> DriverResult<()> {
        self.request("test", None, self.control_timeout)
            .await
            .map(|_| ())
    }

    async fn catalogs(&self) -> DriverResult<Vec<Catalog>> {
        let result = self.request("catalogs", None, self.control_timeout).await?;
        serde_json::from_value(result)
            .map_err(|error| DatabaseError::driver(format!("invalid catalogs response: {error}")))
    }

    async fn describe_table(&self, table: &TableRef) -> DriverResult<TableSchema> {
        let result = self
            .request(
                "describe_table",
                Some(json!({ "table": table })),
                self.control_timeout,
            )
            .await?;
        serde_json::from_value(result).map_err(|error| {
            DatabaseError::driver(format!("invalid describe_table response: {error}"))
        })
    }

    async fn execute(
        &self,
        sql: &str,
        params: &[crate::model::Value],
    ) -> DriverResult<ExecutedQuery> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = QueryHandle::new(
            cancelled.clone(),
            Some(Arc::new(ExtensionInterrupt {
                process: self.process.clone(),
                next_request: self.next_request.clone(),
            })),
        );
        let response = self
            .request(
                "execute",
                Some(json!({ "sql": sql, "params": params })),
                self.execute_timeout,
            )
            .await;
        let response: ExecuteResponse = match response {
            Ok(result) => serde_json::from_value(result).map_err(|error| {
                DatabaseError::driver(format!("invalid execute response: {error}"))
            })?,
            Err(error) if error.is_cancelled() => {
                cancelled.store(true, Ordering::SeqCst);
                return Err(error);
            }
            Err(error) => return Err(error),
        };

        let (sender, stream) = RowStream::new(crate::driver::ROW_STREAM_CAPACITY);
        let rows = response.rows;
        let cancel_flag = cancelled.clone();
        smol::spawn(async move {
            for row in rows {
                if cancel_flag.load(Ordering::SeqCst) {
                    break;
                }
                if sender.send(Ok(row)).await.is_err() {
                    // The consumer closed the stream and there is nobody left to report to;
                    // `finish` below still runs so the stream ends deliberately.
                    break;
                }
            }
            sender.finish();
        })
        .detach();
        Ok(ExecutedQuery {
            handle,
            metadata: QueryMetadata {
                columns: response.columns,
                affected_rows: response.affected_rows,
            },
            rows: stream,
        })
    }

    async fn begin(&self) -> DriverResult<TransactionId> {
        let result = self.request("begin", None, self.control_timeout).await?;
        let response: BeginResponse = serde_json::from_value(result)
            .map_err(|error| DatabaseError::driver(format!("invalid begin response: {error}")))?;
        Ok(TransactionId(response.transaction))
    }

    async fn commit(&self, transaction: TransactionId) -> DriverResult<()> {
        self.request(
            "commit",
            Some(json!({ "transaction": transaction.0 })),
            self.control_timeout,
        )
        .await
        .map(|_| ())
    }

    async fn rollback(&self, transaction: TransactionId) -> DriverResult<()> {
        self.request(
            "rollback",
            Some(json!({ "transaction": transaction.0 })),
            self.control_timeout,
        )
        .await
        .map(|_| ())
    }

    async fn explain(&self, sql: &str) -> DriverResult<ExplainPlan> {
        let result = self
            .request("explain", Some(json!({ "sql": sql })), self.control_timeout)
            .await?;
        serde_json::from_value(result)
            .map_err(|error| DatabaseError::driver(format!("invalid explain response: {error}")))
    }

    fn interrupt(&self) {
        self.notify_cancel(0);
    }

    async fn close(&self) -> DriverResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let outcome = self.request("close", None, CLOSE_TIMEOUT).await;
        self.closed.store(true, Ordering::SeqCst);
        let process = self.process.clone();
        // Termination blocks while it polls the child, so it leaves the async
        // executor. It cannot fail meaningfully: the fallback is `kill`.
        smol::unblock(move || process.terminate()).await;
        outcome.map(|_| ())
    }
}
