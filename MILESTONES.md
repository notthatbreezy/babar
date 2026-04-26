# Milestones

Each milestone lists its scope, deliverables, test requirements, and
acceptance criteria. A milestone is **not done** until every acceptance item
is checked and the test suite specified here is passing in CI against all
supported Postgres versions.

---

## M0 — Protocol foundation and authentication

**Duration:** ~2 weeks
**Depends on:** nothing

### Scope

Stand up the crate, the driver task architecture, and enough protocol work to
connect, authenticate, and run a trivial simple-query roundtrip. No public
codec API yet — raw bytes out.

### Deliverables

- Workspace with `core/` and `macros/` crates. `macros/` is empty but compiles.
- `postgres-protocol` dependency wired in for message framing.
- `Session::connect(Config)` returns a `Session` holding an mpsc handle to a
  background driver task.
- Driver task owns the `TcpStream`, serializes all frontend writes, demuxes
  backend messages, and responds on oneshot reply channels.
- Startup: protocol version negotiation, startup-parameters, parameter-status
  handling.
- Authentication: cleartext, MD5, SCRAM-SHA-256 (SCRAM-SHA-256-PLUS deferred).
- Simple query protocol: `Session::simple_query_raw(&str)` returns rows as
  `Vec<Vec<Option<Bytes>>>`. This is an internal API; not public stable.
- Clean shutdown: `Session::close()` sends `Terminate`, awaits task exit.
- Panic/drop safety: if the `Session` handle is dropped, the task terminates
  cleanly and closes the connection.

### Tests

**Unit tests (no network):**

- Startup message encoding golden tests (bytes compared to known-good captures).
- SCRAM-SHA-256 client proof computation against RFC 7677 test vectors.
- MD5 auth hash computation against known values.

**Integration tests (testcontainers):**

- Connect to PG 14, 15, 16, 17; run `SELECT 1`; verify row `["1"]`.
- Connect with wrong password; verify `Error::Auth` is returned and task exits cleanly.
- Connect with cleartext auth enabled server; verify success.
- Connect with MD5 auth enabled server; verify success.
- Connect with SCRAM auth (default modern PG); verify success.
- Send `SELECT 1; SELECT 2;` simple query; verify two row sets returned in order.
- Drop `Session` mid-query; verify driver task terminates and no panics occur.
- Spawn 100 concurrent `simple_query_raw` calls on one session; verify all
  succeed and responses are correctly demuxed.

### Acceptance criteria

- [ ] All unit tests pass.
- [ ] Integration tests pass on PG 14, 15, 16, 17 in CI.
- [ ] `cargo clippy --all-targets -- -D warnings` is clean.
- [ ] Driver task is documented: lifecycle, message flow, error handling.
- [ ] A loom-based test covers the driver task's happy path for concurrent
      requests (shutdown-during-request invariants specifically).
- [ ] Demo: `examples/m0_smoke.rs` connects, runs `SELECT 1`, prints result,
      exits with code 0.

---

## M1 — Core type surface and primitive codecs

**Duration:** ~2 weeks
**Depends on:** M0

### Scope

Introduce the public type vocabulary — `Session`, `Query`, `Command`,
`Fragment`, `Void`, `Codec`, `Encoder`, `Decoder` — and primitive codecs for
the most common scalar types, in **text format only**. No prepared statements
yet; everything goes through simple-query or an initial straight-line extended
query flow.

### Deliverables

- `Fragment<A>` with builder API: `Fragment::lit(sql)`, `.bind(codec)`,
  `.plus(fragment)`.
- `Query<A, B>`, `Command<A>`, `Void` types.
- `Encoder<A>`, `Decoder<A>`, `Codec<A>` traits.
- Primitive codecs: `int2`, `int4`, `int8`, `float4`, `float8`, `bool`, `text`,
  `varchar`, `bpchar`, `bytea`, `nullable(codec)` combinator.
- Codec combinators: `product` (pair of codecs), `imap` (isomorphism),
  tuple composition for arities 1–16.
- `Session::execute`, `Session::stream` public API (text format on top of
  extended protocol for parameterized queries; simple protocol for no-param).
- Row decoder validation: `ColumnAlignmentError` when the decoder's declared
  width doesn't match `RowDescription`.

### Tests

**Unit tests:**

- Each primitive codec's `encode`/`decode` roundtrip on representative values,
  including boundary values (`i32::MIN`, empty string, negative timestamps
  where applicable).
