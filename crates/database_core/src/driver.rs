//! Driver protocol: the single trait every built-in and extension driver implements.
//!
//! Cancellation is a first-class part of the contract (D-08): every `execute`
//! returns a `QueryHandle` whose `cancel()` is synchronous and never an error.
//! Rows are delivered through a bounded `RowStream` so a large query is never
//! materialized in one allocation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;

use async_channel::{Receiver, Sender, bounded};

use crate::error::DatabaseError;
use crate::model::{
    Capabilities, Catalog, Column, ConnectionConfig, ResultSet, Row, TableRef, TableSchema, Value,
};

/// Result type used throughout the driver protocol. `DatabaseError` is used
/// directly (it implements `std::error::Error`, so `anyhow::Error` coercion is
/// automatic via the blanket impl).
pub type DriverResult<T> = Result<T, DatabaseError>;

/// The maximum number of rows buffered in a stream before backpressure applies.
pub const ROW_STREAM_CAPACITY: usize = 256;

/// A handle for a running query. `cancel()` is synchronous so a GPUI action can
/// trigger it immediately; the driver observes the flag and stops the query.
/// Cancellation is never reported as an error (D-08).
#[derive(Clone)]
pub struct QueryHandle {
    id: u64,
    cancelled: Arc<AtomicBool>,
    interrupt: Option<Arc<dyn Interrupt>>,
}

impl std::fmt::Debug for QueryHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QueryHandle")
            .field("id", &self.id)
            .field("cancelled", &self.is_cancelled())
            .field("has_interrupt", &self.interrupt.is_some())
            .finish()
    }
}

/// A driver-provided hook that aborts an in-flight native query. For SQLite this
/// calls `sqlite3_interrupt`; for a subprocess driver it writes a `cancel` frame.
pub trait Interrupt: Send + Sync + 'static {
    fn interrupt(&self);
}

static NEXT_QUERY_ID: AtomicU64 = AtomicU64::new(1);

impl QueryHandle {
    pub fn new(cancelled: Arc<AtomicBool>, interrupt: Option<Arc<dyn Interrupt>>) -> Self {
        Self {
            id: NEXT_QUERY_ID.fetch_add(1, Ordering::SeqCst),
            cancelled,
            interrupt,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Request cancellation. Idempotent: repeated calls are harmless.
    pub fn cancel(&self) {
        if self.cancelled.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(interrupt) = &self.interrupt {
            interrupt.interrupt();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// A cloneable cancel slot for one query run.
///
/// A `QuerySession` publishes the driver's [`QueryHandle`] here as soon as it
/// exists, so an owner that does not hold the session while it awaits (a
/// notebook cell, whose session is inside a task) can cancel exactly that one
/// query instead of every query on the connection (D-08).
#[derive(Clone, Default)]
pub struct QueryCanceller {
    cancelled: Arc<AtomicBool>,
    handle: Arc<Mutex<Option<QueryHandle>>>,
}

impl QueryCanceller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish the handle of the query that is starting. A cancel that raced
    /// the driver (arriving before the handle existed) is honoured here.
    pub fn attach(&self, handle: QueryHandle) {
        *self.handle.lock() = Some(handle);
        if self.cancelled.load(Ordering::SeqCst)
            && let Some(handle) = self.handle.lock().take()
        {
            handle.cancel();
        }
    }

    /// Cancel the attached query, or the next one if the driver has not
    /// returned a handle yet. Idempotent.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.lock().take() {
            handle.cancel();
        }
    }

    /// Drop the finished handle without cancelling it. The cancelled flag stays
    /// until [`QueryCanceller::reset`] so a late `attach` still sees it.
    pub fn clear(&self) {
        self.handle.lock().take();
    }

    /// Prepare the slot for a new run.
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
        self.handle.lock().take();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// A bounded stream of typed rows. The producer stops when the receiver is
/// dropped, which cancels the underlying query without an orphan task.
///
/// A step error (constraint violation, out-of-memory, ...) is delivered as the
/// stream's final item and then the stream ends; it must not be silently
/// dropped, because statements that return no rows have no other error channel.
#[derive(Debug)]
pub struct RowStream {
    pub receiver: Receiver<DriverResult<Row>>,
}

impl RowStream {
    pub fn new(capacity: usize) -> (Sender<DriverResult<Row>>, Self) {
        let (sender, receiver) = bounded(capacity);
        (sender, Self { receiver })
    }

    /// Pull the next row. A closed channel means the producer finished (or was
    /// cancelled); that is a normal terminal condition, not an error.
    pub async fn next(&self) -> Option<DriverResult<Row>> {
        self.receiver.recv().await.ok()
    }
}

/// Metadata collected for a query before its rows are streamed.
#[derive(Debug, Clone)]
pub struct QueryMetadata {
    pub columns: Vec<Column>,
    /// Number of rows changed for DML statements.
    pub affected_rows: Option<usize>,
}

/// The outcome of `DriverConnection::execute`: a handle for cancellation, the
/// column metadata, and a bounded stream of rows.
pub struct ExecutedQuery {
    pub handle: QueryHandle,
    pub metadata: QueryMetadata,
    pub rows: RowStream,
}

/// A parsed EXPLAIN plan node used by Visual EXPLAIN (Phase 8).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExplainPlan {
    /// The raw detail text from `EXPLAIN QUERY PLAN`, already ordered by id.
    pub nodes: Vec<ExplainNode>,
}

/// One node in an EXPLAIN tree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExplainNode {
    pub id: i64,
    pub parent: i64,
    pub detail: String,
}

/// Every driver implements this. The UI derives available actions only from
/// `capabilities()`, never from `id()` (invariant 1).
#[async_trait::async_trait]
pub trait DatabaseDriver: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;

