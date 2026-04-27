# Changelog

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
