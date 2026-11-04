//! End-to-end tests for the extension driver protocol.
//!
//! These spawn the real `mock-database-driver` binary and talk to it over
//! stdin/stdout; nothing here calls the protocol handlers directly.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use database_core::{
    Capabilities, ConnectionConfig, ConnectionId, ConnectionManager, DataSource, DatabaseErrorKind,
    ExtensionDriver, ExtensionDriverConfig, QuerySession, ResultSetState, TableRef, Value,
};

fn mock_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mock-database-driver"))
}

fn driver_config(timeout: Duration) -> ExtensionDriverConfig {
    let mut config =
        ExtensionDriverConfig::new("test.mock", "Mock", mock_binary(), Capabilities::sqlite());
    config.control_timeout = timeout;
    config.execute_timeout = timeout;
    config
}

fn connection_config(id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: ConnectionId(id.to_owned()),
        display_name: id.to_owned(),
        driver_id: "test.mock".to_owned(),
        source: DataSource::InMemory {
            name: id.to_owned(),
        },
        host: None,
        port: None,
        user: None,
        database: None,
        options: Default::default(),
    }
}

fn manager_for(
    config: ExtensionDriverConfig,
    connection: ConnectionConfig,
) -> Arc<ConnectionManager> {
    let manager = Arc::new(ConnectionManager::new());
    manager.register_driver(Arc::new(ExtensionDriver::new(config)));
    manager.upsert_config(connection);
    manager
}

#[test]
fn extension_driver_round_trips_every_method() {
    futures::executor::block_on(async {
        let config = connection_config("round-trip");
        let manager = manager_for(driver_config(Duration::from_secs(5)), config.clone());
        let connection = manager
            .connect(&config.id, Some(b"secret"))
            .await
            .expect("connect");

        connection.test().await.expect("test");
        let catalogs = connection.catalogs().await.expect("catalogs");
        assert_eq!(catalogs.len(), 1);
        assert_eq!(catalogs[0].schemas[0].tables[0].reference.name, "widgets");

        let schema = connection
            .describe_table(&TableRef::new("widgets"))
            .await
            .expect("describe");
        assert_eq!(schema.columns.len(), 2);
        assert_eq!(
            schema.primary_key.as_ref().map(|key| key.columns.clone()),
            Some(vec!["id".to_owned()])
        );

        let executed = connection
            .execute("SELECT ?", &[Value::Integer(7)])
            .await
            .expect("execute");
        assert_eq!(executed.metadata.columns.len(), 1);
        let row = executed.rows.next().await.expect("row").expect("ok");
        assert_eq!(row.get(0), Some(&Value::Integer(7)));
        assert!(executed.rows.next().await.is_none());

        let transaction = connection.begin().await.expect("begin");
        connection.commit(transaction).await.expect("commit");

        let plan = connection.explain("SELECT 1").await.expect("explain");
        assert_eq!(plan.nodes[0].detail, "mock scan");

        connection.close().await.expect("close");
        let error = connection.test().await.expect_err("closed connection");
        assert_eq!(error.kind, DatabaseErrorKind::Connection);
    });
}

#[test]
fn extension_errors_are_mapped_by_code() {
    futures::executor::block_on(async {
        let config = connection_config("syntax");
        let manager = manager_for(driver_config(Duration::from_secs(5)), config.clone());
        let connection = manager.connect(&config.id, None).await.expect("connect");
        let error = connection
            .execute("syntax problem", &[])
            .await
            .err()
            .expect("syntax error");
        assert_eq!(error.kind, DatabaseErrorKind::Syntax);
        connection.close().await.expect("close");
    });
}

#[test]
fn cancelling_a_session_stops_a_subprocess_query() {
    futures::executor::block_on(async {
        let config = connection_config("cancel");
        let manager = manager_for(driver_config(Duration::from_secs(5)), config.clone());
        let connection = manager.connect(&config.id, None).await.expect("connect");
        let mut session = QuerySession::new(
            config.id.clone(),
            connection,
            manager.clone(),
            Capabilities::sqlite(),
        );

        let cancel_manager = manager.clone();
        let connection_id = config.id.clone();
        // Cancel from another thread so the test does not block the executor
        // that is polling the query.
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel_manager.cancel_queries(&connection_id);
        });
        let result = session.execute("slow query", &[]).await;
        canceller.join().expect("cancel thread");
        result.expect("cancellation is not an error");
        assert!(session.is_cancelled());
        assert_eq!(
            session.result_set().map(|set| set.state),
            Some(ResultSetState::Cancelled)
        );
    });
}

#[test]
fn a_request_without_a_response_times_out() {
    futures::executor::block_on(async {
        let config = connection_config("timeout");
        let manager = manager_for(driver_config(Duration::from_millis(200)), config.clone());
        let connection = manager.connect(&config.id, None).await.expect("connect");
        let error = connection
            .execute("hang forever", &[])
            .await
            .err()
            .expect("timeout");
        assert_eq!(error.kind, DatabaseErrorKind::Timeout);
        connection.close().await.expect("close");
    });
}

#[cfg(target_os = "linux")]
#[test]
fn dropping_a_connection_kills_the_driver_process() {
    futures::executor::block_on(async {
        let pid_file = std::env::temp_dir().join(format!(
            "zzz-mock-driver-{}-{}.pid",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let config = connection_config("drop");
        let mut driver = driver_config(Duration::from_secs(5));
        driver.args = vec!["--pid-file".to_owned(), pid_file.display().to_string()];
        let manager = manager_for(driver, config.clone());
        let connection = manager.connect(&config.id, None).await.expect("connect");
        let pid = std::fs::read_to_string(&pid_file)
            .expect("pid file")
            .trim()
            .to_owned();
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());

        drop(connection);
        // The manager is the connection owner; dropping it must terminate the
        // child even when nobody calls `close` explicitly.
        drop(manager);
        // Drop terminates the child synchronously, but allow the kernel a
        // moment to reap the process entry.
        for _ in 0..60 {
            if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                std::fs::remove_file(&pid_file).expect("remove pid file");
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("extension driver process {pid} is still running after drop");
    });
}