    async fn connect(
        &self,
        config: &ConnectionConfig,
        secret: Option<&[u8]>,
    ) -> DriverResult<Arc<dyn DriverConnection>>;
}

/// A live connection. All methods are async so subprocess drivers and remote
/// transports fit the same contract as the built-in SQLite driver.
#[async_trait::async_trait]
pub trait DriverConnection: Send + Sync + 'static {
    /// Verify the connection without mutating data.
    async fn test(&self) -> DriverResult<()>;

    /// Enumerate catalogs, schemas, tables, and views by querying the database,
    /// never from a hard-coded list.
    async fn catalogs(&self) -> DriverResult<Vec<Catalog>>;

    /// Describe one table: columns, primary key, indexes, foreign keys.
    async fn describe_table(&self, table: &TableRef) -> DriverResult<TableSchema>;

    /// Execute a statement with bound parameters. Returns a cancellable handle
    /// and a bounded row stream.
    async fn execute(&self, sql: &str, params: &[Value]) -> DriverResult<ExecutedQuery>;

    async fn begin(&self) -> DriverResult<TransactionId>;
    async fn commit(&self, transaction: TransactionId) -> DriverResult<()>;
    async fn rollback(&self, transaction: TransactionId) -> DriverResult<()>;

    /// Parse an EXPLAIN QUERY PLAN result into a plan tree.
    async fn explain(&self, sql: &str) -> DriverResult<ExplainPlan>;

    /// Interrupt whatever this connection is currently doing. Drivers whose
    /// queries cannot be cancelled through an already-returned handle implement
    /// this so a request still waiting for its first response can be stopped.
    fn interrupt(&self) {}

    async fn close(&self) -> DriverResult<()>;
}

/// An opaque, connection-scoped transaction identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(pub u64);

/// Build a `ResultSet` shell from query metadata. Filling its window cache is
/// the UI/session layer's job; this keeps ownership of rows in one place.
pub fn result_set_for(metadata: &QueryMetadata, sql: &str) -> ResultSet {
    ResultSet::new(metadata.columns.clone(), sql.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_is_idempotent() {
        let flag = Arc::new(AtomicBool::new(false));
        let handle = QueryHandle::new(flag.clone(), None);
        assert!(!handle.is_cancelled());
        handle.cancel();
        handle.cancel();
        assert!(handle.is_cancelled());
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_invokes_interrupt_once() {
        struct CountingInterrupt(Arc<AtomicBool>, std::sync::atomic::AtomicUsize);
        impl Interrupt for CountingInterrupt {
            fn interrupt(&self) {
                self.0.store(true, Ordering::SeqCst);
                self.1.fetch_add(1, Ordering::SeqCst);
            }
        }
        let fired = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(CountingInterrupt(
            fired.clone(),
            std::sync::atomic::AtomicUsize::new(0),
        ));
        let handle = QueryHandle::new(Arc::new(AtomicBool::new(false)), Some(interrupt));
        handle.cancel();
        handle.cancel();
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn row_stream_delivers_rows_and_closes() {
        futures::executor::block_on(async {
            let (sender, stream) = RowStream::new(4);
            assert!(sender.send(Ok(Row(vec![Value::Integer(1)]))).await.is_ok());
            assert!(sender.send(Ok(Row(vec![Value::Integer(2)]))).await.is_ok());
            drop(sender);
            assert_eq!(stream.next().await, Some(Ok(Row(vec![Value::Integer(1)]))));
            assert_eq!(stream.next().await, Some(Ok(Row(vec![Value::Integer(2)]))));
            assert_eq!(stream.next().await, None);
        });
    }

    #[test]
    fn row_stream_delivers_a_terminal_error() {
        futures::executor::block_on(async {
            let (sender, stream) = RowStream::new(4);
            let error = DatabaseError::constraint("duplicate key");
            sender.send(Err(error.clone())).await.expect("send error");
            drop(sender);
            let item = stream.next().await.expect("error item");
            assert_eq!(item.expect_err("error").kind, error.kind);
            assert_eq!(stream.next().await, None);
        });
    }
}
