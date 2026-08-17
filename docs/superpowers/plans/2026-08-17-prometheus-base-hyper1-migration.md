# Prometheus Base and Hyper 1 Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `prometheus_exporter_base` with internal metric-rendering and server modules, then run the exporter exclusively on Hyper 1.11.0 and HTTP 1.5.0 without changing its metrics or HTTP behavior.

**Architecture:** A private `src/prometheus/` module preserves the metric API currently consumed by `wireguard.rs`. A separate `src/server.rs` owns Hyper 1 request handling and the Tokio TCP accept loop, keeping HTTP types out of metric generation. `main.rs` remains the composition root and passes shared `Options` to the server callback.

**Tech Stack:** Rust 2018, Tokio 1.53.1, Hyper 1.11.0, HTTP 1.5.0, hyper-util 0.1.20, http-body-util 0.1.5, bytes 1.12.1, base64 0.23.1, num 0.4.3. Hyper 1 and HTTP 1 use temporary Cargo aliases during the transition so the old server remains compilable until application integration.

## Global Constraints

- Only read or modify files inside this repository during implementation.
- Preserve all command-line flags, environment variables, metric names, labels, values, ordering, and output formatting.
- Preserve server check order: authorization, path, method, callback.
- Preserve HTTP statuses `401`, `404`, `405`, `500`, and `200`.
- Preserve successful response content type `text/plain; version=0.0.4`.
- Preserve Basic authentication semantics: ignore username and compare decoded credentials with `:<configured password>`.
- Do not add TLS, outbound HTTP clients, or the unused client helpers from `prometheus_exporter_base`.
- Follow red-green-refactor: each production component is implemented only after its focused test fails for the expected reason.
- Do not commit implementation changes unless the user explicitly requests commits during execution.

## File Structure

- Create `src/prometheus/mod.rs`: private module facade and marker types used by the compile-time builder.
- Create `src/prometheus/metric_type.rs`: `MetricType`, string conversion, and display behavior.
- Create `src/prometheus/render_to_prometheus.rs`: rendering trait implemented by metric instances.
- Create `src/prometheus/prometheus_instance.rs`: labels, numeric value, optional timestamp, and instance rendering.
- Create `src/prometheus/prometheus_metric_builder.rs`: compile-time checked metric builder.
- Create `src/prometheus/prometheus_metric.rs`: HELP/TYPE header and instance aggregation.
- Create `src/server.rs`: authorization, request routing, response construction, and Hyper 1 TCP server.
- Modify `src/main.rs`: register internal modules, remove Hyper 0.14 request types, and launch the internal server.
- Modify `src/wireguard.rs`: import metric types from `crate::prometheus`.
- Modify `src/exporter_error.rs`: remove obsolete Hyper and HTTP error variants and conversions.
- Modify `Cargo.toml`: replace the old server dependency graph with Hyper 1 dependencies.
- Modify `Cargo.lock`: record the resolved dependency graph.

---

### Task 1: Internal Prometheus Metric Types

**Files:**
- Create: `src/prometheus/mod.rs`
- Create: `src/prometheus/metric_type.rs`
- Create: `src/prometheus/render_to_prometheus.rs`
- Create: `src/prometheus/prometheus_instance.rs`
- Create: `src/prometheus/prometheus_metric_builder.rs`
- Create: `src/prometheus/prometheus_metric.rs`
- Modify: `src/main.rs:7-16`
- Modify: `src/wireguard.rs:1-7`
- Modify: `Cargo.toml:21-33`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: `crate::prometheus::{MetricType, PrometheusInstance, PrometheusMetric}`.
- Produces: `PrometheusMetric::build() -> PrometheusMetricBuilder<'a, No, No, No>`.
- Produces: `PrometheusInstance::new()`, `with_label`, `with_value`, `with_timestamp`, and `with_current_timestamp`.
- Consumes: Existing calls in `WireGuard::render_with_names` without changing their behavior.

- [ ] **Step 1: Record the current compatibility baseline**

Run:

```bash
cargo test
```

Expected: all existing tests pass. Save the test count in the execution notes; do not modify code if the baseline fails.