- `ColumnAlignmentError` when decoder arity ≠ `RowDescription` columns.
- Tuple codec composition: `(int4, text, bool).into_codec()` decodes correctly.
- `nullable` combinator handles `NULL` and present values.

**Integration tests:**

- For each primitive codec: `CREATE TABLE t (x <pgtype>); INSERT; SELECT`
  roundtrip recovers identical Rust value.
- `proptest` roundtrip for each numeric and string codec over 1000 cases.
- Multi-column `SELECT a, b, c` decodes into a tuple.
- `Fragment` composition produces identical wire bytes as a hand-written query
  (golden test).
- `Command<A>` with parameter returns `u64` affected-row count.

### Acceptance criteria

- [ ] All M0 acceptance preserved (no regression).
- [ ] Every primitive codec has a passing proptest roundtrip.
- [ ] `examples/quickstart.rs` demonstrates: connect, create table, insert 3
      rows with parameters, select them back, print.
- [ ] Public API reviewed against Skunk's `Session`/`Query`/`Command` surface;
      divergences documented with reason.
- [ ] rustdoc builds without warnings; every public item has a doc comment.

---

## M2 — Extended protocol, binary format, prepared statements

**Duration:** ~3 weeks
**Depends on:** M1

### Scope

This is the milestone where performance and correctness both jump. Implement
the real extended query protocol with prepared statements, add binary format
for all primitive codecs, and ship cursor/portal streaming.

### Deliverables

- Extended query state machine: `Parse`, `Bind`, `Describe`, `Execute`, `Sync`.
  Named and unnamed statements.
- `PreparedQuery<A, B>`, `PreparedCommand<A>` values owned by a `Session`,
  with `Drop` issuing `DEALLOCATE`.
- Per-session prepared statement cache keyed by SQL hash + codec OIDs.
- Binary format for every M1 primitive codec. Codecs advertise which formats
  they support; driver picks binary when available.
- **Schema check at prepare time:** after `ParseComplete` + `RowDescription`,
  verify that the decoder's expected OIDs match server OIDs. Mismatch
  produces `SchemaMismatchError` with expected vs actual OIDs and column
  names, before any execute.
- `Session::stream` backed by a portal; consumer backpressure via
  `Execute(row_count)` batches.
- Internal pipelining: `Parse + Describe + Sync` sent together, not sequentially
  awaited.

### Tests

**Unit tests:**

- Extended protocol state machine: fuzz transitions, verify illegal transitions
  produce `ProtocolError`.
- Binary format roundtrip for each primitive.
- Statement cache eviction policy.

**Integration tests:**

- Prepare once, execute 1000× with varying args; verify a single `Parse`
  on the wire (use a protocol-tap test harness).
- Stream 10,000 rows with the consumer sleeping between pulls; verify memory
  stays bounded (no buffering beyond one `Execute` batch).
- Schema mismatch test: declare `Query<Void, i32>` for a `SELECT text_column`;
  verify `SchemaMismatchError` fires at prepare, not execute.
- `DEALLOCATE` on `PreparedQuery::drop` verified via `pg_prepared_statements`.
- Concurrent prepared queries on one session: verify no response cross-talk.
- Text vs binary format parity: same value encoded both ways decodes identically.

### Acceptance criteria

- [ ] All M1 tests still pass.
- [ ] Schema check fires *before* execute in the mismatch case.
- [ ] Streaming 10M rows completes in bounded memory (measured, not just
      assumed — add an integration test with a memory ceiling assertion).
- [ ] Benchmark vs `tokio-postgres` for prepared-statement throughput: our
      numbers are within 20% either way (either is acceptable; we want to know).
- [ ] `crates/core/examples/prepared_and_stream.rs` demonstrates prepared statements and streaming together.
- [ ] Public rustdoc explains the prepared-statement lifecycle and portal streaming usage clearly.

---

## M3 — The `sql!` macro

**Duration:** ~2 weeks
**Depends on:** M2

### Scope

A declarative-style `sql!` macro that produces a `Fragment` with the correct
inferred parameter type, delegating to the M1 builder API under the hood.
For v0.1 this milestone adopts named placeholders: `sql!("SELECT ... WHERE x = $x", x = int4)`.

### Deliverables

- `macros/` crate exports `sql!`.
- `sql!(literal, name = codec_expr, ...)` produces `Fragment<A>` where `A` is
  the flat tuple type of the bound codecs' value types.
