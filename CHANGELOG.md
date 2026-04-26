# Changelog

## 0.1.0

### M0 — protocol foundation
- background driver task and cancellation-safe `Session`
- startup/auth for cleartext, MD5, and SCRAM-SHA-256
- simple-query protocol support and raw result-set access

### M1 — core typed surface
- `Query`, `Command`, `Fragment`, and primitive text codecs
- typed `Session::execute`, `Session::query`, and row streaming surface
- column-alignment validation and richer public docs

### M2 — prepared statements and extended protocol polish
- prepare/describe/execute lifecycle with statement caching
- schema validation at prepare time
- portal-backed streaming and benchmark scaffolding

### M3 — `sql!` macro
- named placeholder syntax (`$name`)
- nested fragment composition
- SQL origin capture via `query::Origin`

### M4 — transactions and pooling
- scoped transactions and nested savepoints
- connection pool with health checks, lifetimes, and pooled prepared/query helpers
- example applications for transactions and pooling

### M5 — expanded codecs and derive support
- optional codecs for uuid/time/chrono/json/numeric/net/interval/array/range
- `#[derive(Codec)]`
- M5 integration tests and examples

### M6 — release polish
- TLS configuration and transport support (`rustls` default, `native-tls` alternate)
- OpenTelemetry-friendly tracing spans for connect/prepare/execute/transaction paths
- rich SQL-aware error rendering with SQLSTATE metadata and caret output
- README overhaul, release runbook, CI workflow, and additional real-world examples