- [ ] **Step 2: Add module declarations and failing metric tests**

Add `mod prometheus;` to `src/main.rs`. Create the six module files and add tests first. The initial tests must cover these exact outputs:

```rust
#[test]
fn renders_metric_header() {
    let metric = PrometheusMetric::build()
        .with_name("wireguard_test_total")
        .with_metric_type(MetricType::Counter)
        .with_help("Test metric")
        .build();

    assert_eq!(
        metric.render(),
        "# HELP wireguard_test_total Test metric\n# TYPE wireguard_test_total counter\n"
    );
}

#[test]
fn renders_labels_value_and_timestamp() {
    let instance = PrometheusInstance::new()
        .with_label("interface", "wg0")
        .with_label("peer", "abc")
        .with_timestamp(1234)
        .with_value(42_u128);

    assert_eq!(instance.render(), "{interface=\"wg0\",peer=\"abc\"} 42 1234");
}

#[test]
fn renders_value_without_labels() {
    let instance = PrometheusInstance::new().with_value(42_u128);
    assert_eq!(instance.render(), " 42");
}
```

Also test all `MetricType` display values: `counter`, `gauge`, `histogram`, and `summary`.

- [ ] **Step 3: Run the focused tests and verify RED**

Run:

```bash
cargo test prometheus::
```

Expected: compilation fails because the tested metric types and methods are not implemented. Confirm the failure is about missing symbols, not malformed test code.

- [ ] **Step 4: Implement the rendering trait and metric type**

Implement:

```rust
pub(crate) trait RenderToPrometheus: std::fmt::Debug {
    fn render(&self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}
```

Implement `AsRef<str>` and `Display` with the exact lowercase Prometheus names.

- [ ] **Step 5: Implement `PrometheusInstance`**

Port the generic instance semantics using `num::Num`, `PhantomData`, `MissingValue`, and the existing `Yes` marker. Keep borrowed labels as `Vec<(&'a str, &'a str)>`. Only implement `RenderToPrometheus` for instances whose value marker is `Yes`.

The render rules are exact:

```text
no labels:  <value>[ <timestamp>]
labels: {key="value",key2="value2"} <value>[ <timestamp>]
```

`with_current_timestamp` must use milliseconds since `UNIX_EPOCH` and return `Result<Self, SystemTimeError>`.

- [ ] **Step 6: Implement the compile-time metric builder**

Define `ToAssign`, `Yes`, and `No` in `src/prometheus/mod.rs`. Port the builder states so `with_name`, `with_metric_type`, and `with_help` can each be called only once, and `build` exists only for `PrometheusMetricBuilder<Yes, Yes, Yes>`.

The completed builder must construct:

```rust
PrometheusMetric {
    counter_name: self.name(),
    counter_type: self.metric_type(),
    counter_help: self.help(),
    rendered_instances: Vec::new(),
}
```

- [ ] **Step 7: Implement metric aggregation and rendering**

Implement `PrometheusMetric::build`, `render_and_append_instance`, and `render`. Preserve the existing format exactly:

```rust
fn render_header(&self) -> String {
    format!(
        "# HELP {} {}\n# TYPE {} {}\n",
        self.counter_name, self.counter_help, self.counter_name, self.counter_type
    )
}
```

Each rendered instance is prefixed with the metric name and terminated by one newline.

- [ ] **Step 8: Verify the focused metric tests are GREEN**

Run:

```bash
cargo test prometheus::
```

Expected: all new metric tests pass.

- [ ] **Step 9: Switch WireGuard rendering to the internal module**

Change only the import in `src/wireguard.rs`:

```rust
use crate::prometheus::{MetricType, PrometheusInstance, PrometheusMetric};
```

Add `num = "0.4.3"` to `Cargo.toml`. Keep `prometheus_exporter_base` temporarily because `main.rs` still uses its server.

- [ ] **Step 10: Verify output compatibility**

Run:

```bash
cargo test wireguard::tests::test_render_to_prometheus_simple
cargo test wireguard::tests::test_render_to_prometheus_complex
cargo test
```

Expected: the two full-string compatibility tests and the entire suite pass without changing expected strings.

