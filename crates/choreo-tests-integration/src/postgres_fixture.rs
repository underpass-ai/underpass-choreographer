//! Shared fixture for Postgres integration tests.
//!
//! Every test gets its own Postgres container — keeps tests isolated
//! and lets testcontainers reap resources cleanly per case. The
//! `start` helper returns both the pool (with migrations applied)
//! and the container handle so the test's lifetime keeps the
//! container alive.

use std::time::Duration;

use choreo_adapters::postgres::{PostgresConfig, PostgresPool};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};

const PG_IMAGE: &str = "postgres";
const PG_TAG: &str = "16-alpine";
const PG_USER: &str = "choreo";
const PG_PASSWORD: &str = "choreo";
const PG_DB: &str = "choreo";

pub async fn start() -> (PostgresPool, testcontainers::ContainerAsync<GenericImage>) {
    let container = GenericImage::new(PG_IMAGE, PG_TAG)
        .with_exposed_port(5432_u16.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_USER", PG_USER)
        .with_env_var("POSTGRES_PASSWORD", PG_PASSWORD)
        .with_env_var("POSTGRES_DB", PG_DB)
        .start()
        .await
        .expect("postgres container should start");
    let port = container
        .get_host_port_ipv4(5432_u16.tcp())
        .await
        .expect("host port");
    let url = format!("postgres://{PG_USER}:{PG_PASSWORD}@127.0.0.1:{port}/{PG_DB}");

    let mut cfg = PostgresConfig::from_url(url);
    cfg.acquire_timeout = Duration::from_secs(10);

    let mut last_err = None;
    for _ in 0..20 {
        match PostgresPool::connect(&cfg).await {
            Ok(pool) => {
                pool.run_migrations()
                    .await
                    .expect("migrations must apply on a fresh database");
                return (pool, container);
            }
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
    panic!("could not connect to postgres after warmup: {last_err:?}");
}
