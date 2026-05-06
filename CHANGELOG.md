# Changelog

## 0.2.0

### typed SQL surface
- public `typed_query!` API shipped on `babar` and `babar-macros`
- optional omission syntax now lowers through the runtime SQL/bind pipeline
- authored schema declarations and external-schema resolution for typed SQL macros
- same table names can now coexist across SQL schemas
- typed macros now support struct-shaped results
- typed SQL APIs are unified around the current public query surface

### examples and documentation
- Axum typed-query example coverage
- typed-query API documentation and release-facing README refresh
- Rust learning documentation track and GitHub Pages CNAME publication support

### release and CI follow-ups
- Rust 1.88 UI-fixture split and related CI fixes
- clippy and struct-support follow-up fixes after the typed SQL milestone landed

## 0.1.0

### Documentation
- mdbook documentation site rewritten as *The Book of Babar* with Diataxis-aligned hierarchy (Get Started, Book, Reference, Explanation), conversational doobie-style voice, inline `// type: T` annotations, and new explanation pages (`why-babar`, `what-makes-babar-babar`, `design-principles`, `driver-task`, `comparisons`, `roadmap`)

### protocol foundation
- background driver task and cancellation-safe `Session`
- startup/auth for cleartext, MD5, and SCRAM-SHA-256
- simple-query protocol support and raw result-set access

### core typed surface
- `Query`, `Command`, `Fragment`, and primitive text codecs
- typed `Session::execute`, `Session::query`, and row streaming surface
- column-alignment validation and richer public docs

### prepared statements and extended protocol polish
- prepare/describe/execute lifecycle with statement caching
- schema validation at prepare time
- portal-backed streaming and benchmark scaffolding

### `sql!` macro
- named placeholder syntax (`$name`)
- nested fragment composition
- SQL origin capture via `query::Origin`

### transactions and pooling
- scoped transactions and nested savepoints
- connection pool with health checks, lifetimes, and pooled prepared/query helpers
- example applications for transactions and pooling

### expanded codecs and derive support
- optional codecs for uuid/time/chrono/json/numeric/net/interval/array/range
- `#[derive(Codec)]`
- M5 integration tests and examples

### release polish
- TLS configuration and transport support (`rustls` default, `native-tls` alternate)
- OpenTelemetry-friendly tracing spans for connect/prepare/execute/transaction paths
- rich SQL-aware error rendering with SQLSTATE metadata and caret output
- README overhaul, release runbook, CI workflow, and additional real-world examples