- Source-span preservation: compile errors in the macro point at the user's
  call site, not inside macro internals.
- Nested fragments: `sql!("... ($filter)", filter = existing_fragment)` composes.
- Origin tracking: each `Fragment` carries a `(file, line, column)` captured
  at macro-expansion time, used by error-rendering in M6.

### Tests

**Unit tests:**

- `trybuild` UI tests:
    - missing / duplicate / unused `$name` bindings → compile error pointing at
      user's call.
    - codec expression that doesn't satisfy the fragment/codec requirements →
      compile error.
    - correct usage compiles and produces a `Fragment<T>` of the expected type.
- Doc tests on every macro invocation example.

**Integration tests:**

- `sql!("SELECT $id::int4", id = int4)` roundtrip matches the builder-API
  equivalent byte for byte.
- A fragment built with `sql!` composes with a builder-built fragment.
- Macro-built query executes correctly against a real database.

### Acceptance criteria

- [ ] All prior tests still pass.
- [ ] Every `trybuild` test has a stable, reviewed error message.
- [ ] `examples/quickstart.rs` is rewritten to use `sql!` throughout.
- [ ] Documentation explains what the macro does *and* what it doesn't
      (specifically: no compile-time schema check, no SQL parsing).

---

## M4 — Transactions and connection pooling

**Duration:** ~2 weeks
**Depends on:** M2 (macro not required)

### Scope

Scoped transaction API and a production-quality connection pool. The pool must
interact correctly with the per-session prepared-statement cache from M2.

### Deliverables

- `session.transaction(|tx| async move { ... }).await`:
    - Commits on `Ok`, rolls back on `Err` or dropped future.
    - `&Transaction` exposes the same query/command API as `Session`.
- Savepoints: `tx.savepoint(|sp| async move { ... }).await`, nestable.
- `Pool` with configurable `min_idle`, `max_size`, `acquire_timeout`,
  `idle_timeout`, `max_lifetime`.
- Health check on acquire (configurable: none, ping, reset-query).
- Pool-aware statement cache: cached statements live on the connection and
  survive checkout/return; invalidated (and connection recycled) if an error
  indicates server-side state corruption.
- `PoolError` taxonomy: `Timeout`, `PoolClosed`, `AcquireFailed(inner)`.

**Decision:** start with a thin wrapper over `deadpool` traits for the pool
core (open decision C). Reassess if statement-cache semantics conflict.

### Tests

**Unit tests:**

- Transaction commit/rollback path decisions: `Ok → COMMIT`, `Err → ROLLBACK`,
  `panic → ROLLBACK`, dropped future → `ROLLBACK`.
- Savepoint state machine.
- Pool config validation.

**Integration tests:**

- 100 concurrent tasks, pool size 10: all complete, no deadlock, total time
  bounded.
- Kill a Postgres connection mid-pool (close the TCP from the DB side); next
  `acquire()` returns a healthy connection.
- Transaction panic: connection returns to pool in a clean state (verified by
  `SELECT txid_current()` on next acquire — no in-progress transaction).
- Nested savepoints: 3 levels deep, middle one rolls back, outer commits;
  verify final table state.
- Prepared statement survives checkout → return → checkout (same SQL hashes
  to cached statement name).
- Connection error during transaction: connection is evicted from pool, not
  returned.

### Acceptance criteria

- [ ] All prior tests still pass.
- [ ] Transaction API cannot leak a live `Transaction<'_>` out of the closure
      (enforced by lifetime).
- [ ] Pool under concurrent load matches or beats `bb8`+`tokio-postgres` on
      throughput in `benches/pool_throughput.rs`.
- [ ] `examples/transactions.rs` and `examples/pool.rs` demonstrate both.

---

## M5 — Expanded type coverage and `#[derive(Codec)]`

**Duration:** ~4 weeks
**Depends on:** M2, M3

### Scope

Broaden codec coverage to cover most real-world use cases, all feature-gated.
Ship the `Codec` derive macro.

### Deliverables

Feature-gated modules, each its own Cargo feature:

- `uuid`: `uuid::Uuid` codec.
- `time`: `time::OffsetDateTime`, `time::Date`, `time::Time`, `time::PrimitiveDateTime`.
- `chrono`: same set via `chrono`. Mutually compatible with `time`.
- `json`: `serde_json::Value` for `json` and `jsonb`, plus a `typed_json<T>`
  codec for `T: Serialize + DeserializeOwned`.