- [ ] **Step 11: Review Task 1 diff**

Run:

```bash
cargo fmt --check
```

Expected: formatting and whitespace checks pass; the diff contains only the internal metric implementation and import/dependency wiring.

---

### Task 2: Hyper 1 Request Handler

**Files:**
- Create: `src/server.rs`
- Modify: `src/main.rs:1-25`
- Modify: `Cargo.toml:21-33`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: `Authorization::{None, Basic(String)}`.
- Produces: `ServerOptions { addr: SocketAddr, authorization: Authorization }`.
- Produces: `async fn handle_request<O, F, Fut, B>(...) -> Response<Full<Bytes>>` for unit tests and network serving.
- Consumes: callback `F: Fn(Arc<O>) -> Fut`, where `Fut::Output = Result<String, Box<dyn Error + Send + Sync>>`.

- [ ] **Step 1: Add Hyper 1 dependencies and handler test scaffold**

Add these requirements under temporary aliases while retaining the existing `hyper = "0.14.32"`, `http = "0.2.12"`, and `prometheus_exporter_base` entries until Task 4:

```toml
hyper1        = { package = "hyper", version = "1.11.0", features = ["http1", "server"] }
http1         = { package = "http", version = "1.5.0" }
hyper-util    = { version = "0.1.20", features = ["tokio", "server", "http1"] }
http-body-util = "0.1.5"
bytes         = "1.12.1"
base64        = "0.23.1"
```

Create `src/server.rs` with `hyper1` and `http1` imports and tests using `Request<http_body_util::Empty<Bytes>>`. Add `mod server;` to `main.rs`. Do not alter the existing Hyper 0.14 imports in `main.rs` yet.

- [ ] **Step 2: Write failing routing tests**

Add tests that call the handler directly:

```rust
#[tokio::test]
async fn serves_metrics_for_get() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/metrics")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let response = handle_request(
        Arc::new(ServerOptions {
            addr: "127.0.0.1:0".parse().unwrap(),
            authorization: Authorization::None,
        }),
        request,
        |_| async { Ok::<_, BoxError>("metric 1\n".to_owned()) },
        Arc::new(()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "text/plain; version=0.0.4");
    assert_eq!(response.into_body().collect().await.unwrap().to_bytes(), "metric 1\n");
}
```

Add separate tests asserting:

- `/health` returns `404 NOT_FOUND` and does not invoke the callback.
- `POST /metrics` returns `405 METHOD_NOT_ALLOWED` and does not invoke the callback.
- A callback error returns `500 INTERNAL_SERVER_ERROR` with the error text as body.

Use an `Arc<AtomicBool>` to prove rejected requests do not invoke the callback.

- [ ] **Step 3: Run routing tests and verify RED**

Run:

```bash
cargo test server::tests::
```

Expected: compilation fails because `handle_request`, `ServerOptions`, and `Authorization` are not implemented.

- [ ] **Step 4: Implement response helpers and routing**

Define:

```rust
type BoxError = Box<dyn std::error::Error + Send + Sync>;
type ResponseBody = http_body_util::Full<bytes::Bytes>;

fn response(status: StatusCode, body: impl Into<Bytes>) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(Full::new(body.into()))
        .expect("valid HTTP response")
}
```

Implement `handle_request` with the specified check order. Add the Prometheus content type only to a successful response. Log callback errors at warning level before returning status 500.

- [ ] **Step 5: Verify routing tests are GREEN**

Run:

```bash
cargo test server::tests::serves_metrics_for_get
cargo test server::tests::rejects_unknown_path
cargo test server::tests::rejects_non_get_method
cargo test server::tests::returns_internal_server_error
```

Expected: all four tests pass.

- [ ] **Step 6: Write failing Basic authentication tests**

Use `base64::engine::general_purpose::STANDARD.encode(":secret")`. Add separate tests for:

- missing authorization header returns 401;
- malformed authorization header returns 401;
- non-Basic scheme returns 401;
- invalid base64 returns 401;
- wrong password returns 401;
- `Basic <base64(:secret)>` reaches the callback and returns 200;
- valid authorization on `/unknown` returns 404, proving authorization runs first.

