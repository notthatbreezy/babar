# Project Plan

A typed, async Postgres driver for Rust on Tokio, modeled ergonomically on
[Skunk](https://github.com/typelevel/skunk). Speaks the Postgres wire protocol
directly — no JDBC, no libpq, no `tokio-postgres` wrapping.

> Crate name is a placeholder throughout this doc. Pick before M0 ships.

## Vision

Ship a driver where:

- Queries and commands are distinct first-class values, parameterized by their
  input and output types.
- Codecs are explicit values, not resolved via trait lookup. `int4`, `text`,
  `uuid` are things you import, not things that get magicked into scope.
- SQL fragments compose, and the parameter type of the composition is the
  concatenation of the component parameter types.
- Row decoders are validated against the server's `RowDescription` at prepare
  time. Shape mismatches fail before execution with useful diagnostics.
- Errors render with the offending SQL fragment highlighted, the way Skunk's do.
- The driver is boring: correct protocol handling, no surprises on cancellation,
  no surprises on panic, no connections left in weird states.

Out of scope for the project's identity:

- Pretending to be a general-purpose SQL library. This is Postgres only.
- Mandatory or offline compile-time schema verification. Runtime verification at
  prepare time remains the default design choice, with optional online macro
  verification available for the v1 verifiable subset when a database is
  configured at build time.
- Runtime abstraction over async executors. Tokio is the commitment.

## Design principles

1. **Explicit over implicit.** Codecs are values. Types are spelled out. No
   blanket impls doing work behind the user's back.
2. **Do the Tokio thing.** Background driver task owns the socket, users talk to
   it over `mpsc` channels. Cancellation-safe by construction. Standard patterns.
3. **Validate early.** At prepare time, check shapes. At bind time, check
   parameter counts. Push failures as close to the user's code as we can.
4. **Own the state machine.** The protocol has modes (idle, in-transaction,
   in-failed-transaction, in-copy). These are not optional to model correctly.
5. **One way to do common things.** Multiple transaction APIs, multiple
   parameter-binding styles, and so on are anti-goals. Skunk is opinionated;
   so are we.

## Resolved architectural decisions

| # | Decision | Choice | Rationale |
|---|---|---|---|
| 1 | Protocol layer | `postgres-protocol` crate for message framing; we own everything above | Mature message codecs, no imposed session model, matches Skunk's philosophy |
| 2 | Codec derivation | Ship explicit codec composition in M1, add `#[derive(Codec)]` in M5 | Derive is the real user-facing win but not worth blocking the early milestones |
| 3 | Session concurrency | Background task owns the socket; public API sends commands over mpsc | Cancellation-safe, idiomatic Tokio, lets the driver always drive the protocol to a consistent state even if the caller future is dropped |
| 4 | Error model | Rich errors with SQLSTATE, PG fields, SQL origin, caret-rendered SQL | One of Skunk's strongest ergonomic wins; cheap to port |
| 5 | Async executor | Tokio only | User requirement; avoids HKT-emulation pain |
| 6 | TLS backend | `rustls` primary, `native-tls` as alt feature | Modern default, pure-Rust |
| 7 | Pipelining | Internal only in v0.1; no user-facing batch API | Real throughput win but significant state-machine complexity |

## Open decisions (resolve before the relevant milestone)

| # | Decision | Blocks | Notes |
|---|---|---|---|
| A | Crate name | Everything | Pick before M0 publishes |
| B | Derive-macro shape for `#[derive(Codec)]` | M5 | **Resolved:** field order is column order, with inference-first defaults and `#[pg(codec = "...")]` overrides |
| C | Pool implementation: custom vs `deadpool` | M4 | `deadpool` is fine for most, but we want statement-cache awareness |
| D | `sql!` macro form: positional `{}` vs named `$name` | M3 | **Resolved:** named `$name` placeholders shipped in M3 |
| E | Public name for `Void` | M1 | `Void`, `NoParams`, `()`? |
| F | MSRV | M6 | Probably Rust 1.75+ for async-trait-in-trait |

## Crate structure

```
<crate>/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── core/                   # main library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session/
│   │       │   ├── mod.rs      # Session public API
│   │       │   ├── driver.rs   # background task
│   │       │   ├── startup.rs  # startup + auth
│   │       │   └── state.rs    # protocol state machine
│   │       ├── protocol/       # thin wrappers over postgres-protocol
│   │       │   ├── frontend.rs
│   │       │   └── backend.rs
│   │       ├── codec/
│   │       │   ├── mod.rs      # Encoder, Decoder, Codec traits
│   │       │   ├── primitive.rs
│   │       │   └── combinators.rs
│   │       ├── types/          # PG OIDs, type metadata
│   │       ├── query/
│   │       │   ├── fragment.rs # Fragment<A>
│   │       │   ├── query.rs    # Query<A, B>, Command<A>
│   │       │   └── prepared.rs # PreparedQuery, PreparedCommand
│   │       ├── transaction.rs
│   │       ├── pool.rs
│   │       ├── error.rs
│   │       ├── tracing.rs
│   │       └── tls.rs          # feature-gated
│   └── macros/                 # proc-macro crate
│       ├── Cargo.toml
│       └── src/lib.rs          # sql!, derive(Codec)
├── tests/                      # integration tests, testcontainer-based
├── examples/
└── benches/
```

Workspace, not single crate, because proc macros must live in their own crate.
`<crate>` re-exports `<crate>_macros::{sql, Codec}` so users never depend on the
macro crate directly.

## Core type surface

```rust
// Session
pub struct Session { /* handle to driver task */ }
pub struct CopyIn<T> { /* typed binary COPY FROM STDIN bulk ingest */ }

impl Session {
    pub async fn connect(config: Config) -> Result<Self>;
    pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64>;
    pub async fn copy_in<T, I>(&self, copy: &CopyIn<T>, rows: I) -> Result<u64>
        where I: IntoIterator<Item = T>;
    pub async fn stream<A, B>(&self, q: &Query<A, B>, args: A)
        -> Result<impl Stream<Item = Result<B>>>;
    pub async fn prepare<A, B>(&self, q: &Query<A, B>) -> Result<PreparedQuery<A, B>>;
    pub async fn transaction<T, F>(&self, f: F) -> Result<T>
        where F: for<'t> FnOnce(&'t Transaction<'t>) -> BoxFuture<'t, Result<T>>;
    pub async fn close(self) -> Result<()>;
}

// Queries and commands
pub struct Query<A, B>   { /* Fragment<A>, decoder for B */ }
pub struct Command<A>    { /* Fragment<A> */ }
pub struct Fragment<A>   { /* SQL pieces + encoder for A */ }

pub struct Void;  // zero-param marker

// Codecs
pub trait Encoder<A> {
    fn encode(&self, value: &A, buf: &mut BytesMut) -> Result<()>;
    fn oids(&self) -> &[Oid];
}
pub trait Decoder<A> {
    fn decode(&self, row: &Row) -> Result<A>;
    fn oids(&self) -> &[Oid];
}
pub trait Codec<A>: Encoder<A> + Decoder<A> { }

// Primitive codecs
pub const int4: Int4Codec;    // Codec<i32>
pub const int8: Int8Codec;    // Codec<i64>
pub const text: TextCodec;    // Codec<String>
// ...

// Transactions
pub struct Transaction<'s> { /* borrows &'s Session */ }
pub struct Savepoint<'s> { /* borrows &'s Session */ }
impl Transaction<'_> {
    pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64>;
    pub async fn savepoint<T, F>(&self, f: F) -> Result<T> where /* passes Savepoint<'_> */;
    // commit/rollback driven by transaction() closure's Result
}
impl Savepoint<'_> {
    pub async fn execute<A>(&self, cmd: &Command<A>, args: A) -> Result<u64>;
    pub async fn savepoint<T, F>(&self, f: F) -> Result<T> where /* passes Savepoint<'_> */;
}
```

## Testing philosophy

Detailed strategy in `TESTING.md`. Headline points:

- Real Postgres in integration tests via `testcontainers-rs`. No mocks of the
  server protocol for integration suites.
- Property-based codec roundtrips via `proptest`.
- `trybuild` UI tests for macro diagnostics.
- An in-process mock backend (we can build one on top of `pgwire`'s server API)
  for protocol-level unit tests that need to force edge cases the real server
  won't reproduce.
- CI matrix: Postgres 14, 15, 16, 17.
- `cargo-deny`, `cargo-audit`, clippy pedantic on warnings as errors, MSRV check.
- Every milestone ships with its own acceptance-test suite; see `MILESTONES.md`.

## Milestone summary

Detail and acceptance criteria in `MILESTONES.md`.

| Milestone | Theme | Weeks |
|---|---|---|
| M0 | Protocol foundation + auth | 1–2 |
| M1 | Core type surface + primitive codecs (text format) | 3–4 |
| M2 | Extended protocol + binary format + prepared statements | 5–7 |
| M3 | SQL macros + optional verification | 8–9 |
| M4 | Transactions + connection pool | 10–11 |
| M5 | Expanded type coverage + `#[derive(Codec)]` | 12–15 |
| M6 | TLS, observability, error rendering polish, v0.1 release | 16–17 |

Deferred past v0.1: LISTEN/NOTIFY channels, remaining COPY protocol work,
out-of-band cancellation, logical replication, broader spatial coverage
(`GeometryCollection`, Z/M coordinates, PostgreSQL built-in geometric types),
and generic extension-backed range/multirange families.

Current COPY support is intentionally narrower: typed binary `COPY FROM STDIN`
bulk ingest is in scope, while `COPY TO`, text/CSV COPY modes, and other COPY
protocol variants remain deferred.
