//! Connection lifecycle management and the credential boundary.
//!
//! The manager is the sole owner of connection state (invariant 6). Runtime
//! status is never persisted (invariant 4). Secrets are resolved by the UI layer
//! from `credentials_provider` and injected as a transient argument at connect
//! time; they never enter `ConnectionConfig`, logs, or this crate's state
//! (invariant 3, D-09).

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::driver::{DatabaseDriver, DriverConnection, DriverResult};
use crate::error::DatabaseError;
#[cfg(test)]
use crate::error::DatabaseErrorKind;
use crate::model::{Capabilities, ConnectionConfig, ConnectionId, ConnectionStatus};

/// A registered driver plus its capability declaration.
#[derive(Clone)]
struct RegisteredDriver {
    driver: Arc<dyn DatabaseDriver>,
    capabilities: Capabilities,
}

/// Runtime state for one saved connection. Owned by the manager only.
struct ManagedConnection {
    config: ConnectionConfig,
    status: ConnectionStatus,
    connection: Option<Arc<dyn DriverConnection>>,
    /// Cancellation handles for in-flight queries. Closing must cancel all of
    /// them before the native connection is released (invariant 9).
    active_queries: Vec<crate::driver::QueryHandle>,
}

impl ManagedConnection {
    fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            connection: None,
            active_queries: Vec::new(),
        }
    }

    /// Drop the live connection after cancelling everything running on it. The
    /// driver's native handle is released by `Drop`; `disconnect` is preferred
    /// when the caller can await `close`.
    fn reset(&mut self, config: ConnectionConfig) {
        for query in std::mem::take(&mut self.active_queries) {
            query.cancel();
        }
        self.connection = None;
        self.status = ConnectionStatus::Disconnected;
        self.config = config;
    }
}

/// Owns driver registration and connection state. Secrets are injected per
/// connect call by the UI layer.
pub struct ConnectionManager {
    drivers: Mutex<BTreeMap<String, RegisteredDriver>>,
    connections: Mutex<BTreeMap<ConnectionId, ManagedConnection>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            drivers: Mutex::new(BTreeMap::new()),
            connections: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register a driver. The built-in SQLite driver and extension drivers both
    /// go through this single registry; no code branches on a driver id.
    pub fn register_driver(&self, driver: Arc<dyn DatabaseDriver>) {
        let capabilities = driver.capabilities();
        self.drivers.lock().insert(
            driver.id().to_owned(),
            RegisteredDriver {
                driver,
                capabilities,
            },
        );
    }

    pub fn capabilities_for(&self, driver_id: &str) -> Option<Capabilities> {
        self.drivers
            .lock()
            .get(driver_id)
            .map(|registered| registered.capabilities.clone())
    }

    /// Save (or replace) a non-sensitive connection configuration. Credentials
    /// are stored separately under `ConnectionConfig::credential_key()`.
    pub fn upsert_config(&self, config: ConnectionConfig) {
        self.connections
            .lock()
            .insert(config.id.clone(), ManagedConnection::new(config));
    }

