//! gRPC server lifecycle.
//!
//! [`serve`] takes a composed [`Application`] and runs the server
//! until a shutdown signal is received. Kept separate from
//! [`crate::compose`] so wiring and lifetime are each
//! unit-testable.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};

use crate::compose::Application;

/// Bind the gRPC server, spawn the optional NATS subscriber, and
/// serve until SIGTERM or SIGINT.
pub async fn serve(app: Application) -> Result<()> {
    let Application {
        service_config,
        grpc_service,
        nats_subscriber,
        ..
    } = app;

    let subscriber_handle = match nats_subscriber {
        Some(subscriber) => Some(
            subscriber
                .spawn()
                .await
                .context("failed to spawn nats trigger subscriber")?,
        ),
        None => None,
    };

    let addr: SocketAddr = format!("0.0.0.0:{}", service_config.grpc_port)
        .parse()
        .with_context(|| {
            format!(
                "invalid grpc bind address for port {}",
                service_config.grpc_port
            )
        })?;
    info!(addr = %addr, "grpc server starting");

    tonic::transport::Server::builder()
        .add_service(grpc_service.into_server())
        .serve_with_shutdown(addr, shutdown_signal())
        .await
        .context("grpc server terminated with error")?;

    info!("grpc server stopped");

    if let Some(handle) = subscriber_handle {
        handle.abort();
        match handle.await {
            Ok(()) => info!("nats subscriber stopped"),
            Err(err) if err.is_cancelled() => info!("nats subscriber cancelled"),
            Err(err) => error!(error = %err, "nats subscriber task errored"),
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(err) => {
            error!(error = %err, "cannot install SIGTERM handler; shutdown relies on SIGINT only");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = sigterm.recv() => info!("received SIGTERM; shutting down"),
        _ = tokio::signal::ctrl_c() => info!("received SIGINT; shutting down"),
    }
}
