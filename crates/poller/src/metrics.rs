//! Prometheus metrics for TxWatch (enabled with the `metrics` feature flag).
//!
//! Exposes three counters:
//! - `txwatch_transactions_total`       — total transactions processed
//! - `txwatch_alerts_total`             — total alert payloads sent (rules matched)
//! - `txwatch_webhook_failures_total`   — total permanent webhook delivery failures
//!
//! An optional HTTP `/metrics` endpoint can be started by calling [`serve_metrics`].
//! It serves the Prometheus text exposition format on the configured bind address.
//! The same server also exposes a `/health` endpoint returning a JSON health check
//! response suitable for Kubernetes liveness/readiness probes and load balancers.

use anyhow::{Context, Result};
use prometheus::{register_int_counter, IntCounter};
use std::{net::SocketAddr, sync::OnceLock, time::Instant};

use http_body_util::Full;
use hyper::{body::Bytes, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

// ── Counters ──────────────────────────────────────────────────────────────────

fn transactions_total() -> &'static IntCounter {
    static C: OnceLock<IntCounter> = OnceLock::new();
    C.get_or_init(|| {
        register_int_counter!(
            "txwatch_transactions_total",
            "Total Stellar transactions processed across all watched contracts"
        )
        .expect("register txwatch_transactions_total")
    })
}

fn alerts_total() -> &'static IntCounter {
    static C: OnceLock<IntCounter> = OnceLock::new();
    C.get_or_init(|| {
        register_int_counter!(
            "txwatch_alerts_total",
            "Total alert payloads sent (rules matched)"
        )
        .expect("register txwatch_alerts_total")
    })
}

fn webhook_failures_total() -> &'static IntCounter {
    static C: OnceLock<IntCounter> = OnceLock::new();
    C.get_or_init(|| {
        register_int_counter!(
            "txwatch_webhook_failures_total",
            "Total permanent webhook delivery failures (after all retries)"
        )
        .expect("register txwatch_webhook_failures_total")
    })
}

/// Increment the `txwatch_transactions_total` counter by `n`.
pub fn inc_transactions(n: u64) {
    transactions_total().inc_by(n);
}

/// Increment the `txwatch_alerts_total` counter by `n`.
pub fn inc_alerts(n: u64) {
    alerts_total().inc_by(n);
}

/// Increment the `txwatch_webhook_failures_total` counter by 1.
pub fn inc_webhook_failures() {
    webhook_failures_total().inc();
}

// ── /health state ─────────────────────────────────────────────────────────────

/// Snapshot of the process state reported by the `/health` endpoint.
#[derive(Clone, Debug)]
pub struct HealthState {
    /// Crate version reported in the health payload.
    pub version: String,
    /// Number of contracts currently being watched.
    pub contracts: usize,
    /// Instant the process started, used to compute uptime.
    pub started_at: Instant,
}

impl HealthState {
    /// Build a health state snapshot, capturing the current instant as the start time.
    pub fn new(version: impl Into<String>, contracts: usize) -> Self {
        Self {
            version: version.into(),
            contracts,
            started_at: Instant::now(),
        }
    }

    /// Render the health check response body as JSON.
    fn to_json(&self) -> String {
        format!(
            "{{\"status\":\"ok\",\"version\":\"{}\",\"contracts\":{},\"uptime_seconds\":{}}}",
            self.version,
            self.contracts,
            self.started_at.elapsed().as_secs()
        )
    }
}

// ── /metrics + /health HTTP endpoint ──────────────────────────────────────────

/// Serve the Prometheus `/metrics` endpoint and the `/health` endpoint on `addr`.
/// Spawns a background task and returns immediately.
pub async fn serve_metrics(addr: SocketAddr) -> Result<()> {
    serve_metrics_with_health(addr, HealthState::new(env!("CARGO_PKG_VERSION"), 0)).await
}

/// Serve `/metrics` and `/health` on `addr`, reporting the given [`HealthState`]
/// from the `/health` endpoint. Spawns a background task and returns immediately.
pub async fn serve_metrics_with_health(addr: SocketAddr, health: HealthState) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind metrics endpoint on {}", addr))?;

    tracing::info!(addr = %addr, "Prometheus /metrics and /health endpoints listening");

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let io = TokioIo::new(stream);
            let health = health.clone();
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(
                        io,
                        hyper::service::service_fn(move |req| {
                            handle_request(req, health.clone())
                        }),
                    )
                    .await;
            });
        }
    });

    Ok(())
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    health: HealthState,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    match req.uri().path() {
        "/health" => Ok(handle_health(&health)),
        _ => Ok(handle_metrics()),
    }
}

fn handle_health(health: &HealthState) -> Response<Full<Bytes>> {
    Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(health.to_json())))
        .unwrap()
}

fn handle_metrics() -> Response<Full<Bytes>> {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buf = Vec::new();
    encoder
        .encode(&prometheus::gather(), &mut buf)
        .unwrap_or_default();
    Response::builder()
        .status(200)
        .header("Content-Type", encoder.format_type())
        .body(Full::new(Bytes::from(buf)))
        .unwrap()
}