    /// Apply the durable configuration set (from settings) to the runtime
    /// registry: remove connections that are gone, add new ones, and reset any
    /// connection whose configuration changed so the next use reconnects to the
    /// new target. Unchanged live connections keep their status and queries.
    pub fn sync_configs(&self, configs: Vec<ConnectionConfig>) {
        let mut connections = self.connections.lock();
        let configured: std::collections::BTreeSet<ConnectionId> =
            configs.iter().map(|config| config.id.clone()).collect();
        connections.retain(|id, _| configured.contains(id));
        for config in configs {
            match connections.entry(config.id.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(ManagedConnection::new(config));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let managed = entry.get_mut();
                    if managed.config != config {
                        managed.reset(config);
                    }
                }
            }
        }
    }

    pub fn remove_config(&self, id: &ConnectionId) {
        self.connections.lock().remove(id);
    }

    pub fn configs(&self) -> Vec<ConnectionConfig> {
        self.connections
            .lock()
            .values()
            .map(|managed| managed.config.clone())
            .collect()
    }

    pub fn status(&self, id: &ConnectionId) -> Option<ConnectionStatus> {
        self.connections
            .lock()
            .get(id)
            .map(|managed| managed.status)
    }

    /// Establish a connection. `secret` (if any) was resolved by the UI layer
    /// from `credentials_provider`; it is passed straight to the driver and
    /// never stored in `ConnectionConfig` or this manager.
    pub async fn connect(
        &self,
        id: &ConnectionId,
        secret: Option<&[u8]>,
    ) -> DriverResult<Arc<dyn DriverConnection>> {
        let (config, driver) = {
            let connections = self.connections.lock();
            let managed = connections
                .get(id)
                .ok_or_else(|| DatabaseError::not_found("connection config not found"))?;
            if managed.status == ConnectionStatus::Connected
                && let Some(connection) = &managed.connection
            {
                return Ok(connection.clone());
            }
            let driver = self
                .drivers
                .lock()
                .get(&managed.config.driver_id)
                .cloned()
                .ok_or_else(|| {
                    DatabaseError::driver(format!(
                        "no driver registered for {}",
                        managed.config.driver_id
                    ))
                })?;
            (managed.config.clone(), driver)
        };

        self.set_status(id, ConnectionStatus::Connecting);
        match driver.driver.connect(&config, secret).await {
            Ok(connection) => {
                let mut connections = self.connections.lock();
                if let Some(managed) = connections.get_mut(id) {
                    managed.connection = Some(connection.clone());
                    managed.status = ConnectionStatus::Connected;
                }
                Ok(connection)
            }
            Err(error) => {
                self.set_status(id, ConnectionStatus::Disconnected);
                Err(error)
            }
        }
    }

    /// Open a connection purely to verify it. A bad password or missing file
    /// surfaces as a classified `DatabaseError`.
    pub async fn test_connection(
        &self,
        id: &ConnectionId,
        secret: Option<&[u8]>,
    ) -> DriverResult<()> {
        let connection = self.connect(id, secret).await?;
        connection.test().await
    }

    /// Verify a configuration without registering or saving it. The dialog uses
    /// this before the user commits a new connection, so a failed test cannot
    /// leave a stale entry in the manager.
    pub async fn test_config(
        &self,
        config: &ConnectionConfig,
        secret: Option<&[u8]>,
    ) -> DriverResult<()> {
        let driver = self
            .drivers
            .lock()
            .get(&config.driver_id)
            .cloned()
            .ok_or_else(|| {
                DatabaseError::driver(format!("no driver registered for {}", config.driver_id))
            })?;
        let connection = driver.driver.connect(config, secret).await?;
        let test_result = connection.test().await;
        let close_result = connection.close().await;
        test_result.and(close_result)
    }

    /// Disconnect: cancel every in-flight query, then close the native
    /// connection. This removes the close/step race recorded in Phase 1.
    pub async fn disconnect(&self, id: &ConnectionId) -> DriverResult<()> {
        let (connection, queries) = {
            let mut connections = self.connections.lock();
            let Some(managed) = connections.get_mut(id) else {
                return Ok(());
            };
            managed.status = ConnectionStatus::Disconnected;
            let connection = managed.connection.take();
            let queries = std::mem::take(&mut managed.active_queries);
            (connection, queries)
        };
        for query in queries {
            query.cancel();
        }
        if let Some(connection) = connection {
            connection.close().await?;
        }
        Ok(())
    }

    pub fn track_query(&self, id: &ConnectionId, handle: crate::driver::QueryHandle) {
        if let Some(managed) = self.connections.lock().get_mut(id) {
            managed.status = ConnectionStatus::Querying;
            managed.active_queries.push(handle);
        }
    }

    /// Cancel every in-flight query on `id` without disconnecting. The result
    /// view uses this while `QuerySession` owns the query handle, so a keypress
    /// does not have to reach into the session that is awaiting rows.
    pub fn cancel_queries(&self, id: &ConnectionId) {
        let connections = self.connections.lock();
        if let Some(managed) = connections.get(id) {
            for query in &managed.active_queries {
                query.cancel();
            }
        }
    }

    pub fn finish_query(&self, id: &ConnectionId, query_id: u64) {
        if let Some(managed) = self.connections.lock().get_mut(id) {
            managed
                .active_queries
                .retain(|tracked| tracked.id() != query_id);
            if managed.active_queries.is_empty() && managed.status == ConnectionStatus::Querying {
                managed.status = ConnectionStatus::Connected;
            }
        }
    }

    fn set_status(&self, id: &ConnectionId, status: ConnectionStatus) {
        if let Some(managed) = self.connections.lock().get_mut(id) {
            managed.status = status;
        }
    }
}