- `numeric`: `rust_decimal::Decimal` codec for `numeric`.
- `net`: `std::net::IpAddr` for `inet`/`cidr`.
- `interval`: custom `Interval` type with month/day/microsecond split.
- `array`: 1-D and N-D array codec, `array(inner_codec)` combinator.
- `range`: range type codec, `range(inner_codec)` combinator.

Proc macro:

- `#[derive(Codec)]` on structs. Field order is column order. Common
  unambiguous Rust field types infer their default codecs automatically;
  `#[pg(codec = "int4")]` (or similar) remains available as an explicit
  override for unsupported or intentionally different mappings.
- `#[derive(Codec)]` generates code equivalent to
  `imap((f1_codec, f2_codec, ...).into_codec(), |tup| Struct{...},
  |s| (s.f1, s.f2, ...))`.

### Tests

- Per-feature integration test with `CREATE TABLE` using the PG type and
  roundtripping a representative set of values.
- `proptest` roundtrip for all numeric and temporal types.
- Feature-matrix CI job: `--no-default-features`, each feature alone,
  all features.
- Array roundtrip: 1-D int array of 1000 elements, 2-D 100×100 text array.
- `#[derive(Codec)]` on a 5-field struct: insert + select roundtrip.
- `trybuild` tests for derive error messages (missing attribute, wrong field
  type, etc.).

### Acceptance criteria

- [ ] All prior tests still pass.
- [ ] `cargo build --no-default-features` compiles a minimal core driver.
- [ ] Each feature compiles independently.
- [ ] `examples/derive_codec.rs` demonstrates struct mapping.
- [ ] Feature compatibility matrix documented in `README.md`.

---

## M6 — TLS, observability, error polish, v0.1 release

**Duration:** ~2 weeks
**Depends on:** M5

### Scope

Everything a serious user will check before adopting: TLS, tracing, error
quality, docs, CI hygiene, release engineering.

### Deliverables

- TLS: `rustls` feature with certificate verification, SNI, SCRAM-SHA-256-PLUS
  channel binding. `native-tls` as alt feature.
- `tracing` spans: `db.connect`, `db.prepare`, `db.execute`, `db.transaction`,
  with OpenTelemetry-compatible attributes (`db.system = "postgresql"`,
  `db.statement`, `db.operation`).
- Error rendering: `impl Display for Error` produces Skunk-style output with
  the SQL fragment, a caret pointing at the offending column, and the PG
  error fields (`SQLSTATE`, severity, detail, hint, position).
- rustdoc: every public item documented, crate-level docs include a tour,
  every example is a doc test.
- `README.md`: feature matrix, quick start, comparison to `sqlx` and
  `tokio-postgres` (honest — acknowledge what each does better).
- CI: `cargo-deny`, `cargo-audit`, `cargo-msrv` check, `miri` for unsafe blocks
  (should be zero), `cargo-semver-checks` for the public API.

### Tests

- TLS integration test: connect to a TLS-only PG with a self-signed cert via
  `testcontainers` configured appropriately.
- SCRAM-SHA-256-PLUS with channel binding integration test.
- Golden-file tests for error rendering covering: schema mismatch, SQLSTATE
  23505 (unique violation), parse error at position, connection closed mid-query.
- Tracing spans emit expected attributes (use `tracing-test`).
- `cargo-semver-checks` baseline established.

### Acceptance criteria

- [ ] `cargo publish --dry-run` passes.
- [ ] docs.rs build is green.
- [ ] MSRV documented and enforced in CI.
- [ ] Changelog covers every milestone.
- [ ] At least two "real-world" example apps: a CLI tool and a small Axum
      web service using the crate.
- [ ] Release checklist runbook written.
- [ ] Tagged `v0.1.0`.

---

## Deferred (post-v0.1)

Not worked on until v0.1 is shipped and has live users. Each is its own
future milestone:

- **LISTEN/NOTIFY channels** as a `Stream` of notifications.
- **Remaining COPY protocol work** beyond the shipped typed binary `COPY FROM STDIN` ingest surface (notably `COPY TO`, text/CSV COPY, and export helpers).
- **Out-of-band query cancellation** via the side-channel connection.
- **Logical replication client.**
- **Richer SQL proc-macro ergonomics** beyond the shipped `$name` placeholder
  syntax (for example, more Skunk-like interpolation forms).
- **Compile-time schema verification** (sqlx-macro style) — remains out of
  scope per the project's stated philosophy, but could be a companion crate.
