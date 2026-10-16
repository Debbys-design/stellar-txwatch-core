# Configuration

TxWatch is configured through a combination of CLI flags, environment variables, and
an optional TOML configuration file. This document describes the available options
and how they interact.

## CLI flags

| Flag | Description | Default |
| --- | --- | --- |
| `--rpc-url <URL>` | Soroban RPC endpoint used to poll contract events. | `http://localhost:8000/soroban/rpc` |
| `--start-ledger <N>` | Ledger to start polling from. | latest |
| `--poll-interval <SECS>` | Seconds between polling rounds. | `5` |
| `--contract <ID>` | Contract ID to watch. May be repeated. | none |
| `--features <LIST>` | Comma-separated feature list (e.g. `metrics`). | none |

## Environment variables

| Variable | Description |
| --- | --- |
| `TXWATCH_RPC_URL` | Overrides `--rpc-url`. |
| `TXWATCH_POLL_INTERVAL` | Overrides `--poll-interval`. |
| `TXWATCH_METRICS_PORT` | Port used by the observability HTTP server. |

## Observability endpoints

When the `metrics` feature is enabled, TxWatch starts a small HTTP server that
exposes both the Prometheus metrics endpoint and a JSON health endpoint on the
same port (default `9090`, configurable via `TXWATCH_METRICS_PORT`).

### `GET /metrics`

Prometheus text exposition format. Scrape this endpoint with your Prometheus
server or any compatible agent.

### `GET /health`

Returns HTTP `200` with a JSON body when TxWatch is running:

```json
{"status": "ok", "version": "0.4.0", "contracts": 3, "uptime_seconds": 3600}
```

| Field | Description |
| --- | --- |
| `status` | `"ok"` while the poller is healthy. |
| `version` | The running TxWatch version. |
| `contracts` | Number of contracts currently being watched. |
| `uptime_seconds` | Seconds since the poller started. |

This endpoint is intended for container orchestration systems and load balancers
that need a lightweight liveness/readiness signal.

## Kubernetes probes

Because `/health` returns `200` only while the poller is running, it can be used
directly for both liveness and readiness probes. The following example assumes the
observability server is listening on port `9090`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: txwatch-poller
spec:
  replicas: 1
  selector:
    matchLabels:
      app: txwatch-poller
  template:
    metadata:
      labels:
        app: txwatch-poller
    spec:
      containers:
        - name: txwatch-poller
          image: txwatch/poller:0.4.0
          args: ["--features", "metrics"]
          ports:
            - name: observability
              containerPort: 9090
          livenessProbe:
            httpGet:
              path: /health
              port: observability
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 2
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health
              port: observability
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 2
            failureThreshold: 3
```

If you expose the observability server on a different port, update
`containerPort` and the probe `port` values accordingly.

## Configuration file

A TOML file may be supplied with `--config <PATH>`. CLI flags take precedence over
values in the file, and environment variables take precedence over both.

```toml
rpc_url = "https://soroban-testnet.stellar.org"
poll_interval = 5

[[contracts]]
id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
```