// Transactions are owned by `QuerySession`; the manager only owns connection
// state and in-flight query cancellation.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DataSource, TableRef};
    use crate::sqlite::SqliteDriver;

    fn config(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: ConnectionId(id.into()),
            display_name: id.into(),
            driver_id: crate::sqlite::SQLITE_DRIVER_ID.into(),
            source: DataSource::InMemory {
                name: format!("manager_{id}_{}", std::process::id()),
            },
            host: None,
            port: None,
            user: None,
            database: None,
            options: BTreeMap::new(),
        }
    }

    #[test]
    fn connect_test_and_disconnect() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(SqliteDriver));
            let id = ConnectionId("local".into());
            manager.upsert_config(config("local"));
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Disconnected));
            manager
                .test_connection(&id, None)
                .await
                .expect("test connection");
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Connected));
            manager.disconnect(&id).await.expect("disconnect");
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Disconnected));
        });
    }

    #[test]
    fn opens_a_real_file_and_describes_schema() {
        futures::executor::block_on(async {
            let path = std::env::temp_dir().join(format!(
                "zzz_database_manager_{}_{}.db",
                std::process::id(),
                "schema"
            ));
            std::fs::File::create(&path).expect("create database file");

            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(SqliteDriver));
            let id = ConnectionId("file".into());
            let mut file_config = config("file");
            file_config.source = DataSource::File { path: path.clone() };
            manager.upsert_config(file_config);

            let connection = manager.connect(&id, None).await.expect("connect");
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Connected));
            for statement in [
                "CREATE TABLE customers (id INTEGER PRIMARY KEY)",
                "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER NOT NULL REFERENCES customers(id))",
                "CREATE INDEX idx_orders_customer ON orders(customer_id)",
            ] {
                let query = connection
                    .execute(statement, &[])
                    .await
                    .expect("create schema");
                // Consume to completion so the statement has actually stepped
                // before the next one is prepared on this shared connection.
                while query.rows.next().await.is_some() {}
            }

            let catalogs = connection.catalogs().await.expect("catalogs");
            let table_names: Vec<&str> = catalogs[0].schemas[0]
                .tables
                .iter()
                .map(|table| table.reference.name.as_str())
                .collect();
            assert!(table_names.contains(&"orders"));

            let schema = connection
                .describe_table(&TableRef::new("orders"))
                .await
                .expect("describe");
            assert_eq!(
                schema.primary_key.as_ref().map(|key| key.columns.clone()),
                Some(vec!["id".to_owned()])
            );
            assert_eq!(schema.indexes.len(), 1);
            assert_eq!(schema.indexes[0].columns, vec!["customer_id".to_owned()]);
            assert_eq!(schema.foreign_keys.len(), 1);
            assert_eq!(schema.foreign_keys[0].referenced_table, "customers");
            assert_eq!(
                schema.foreign_keys[0].referenced_columns,
                vec!["id".to_owned()]
            );

            manager.disconnect(&id).await.expect("disconnect");
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Disconnected));
            std::fs::remove_file(&path).expect("remove database file");
        });
    }

    #[test]
    fn sync_configs_adds_removes_and_resets_changed_connections() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(SqliteDriver));
            manager.sync_configs(vec![config("a"), config("b")]);
            assert_eq!(manager.configs().len(), 2);

            let id = ConnectionId("a".into());
            manager.connect(&id, None).await.expect("connect");
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Connected));

            let mut renamed = config("a");
            renamed.display_name = "renamed".into();
            manager.sync_configs(vec![renamed.clone()]);
            assert_eq!(manager.configs(), vec![renamed]);
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Disconnected));

            manager.sync_configs(Vec::new());
            assert!(manager.configs().is_empty());
            assert_eq!(manager.status(&id), None);
        });
    }

    struct PasswordDriver;

    struct PasswordConnection;

    #[async_trait::async_trait]
    impl DatabaseDriver for PasswordDriver {
        fn id(&self) -> &'static str {
            "password-test"
        }

        fn display_name(&self) -> &'static str {
            "Password Test"
        }

        fn capabilities(&self) -> Capabilities {
            let mut capabilities = Capabilities::read_only();
            capabilities.requires_credentials = true;
            capabilities
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
            secret: Option<&[u8]>,
        ) -> DriverResult<Arc<dyn DriverConnection>> {
            if secret == Some(b"secret".as_slice()) {
                Ok(Arc::new(PasswordConnection))
            } else {
                Err(DatabaseError::authentication("bad password"))
            }
        }
    }

    #[async_trait::async_trait]
    impl DriverConnection for PasswordConnection {
        async fn test(&self) -> DriverResult<()> {
            Ok(())
        }

        async fn catalogs(&self) -> DriverResult<Vec<crate::model::Catalog>> {
            Ok(Vec::new())
        }

        async fn describe_table(
            &self,
            _table: &TableRef,
        ) -> DriverResult<crate::model::TableSchema> {
            Err(DatabaseError::driver("not used"))
        }

        async fn execute(
            &self,
            _sql: &str,
            _params: &[crate::model::Value],
        ) -> DriverResult<crate::driver::ExecutedQuery> {
            Err(DatabaseError::driver("not used"))
        }

        async fn begin(&self) -> DriverResult<crate::driver::TransactionId> {
            Err(DatabaseError::driver("not used"))
        }

        async fn commit(&self, _transaction: crate::driver::TransactionId) -> DriverResult<()> {
            Err(DatabaseError::driver("not used"))
        }

        async fn rollback(&self, _transaction: crate::driver::TransactionId) -> DriverResult<()> {
            Err(DatabaseError::driver("not used"))
        }

        async fn explain(&self, _sql: &str) -> DriverResult<crate::driver::ExplainPlan> {
            Err(DatabaseError::driver("not used"))
        }

        async fn close(&self) -> DriverResult<()> {
            Ok(())
        }
    }

    #[test]
    fn cancel_queries_stops_tracked_queries_without_disconnecting() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(SqliteDriver));
            let id = ConnectionId("cancel".into());
            manager.upsert_config(config("cancel"));
            let connection = manager.connect(&id, None).await.expect("connect");
            let query = connection
                .execute(
                    "WITH RECURSIVE counter(value) AS (
                        VALUES(1) UNION ALL SELECT value + 1 FROM counter WHERE value < 100000000
                    ) SELECT value FROM counter",
                    &[],
                )
                .await
                .expect("execute");
            manager.track_query(&id, query.handle.clone());
            assert!(query.rows.next().await.is_some(), "stream started");
            manager.cancel_queries(&id);
            assert!(query.handle.is_cancelled());
            while query.rows.next().await.is_some() {}
            manager.finish_query(&id, query.handle.id());
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Connected));
            manager.disconnect(&id).await.expect("disconnect");
        });
    }

    #[test]
    fn test_config_leaves_the_registry_untouched() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(SqliteDriver));
            let config = config("transient");
            manager
                .test_config(&config, None)
                .await
                .expect("test config");
            assert!(manager.configs().is_empty());
            assert_eq!(manager.status(&config.id), None);
        });
    }

    #[test]
    fn rejected_credentials_are_classified_and_never_stored() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            manager.register_driver(Arc::new(PasswordDriver));
            let id = ConnectionId("auth".into());
            let mut password_config = config("auth");
            password_config.driver_id = PasswordDriver.id().to_owned();
            manager.upsert_config(password_config);

            let error = manager
                .connect(&id, Some(b"wrong"))
                .await
                .err()
                .expect("authentication error");
            assert_eq!(error.kind, DatabaseErrorKind::Authentication);
            assert_eq!(manager.status(&id), Some(ConnectionStatus::Disconnected));

            manager
                .test_connection(&id, Some(b"secret"))
                .await
                .expect("valid secret");

            let rendered = format!("{:?}", manager.configs());
            assert!(!rendered.contains("wrong"));
            assert!(!rendered.contains("secret"));
        });
    }

    #[test]
    fn missing_driver_is_a_classified_error() {
        futures::executor::block_on(async {
            let manager = ConnectionManager::new();
            let id = ConnectionId("local".into());
            manager.upsert_config(config("local"));
            let error = manager.connect(&id, None).await.err().expect("error");
            assert_eq!(error.kind, DatabaseErrorKind::Driver);
        });
    }

    #[test]
    fn caps_come_from_registry_not_driver_id() {
        let manager = ConnectionManager::new();
        manager.register_driver(Arc::new(SqliteDriver));
        assert_eq!(
            manager.capabilities_for(crate::sqlite::SQLITE_DRIVER_ID),
            Some(Capabilities::sqlite())
        );
        assert_eq!(manager.capabilities_for("unknown"), None);
    }
}
