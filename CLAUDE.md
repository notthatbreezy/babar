# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Babar is a typed, async PostgreSQL driver for Rust on Tokio that speaks the Postgres wire protocol directly — no libpq, no JDBC, no `tokio-postgres`. PLAN.md, MILESTONES.md, and TESTING.md remain the authoritative design/reference documents for the current implementation.

## Commands

```bash
cargo fmt --check
cargo clippy -D warnings
cargo test --all-features
cargo audit
cargo deny check
cargo semver-checks
RUSTDOCFLAGS="-D warnings" cargo doc
cargo tarpaulin          # coverage — informational, not gated
cargo msrv               # MSRV enforcement (Rust 1.88+ for edition2024 transitive deps)
```

Integration tests use hand-rolled Docker containers (including a PostGIS image when needed) across Postgres 14–17 scenarios. Property-based tests use `proptest` (256 cases by default, 2048 on nightly). UI/macro tests use `trybuild` with golden `.stderr` files in `macros/tests/`.

## Architecture

The driver follows a background-task model: a single Tokio task owns the TCP socket and serializes all wire-protocol I/O. User-facing `Session` is a lightweight handle that sends commands over an `mpsc` channel and receives results via per-command `oneshot` channels. This makes all public API calls cancellation-safe.

```
User Code → Session (mpsc handle)
                ↓
        Background Driver Task  (owns TcpStream, state machine)
                ↓
        Postgres Server (wire protocol)
```

### Planned workspace layout

```
crates/
  core/src/
    session/          # mod.rs (public API), driver.rs, startup.rs (auth), state.rs
    protocol/         # thin wrappers around postgres-protocol crate
    codec/            # Encoder/Decoder/Codec traits + primitive impls
    query/            # Fragment<A>, Query<A,B>, Command<A>, PreparedQuery
    types/            # Postgres OID constants + metadata
    transaction.rs
    pool.rs           # deadpool-based connection pool
    error.rs          # rich Error with SQLSTATE, SQL origin, caret rendering
    tracing.rs
    tls.rs            # feature-gated: rustls (primary), native-tls
  macros/             # sql! + #[derive(Codec)] proc-macro crate
tests/                # integration tests (testcontainers)
examples/
benches/
```

### Core type surface

```rust
pub struct Session { /* mpsc handle */ }
pub struct CopyIn<T>  { /* typed binary COPY FROM STDIN bulk ingest */ }
pub struct Query<A, B>  { /* Fragment<A>, decoder B */ }
pub struct Command<A>   { /* Fragment<A> */ }
pub struct Fragment<A>  { /* SQL pieces + encoder A */ }
pub struct Void;        // zero-param marker (name TBD — see Open Decisions)

pub trait Encoder<A> { fn encode(&self, &A, &mut BytesMut) -> Result<()>; }
pub trait Decoder<A> { fn decode(&self, &Row) -> Result<A>; }
pub trait Codec<A>: Encoder<A> + Decoder<A> {}

// Codec values are const singletons imported explicitly by callers
pub const int4: Int4Codec;
pub const text: TextCodec;
```

### Key design constraints

- **No unsafe code** — enforced by Miri in CI.
- **Validate early** — schema mismatch detected at prepare time, parameter count at bind time.
- **Compile-time verification is opt-in** — `query!` / `command!` can verify against a live database during macro expansion when `BABAR_DATABASE_URL` or `DATABASE_URL` is set; `sql!` only best-effort verifies supported binding codecs.
- **Binary format in M2** — text format ships in M1 as a stepping stone.
- **Explicit codec composition** — callers name their codecs; no magic inference.
- **One way to do things** — no multiple transaction styles, no generic abstraction over databases.

### Protocol implementation notes

- Message framing via the `postgres-protocol` crate.
- Authentication: cleartext, MD5, SCRAM-SHA-256 (SCRAM-SHA-256-PLUS deferred post-v0.1).
- Protocol state machine has four explicit states: Idle, InTransaction, InFailedTransaction, InCopy.
- COPY support is intentionally limited to typed binary `COPY FROM STDIN` bulk ingest via `CopyIn<T>` / `Session::copy_in`.
- Internal pipelining (batch multiple round-trips) lands in M2.
- Pool wraps `deadpool` traits with statement-cache awareness (M4).

## Milestone summary

| M | Theme | Key deliverable |
|---|-------|-----------------|
| M0 | Protocol foundation | Driver task, auth (SCRAM/MD5/cleartext), simple query |
| M1 | Core types + text codecs | `Session`, `Query`, `Command`, `Fragment`, 9 primitive codecs |
| M2 | Extended protocol + binary | `PreparedQuery`, schema check at prepare time, binary codecs, pipelining |
| M3 | `sql!` macro | Fragment composition, placeholder syntax, SQL origin tracking |
| M4 | Transactions + pool | `Transaction<'s>`, savepoints, deadpool integration |
| M5 | Expanded types + derive | uuid/time/chrono/json/numeric/net/interval/array/range/multirange plus postgis/pgvector/text-search/macaddr/bits/hstore/citext codec modules, `#[derive(Codec)]` |
| M6 | TLS + observability + release | rustls/native-tls, tracing spans, error polish, v0.1 |

## Resolved decisions

- **A** (M0): Crate name — `babar`.
- **D** (M3): `sql!` placeholder syntax — named `$name` placeholders.
- **E** (M1): Zero-param type — Rust's unit type `()`. `Query<(), B>` reads
  as "no parameters"; no new public vocabulary to teach. Skunk's `Void`
  exists to dodge Scala's bulky `Unit`; Rust doesn't have that problem.
- **F** (M0): MSRV — Rust 1.88 (edition2024 stabilization; required by transitive deps in the `toml` family).

## Open decisions (resolve before the indicated milestone)

- **C** (M4): Pool implementation — custom vs deadpool?

## Testing policy

- Zero tolerance for flaky tests — fix immediately or delete and file an issue.
- Integration test containers are reused across test cases (Postgres 14–17 matrix).
- Every new behavior requires integration tests; every error path requires a test.
- Benchmark baseline vs `tokio-postgres` set at M2; 10%+ regression is a gate.
- Pre-merge checklist: `cargo fmt`, `cargo clippy -D warnings`, rustdoc, integration test coverage for new behavior.

## Deferred post-v0.1

LISTEN/NOTIFY, broader COPY coverage (`COPY TO`, text/CSV COPY, replication-style modes), out-of-band cancellation, logical replication, offline cache / broader compile-time verification coverage, SCRAM-SHA-256-PLUS.
