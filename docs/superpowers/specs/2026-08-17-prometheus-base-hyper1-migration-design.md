# Prometheus Base and Hyper 1 Migration Design

## Goal

Remove the `prometheus_exporter_base` dependency by integrating the subset of
its metric-rendering and HTTP-server functionality used by this exporter, then
upgrade the application to Hyper 1 and HTTP 1 without changing its observable
Prometheus output.

## Scope

The migration includes:

- `MetricType`, `RenderToPrometheus`, `PrometheusInstance`,
  `PrometheusMetricBuilder`, and `PrometheusMetric`.
- Compile-time enforcement of mandatory metric name, type, and help fields.
- Rendering of HELP and TYPE headers, labels, numeric values, and optional
  timestamps.
- A Hyper server that binds to the configured socket address and accepts the
  existing application options as shared state.
- Request validation for Basic authentication, `/metrics`, and `GET`.
- The existing HTTP status behavior: `401`, `404`, `405`, `500`, and `200`.
- `Content-Type: text/plain; version=0.0.4` on successful metric responses.

The two client helpers from `prometheus_exporter_base`, which issue outbound
HTTPS requests and optionally deserialize JSON, are excluded. The exporter does
not use them, and copying them would introduce an unused TLS client stack.

## Source Analysis

`prometheus_exporter_base` 1.4.0 has two independent responsibilities.

The metric side consists of small formatting types. `PrometheusMetricBuilder`
uses marker types to make name, metric type, and help mandatory at compile
time. `PrometheusInstance` stores borrowed labels, one numeric value, and an
optional millisecond timestamp. `PrometheusMetric` renders its header and
appends pre-rendered instances.

The server side uses Hyper 0.14. It shares application options through `Arc`,
validates authorization before path and method, invokes an asynchronous
callback for valid requests, and maps callback failures to status 500. The
current exporter configures `Authorization::None`, but retaining Basic auth
keeps the server behavior complete and inexpensive.

The exporter directly uses all metric types from the library and its server
entry point. Its request argument is currently ignored. No outbound client
helper is used.

## Architecture

Create a private `src/prometheus/` module with focused files for metric type,
instance rendering, metric building, and metric aggregation. The public surface
inside the binary will preserve the names currently imported from
`prometheus_exporter_base`, minimizing changes in `wireguard.rs`.

Create `src/server.rs` for Hyper-specific code. It will expose the server
options, authorization mode, a testable request handler, and the TCP accept
loop. The handler will use HTTP 1 request and response types with
`hyper::body::Incoming` for network requests and `http_body_util::Full<Bytes>`
for responses. The accept loop will adapt Tokio streams with `hyper-util` and
serve HTTP/1 connections.

`main.rs` will continue to own command-line parsing and logging setup. It will
construct server options and pass `perform_request` to the internal server.
Because `perform_request` does not inspect the request, its interface may be
simplified to accept only shared application options. This keeps Hyper types at
the server boundary.

## Request Behavior

Checks retain their original order:

1. Reject invalid Basic authentication with `401 Unauthorized`.
2. Reject paths other than `/metrics` with `404 Not Found`.
3. Reject methods other than GET with `405 Method Not Allowed`.
4. Invoke metric generation for a valid request.
5. Return the generated text and Prometheus content type with `200 OK`.
6. Return the callback error text with `500 Internal Server Error`.

Authorization is disabled by the current application configuration. Basic auth
will still compare the decoded credentials to `:<configured password>`, matching
the source library's behavior where the username is ignored.

## Dependencies

- Upgrade `hyper` from `0.14.32` to `1.11.0` with its server and HTTP/1 support.
- Upgrade `http` from `0.2.12` to `1.5.0`.
- Add `hyper-util` for Tokio I/O adaptation.
- Add `http-body-util` and `bytes` for concrete response bodies.
- Add `base64` for retained Basic authentication.
- Enable Tokio networking and I/O features required by `TcpListener` and
  Hyper's Tokio adapter.
- Add `num` to preserve the generic numeric constraints of
  `PrometheusInstance`.
- Remove `prometheus_exporter_base` and its obsolete Hyper 0.14 dependency
  graph.

Exact compatible versions and features will be resolved through crates.io and
recorded in `Cargo.lock` during implementation.

## Error Handling

The TCP listener returns bind errors to `main`. Per-connection protocol errors
are logged and do not terminate the accept loop. Request-handler errors are
converted into HTTP responses, so the Hyper service itself remains infallible.

The old `ExporterError` conversions for `hyper::Error` and `http::Error` will be
removed if no call site remains after compilation. Existing parsing and I/O
error behavior remains unchanged.

## Testing

Development follows red-green-refactor cycles.

- Port focused unit tests for metric headers, labels, unlabeled values, and
  timestamps before implementing the internal metric types.
- Keep the existing full-string metric rendering tests as compatibility tests.
- Add handler tests for successful GET, wrong path, wrong method, failed Basic
  authentication, successful Basic authentication, and callback failure.
- Run the full existing test suite after each component migration.
- Finish with formatting, all tests, Clippy across targets and features, debug
  build, release build, and a dependency-tree check proving that Hyper 0.14,
  HTTP 0.2, and `prometheus_exporter_base` are absent.

## Non-Goals

- No changes to command-line flags, environment variables, metric names,
  labels, values, or ordering.
- No TLS listener or outbound HTTP client.
- No adoption of a different Prometheus client library.
- No unrelated refactoring of WireGuard parsing or metric calculation.