- [ ] **Step 7: Run authentication tests and verify RED**

Run:

```bash
cargo test server::tests::basic_auth
```

Expected: at least the valid credentials test fails because authentication is not implemented; malformed credentials must not panic.

- [ ] **Step 8: Implement Basic authentication**

Use the Base64 engine API:

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};
```

Parse the header with `to_str`, split it into exactly two whitespace-separated tokens, require scheme `Basic`, decode the second token, decode UTF-8, and compare with `format!(":{password}")`. Convert every parsing failure to unauthorized rather than propagating or panicking.

- [ ] **Step 9: Verify all handler tests are GREEN**

Run:

```bash
cargo test server::tests::
```

Expected: all handler tests and all existing tests pass.

---

### Task 3: Hyper 1 TCP Server and Application Integration

**Files:**
- Modify: `src/server.rs`
- Modify: `src/main.rs:1-25,22-25,114-239`
- Modify: `Cargo.toml:21-33`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: `pub(crate) async fn run_server<O, F, Fut>(server_options: ServerOptions, options: O, callback: F) -> std::io::Result<()>`.
- Consumes: `perform_request(options: Arc<Options>) -> Result<String, BoxError>`.
- Consumes: Task 2's `handle_request` and response body type.

- [ ] **Step 1: Write a failing listener-bind test**

Extract listener setup into a small function so binding can be tested without running an infinite accept loop:

```rust
async fn bind_listener(addr: SocketAddr) -> std::io::Result<TcpListener>;
```

Test it with `127.0.0.1:0`, then assert `listener.local_addr().unwrap().ip().is_loopback()` and `port() != 0`.

- [ ] **Step 2: Run the bind test and verify RED**

Run:

```bash
cargo test server::tests::binds_tcp_listener
```

Expected: compilation fails because `bind_listener` does not exist.

- [ ] **Step 3: Implement binding and Hyper connection serving**

Implement `bind_listener` with `TcpListener::bind`. Implement `run_server` as an accept loop that:

1. wraps each `TcpStream` in `hyper_util::rt::TokioIo`;
2. creates `hyper1::service::service_fn`;
3. calls Task 2's `handle_request`;
4. returns `Ok::<_, Infallible>(response)` from the service;
5. serves the connection through `hyper1::server::conn::http1::Builder`;
6. spawns each connection with `tokio::task::spawn`;
7. logs a connection error without terminating the listener.

Use `Arc` for server options and application options, and clone the callback per connection/request.

- [ ] **Step 4: Enable required Tokio features**

Update Tokio to:

```toml
tokio = { version = "1.53.1", features = ["macros", "net", "rt", "sync"] }
```

If compilation reports that Tokio I/O traits are feature-gated, add only the exact required `io-util` feature.

- [ ] **Step 5: Verify server unit tests and compilation**

Run:

```bash
cargo test server::tests::
cargo check
```

Expected: server tests pass and the complete application still compiles with the old server active alongside the new, not-yet-integrated server module.

- [ ] **Step 6: Integrate the server in `main.rs`**

Remove:

```rust
use hyper::{Body, Request};
use prometheus_exporter_base::prelude::{Authorization, ServerOptions};
use prometheus_exporter_base::render_prometheus;
```

Import the internal server API. Change:

```rust
async fn perform_request(
    options: Arc<Options>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
```

Replace `render_prometheus(...)` with:

```rust
run_server(server_options, options, perform_request).await?;
```

Remove `prometheus_exporter_base` from the `RUST_LOG` strings so they contain only this package's log target.

- [ ] **Step 7: Run application tests and verify GREEN**

Run:

```bash
cargo test
cargo check
cargo run -- --help
```

Expected: all tests pass, `cargo check` succeeds, and help output lists the existing CLI options without starting the server.

---

### Task 4: Remove the Old Dependency Graph and Obsolete Errors

**Files:**
- Modify: `Cargo.toml:21-36`
- Modify: `Cargo.lock`
- Modify: `src/exporter_error.rs:24-101`

**Interfaces:**
- Removes: `prometheus_exporter_base`, Hyper 0.14, and HTTP 0.2 dependencies.
- Renames: temporary dependency aliases `hyper1` and `http1` to final names `hyper` and `http`.
- Removes: `ExporterError::Hyper`, `ExporterError::Http`, and their `From` implementations.
- Preserves: all parser-related `ExporterError` variants and conversions.

- [ ] **Step 1: Replace the transitional dependencies with final names**

Delete these old entries:

```toml
hyper = { version = "0.14.32", features = ["stream"] }
http = "0.2.12"
prometheus_exporter_base = { version = "1.4.0", features = ["hyper_server"] }
```

Rename the transitional entries to:

```toml
hyper = { version = "1.11.0", features = ["http1", "server"] }
http = "1.5.0"
```

Update only `src/server.rs` imports from `hyper1` to `hyper` and from `http1` to `http`. Run `cargo check` to regenerate and validate `Cargo.lock`.

- [ ] **Step 2: Verify obsolete error code has no call sites**

Run:

```bash
rg 'ExporterError::(Hyper|Http)|From<(hyper|http)::Error>' src
```

Expected: matches occur only in `src/exporter_error.rs`; no application call site constructs these variants or relies on these conversions.

- [ ] **Step 3: Remove obsolete Hyper and HTTP error variants**

Delete `ExporterError::Hyper`, `ExporterError::Http`, `impl From<hyper::Error>`, and `impl From<http::Error>`. Do not change the remaining parsing, UTF-8, JSON, I/O, or integer error handling.

- [ ] **Step 4: Prove old dependency versions are absent**

Run:

```bash
cargo tree
```

Inspect the output and then run:

```bash
if cargo tree | rg -q 'prometheus_exporter_base|hyper v0\.14|http v0\.2'; then exit 1; fi
```

Expected: exit code 0 and no matching old dependencies.

- [ ] **Step 5: Verify direct versions**

Run:

```bash
cargo tree -i hyper@1.11.0
cargo tree -i http@1.5.0
```

Expected: both trees include `prometheus_wireguard_exporter` and contain no parallel old major version.

---

### Task 5: Final Verification and Review

**Files:**
- Review: `Cargo.toml`
- Review: `Cargo.lock`
- Review: `src/main.rs`
- Review: `src/server.rs`
- Review: `src/prometheus/`
- Review: `src/wireguard.rs`
- Review: `src/exporter_error.rs`

**Interfaces:**
- Verifies all interfaces and constraints established in Tasks 1-4.

- [ ] **Step 1: Format the complete change**

Run:

```bash
cargo fmt
cargo fmt --check
```

Expected: both commands exit successfully and the second produces no diff.

- [ ] **Step 2: Run the full test suite**

Run:

```bash
cargo test --all-features
```

Expected: all unit and compatibility tests pass with zero failures.

- [ ] **Step 3: Run strict Clippy**

Run:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: exit code 0 with no warnings. The existing invalid `clippy` crate dev-dependency warning may require removing `clippy = "0.0.302"` from `[dev-dependencies]`; if so, remove it because Clippy is a Rustup component, regenerate `Cargo.lock`, and rerun this command.

- [ ] **Step 4: Build debug and release artifacts**

Run:

```bash
cargo build
cargo build --release
```

Expected: both builds finish successfully.

- [ ] **Step 5: Check formatting, dependency graph, and worktree diff**

Run:

```bash
git diff --check
if cargo tree | rg -q 'prometheus_exporter_base|hyper v0\.14|http v0\.2'; then exit 1; fi
git status --short
git diff --stat
git diff
```

Expected: whitespace and dependency checks pass. The diff is limited to the files listed by this plan, with no changes to metric reference strings or CLI definitions.

- [ ] **Step 6: Reconcile requirements against the design**

Confirm explicitly in the execution report:

- all metric compatibility tests passed;
- GET `/metrics` returns 200 and the Prometheus content type;
- invalid auth, path, method, and callback errors map to 401, 404, 405, and 500;
- Basic auth retains `:<password>` comparison semantics;
- no outbound HTTP/TLS helper was introduced;
- Hyper 0.14, HTTP 0.2, and `prometheus_exporter_base` are absent;
- only repository-local files were accessed or modified during execution.
