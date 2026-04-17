//! gRPC + HTTP server lifecycle.
//!
//! [`serve`] takes a composed [`Application`] and runs two servers
//! concurrently until a shutdown signal is received:
//!
//! - The gRPC server (`ChoreographerService`) on
//!   `CHOREO_GRPC_PORT` (default 50055).
//! - The HTTP server exposing `/healthz` and `/readyz` on
//!   `CHOREO_HTTP_PORT` (default 8080).
//!
//! Both share the same shutdown signal (SIGTERM or SIGINT).
//! Returning the first error wins; diagnostics on the other servers
//! come through tracing.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tracing::{error, info};

use crate::compose::Application;

/// Bind gRPC + HTTP, spawn the optional NATS subscriber, and serve
/// until SIGTERM or SIGINT.
pub async fn serve(app: Application) -> Result<()> {
    let Application {
        service_config,
        grpc_service,
        nats_subscriber,
        health_state,
        ..
    } = app;

    // Shutdown channel: a single send triggers both servers.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let subscriber_handle = match nats_subscriber {
        Some(subscriber) => Some(
            subscriber
                .spawn()
                .await
                .context("failed to spawn nats trigger subscriber")?,
        ),
        None => None,
    };

    let grpc_addr: SocketAddr = format!("0.0.0.0:{}", service_config.grpc_port)
        .parse()
        .with_context(|| {
            format!(
                "invalid grpc bind address for port {}",
                service_config.grpc_port
            )
        })?;
    let http_addr: SocketAddr = format!("0.0.0.0:{}", service_config.http_port)
        .parse()
        .with_context(|| {
            format!(
                "invalid http bind address for port {}",
                service_config.http_port
            )
        })?;

    info!(grpc = %grpc_addr, http = %http_addr, "servers starting");

    // Driver: one task waits for the OS signal and flips the watch.
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    let grpc_shutdown = wait_for_shutdown(shutdown_rx.clone());
    let grpc_task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(grpc_service.into_server())
            .serve_with_shutdown(grpc_addr, grpc_shutdown)
            .await
            .context("grpc server terminated with error")
    });

    let http_shutdown = wait_for_shutdown(shutdown_rx);
    let http_task = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr)
            .await
            .with_context(|| format!("failed to bind http listener on {http_addr}"))?;
        axum::serve(listener, crate::health::router(health_state))
            .with_graceful_shutdown(http_shutdown)
            .await
            .context("http server terminated with error")
    });

    // Await both. If one exits with an error we still try to drain
    // the other so diagnostics aren't dropped.
    let grpc_res = grpc_task.await.context("grpc server task join failed")?;
    let http_res = http_task.await.context("http server task join failed")?;

    info!("servers stopped");

    if let Some(handle) = subscriber_handle {
        handle.abort();
        match handle.await {
            Ok(()) => info!("nats subscriber stopped"),
            Err(err) if err.is_cancelled() => info!("nats subscriber cancelled"),
            Err(err) => error!(error = %err, "nats subscriber task errored"),
        }
    }

    grpc_res?;
    http_res?;
    Ok(())
}

async fn wait_for_shutdown(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            break;
        }
    }
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
