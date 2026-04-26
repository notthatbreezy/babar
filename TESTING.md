# Testing Strategy

Conventions and tooling for the test suite. Per-milestone test requirements
live in `MILESTONES.md`; this document covers what applies across all of them.

## Test categories

We maintain four distinct test categories with different constraints and
lifetimes. Each has a dedicated location.

### 1. Unit tests — `src/**/*.rs` under `#[cfg(test)]`

Pure logic, no network, no filesystem beyond tempfiles. Everything deterministic.

Appropriate for:

- Protocol message encode/decode golden tests.
- State machine transition tests.
- Codec roundtrip logic (not involving a live DB).
- Auth computation (hash, SCRAM proof) against known vectors.

Run in every CI job. Must complete in under 10 seconds total.

### 2. Integration tests — `tests/` using `testcontainers-rs`

Spin up a real Postgres. This is the primary correctness bar.

- Each test module owns its own container lifecycle via a helper.
- Containers are reused within a test run but torn down on exit.
- No shared mutable schema between tests; each test creates a unique schema or
  temp table.
- Container image pinned in `tests/common/mod.rs`; matrix overrides via
  `POSTGRES_VERSION` env var.
- Tests that exercise version-specific behavior gate via
  `#[cfg(feature = "pg17")]` or skip at runtime with a logged reason.

### 3. Property-based tests — `proptest`

Located alongside the code under test. Used for:

- Codec roundtrip invariants (`decode(encode(x)) == x`).
- Fragment composition associativity.
- State machine invariants (e.g., post-transaction state is always `Idle`
  or `Failed`, never something else).

Default case count: 256. CI `--release` run bumps to 2048 for nightly.

### 4. UI / compile-fail tests — `trybuild` in `macros/tests/`

For every macro we ship, exhaustive error-path coverage. Each error message
is checked against a `.stderr` golden file. Updating these requires
`TRYBUILD=overwrite cargo test` and explicit review in the PR.

## Protocol-level testing

For the nasty edge cases a real server won't readily reproduce — specific
error codes at specific positions, malformed responses, network blips — we
build an in-process mock backend using `pgwire`'s server API. This lives in
`tests/common/mock_backend.rs` and exposes:

```rust
pub fn spawn_mock<F>(handler: F) -> MockHandle
    where F: FnOnce(MockConn) -> BoxFuture<'static, ()>;
```

The handler receives a `MockConn` driving the backend side of the protocol
and can send arbitrary messages. Used for:

- Injecting specific `ErrorResponse` messages.
- Simulating a server that stops responding mid-query (cancellation tests).
- Verifying the driver's behavior when pipelined messages arrive out of
  "expected" order.

The mock is **not** used for the main correctness bar — real Postgres is.
The mock is for cases where we specifically need something pathological.

## CI matrix

Every push runs:

| Job | OS | Rust | Postgres | Features |
|---|---|---|---|---|
| stable | ubuntu-latest | stable | 17 | default |
| msrv | ubuntu-latest | MSRV | 17 | default |
| nightly | ubuntu-latest | nightly | 17 | default (allowed to fail) |
| pg-matrix | ubuntu-latest | stable | 14, 15, 16, 17 | default |
| features-none | ubuntu-latest | stable | 17 | `--no-default-features` |
| features-each | ubuntu-latest | stable | 17 | each feature alone |
| features-all | ubuntu-latest | stable | 17 | `--all-features` |
| macos | macos-latest | stable | 17 | default |
| windows | windows-latest | stable | 17 | default |
| clippy | ubuntu-latest | stable | — | `-D warnings` |
| fmt | ubuntu-latest | stable | — | `cargo fmt --check` |
| doc | ubuntu-latest | stable | — | `RUSTDOCFLAGS="-D warnings"` |
| deny | ubuntu-latest | stable | — | `cargo deny check` |
| audit | ubuntu-latest | stable | — | `cargo audit` |
| semver | ubuntu-latest | stable | — | `cargo semver-checks` |

Nightly-only jobs (run on schedule, not every push):

- `proptest` with bumped case counts.
- `miri` over the unit test suite (should be zero unsafe; this enforces that).
- Benchmarks vs `tokio-postgres`, with regression alert on 10%+ degradation.
  Run locally with
  `cargo bench -p prepared-throughput-bench --bench prepared_throughput`.
  The current M2 benchmark starts its own Dockerized Postgres and respects
  `BABAR_PG_IMAGE` if you want a different server version.

## Benchmark discipline

Benches live in `benches/`, use `criterion`. Two categories:

1. **Absolute throughput** — prepared-statement execution rate, cursor
   streaming rate, pool acquire rate.
2. **Comparative** — same workload against `tokio-postgres` for reference.

Regression gate on CI: a benchmark must not regress more than 10% between
merges without an accompanying note in the PR. Initial baseline set at M2.

## Test data and fixtures

- No SQL fixtures checked in as `.sql` files for unit tests. Everything is
  inline in Rust.
- Integration tests may use small `.sql` files in `tests/sql/` for table
  setup, but prefer inline creation.
- No binary fixtures for codec tests — generate test values in Rust.

## Coverage

We don't enforce a coverage percentage — it's a vanity metric and incentivizes
bad tests. We do enforce:

- Every public function has at least one test exercising its happy path.
- Every `match` on an externally-controlled enum (e.g., `BackendMessage`)
  has tests for each arm OR has an explicit `// untested: reason` comment.
- Every unsafe block (we aim for zero) has a safety comment and a test
  justifying it.

`cargo tarpaulin` reports are generated for reference but not gated on.

## Flaky test policy

Zero tolerance. A flaky test is either:

1. Fixed immediately (real race condition found), or
2. Deleted and a GitHub issue filed.

No `#[ignore]` retries, no "run it again in CI." A flaky test is a bug in
the test or the code under test, and we treat it as a P0.

## Test naming

- Unit tests: `fn test_<thing>_<behavior>`, e.g.,
  `test_scram_auth_rfc_vector`.
- Integration tests: file named after the feature area, functions named
  `fn <feature>_<scenario>`, e.g., `fn pool_survives_connection_kill`.
- No `fn test_it_works`. If you can't name what you're testing, you don't
  know what you're testing.

## Pre-merge checklist

Before any PR merges, the author confirms:

- [ ] New or changed public API has rustdoc.
- [ ] New behavior has at least one integration test.
- [ ] New error paths have at least one test that produces them.
- [ ] `cargo fmt`, `cargo clippy -D warnings`, `cargo test --all-features`
      pass locally.
- [ ] `CHANGELOG.md` updated if user-visible.
- [ ] If this completes a milestone, the milestone's acceptance criteria are
      all checked.
