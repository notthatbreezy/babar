---
date: 2026-04-27T00:00:31-04:00
git_commit: e9a14459f8811dfd89ebe44f8ebb653ac4fb3af0
branch: feature/mdbook-docs-rewrite
repository: notthatbreezy/babar
topic: "babar API & docs surface — source-of-truth for mdbook chapter rewrite"
tags: [research, codebase, babar, docs, mdbook]
status: complete
last_updated: 2026-04-27
---

# Research: babar API & Docs Surface for mdbook Rewrite

## Research Question

Document everything chapters in the new mdbook site need to cite accurately:
public API surface, codecs, configuration, examples, project context, and the
current state of `docs/`, `images/`, and CI. The implementation will draft full
prose chapters; this artifact is the technical source-of-truth those chapters
draw from.

## Summary

babar is a single user-facing crate (`babar`, at `crates/core/`) plus a
proc-macro crate (`babar-macros`, re-exported from `babar`). The public surface
is exactly what `crates/core/src/lib.rs` re-exports: `Config`, `Session`,
`Pool`, `Transaction`/`Savepoint`, `CopyIn`, `Migrator`, `Error`/`Result`,
the codec module, `Query`/`Command`/`Fragment`, and the macros `sql!`,
`query!`, `command!`, `#[derive(Codec)]`. There is no env-var / DSN parsing —
`Config` is built by struct methods only. TLS is feature-gated (`rustls`
default, optional `native-tls`). Tracing spans live in `crates/core/src/telemetry.rs`.
Migration support is library-first; the CLI is an *example* (`migration_cli.rs`).
The `docs/` directory is currently a 4-line `SUMMARY.md` plus one 1162-line
tutorial; brand images live outside the book at repo-root `images/` with
whitespace filenames and Windows `:Zone.Identifier` sidecars. CI builds the
mdbook site via `.github/workflows/pages.yml`; `mdbook test` is **not** wired up.

**Gaps that affect chapter coverage** (each calls for "roadmap / not yet
implemented" framing in the chapter, never fabrication):

- No DSN / connection-URL parsing or `PG*` env-var reading inside `Config` —
  the examples handle env vars by hand.
- No `COPY TO`, no text/CSV COPY mode, no `LISTEN/NOTIFY`, no out-of-band
  cancel — explicitly deferred (`crates/core/src/copy.rs:1-19`,
  `crates/core/src/lib.rs:24-27`, `CLAUDE.md` "Deferred post-v0.1").
- No SCRAM-SHA-256-PLUS auth ship; PostGIS limited to 2D EWKB; range/multirange
  limited to built-in scalar families (`crates/core/src/lib.rs:28-46`).
- No public `Error::kind()` enum-classifier; SQLSTATE is a `String` field on
  `Error::Server` and there is **no SQLSTATE-to-variant mapping table** —
  classification is done by reading `code` directly. The "error catalog"
  Reference page must therefore document *variants* (Io / Closed / Protocol /
  Auth / UnsupportedAuth / Server / Config / Codec / ColumnAlignment /
  SchemaMismatch / Migration) plus the `Error::Server { code, … }` shape, and
  point at PostgreSQL's SQLSTATE appendix for the codes themselves.
- No mdbook preprocessor configured; `mdbook test` is not wired up; the only
  build step is `mdbook build` in `.github/workflows/pages.yml`.

## Documentation System

- **Framework**: mdBook (stock; no preprocessors).
- **Docs Directory**: `docs/` (`book.toml:4` sets `src = "docs"`).
- **Navigation Config**: `docs/SUMMARY.md` (4 lines today: `docs/SUMMARY.md:1-4`).
- **Style Conventions** *(target, drawn from `docs/SITE-COPY.md`)*: doobie-style
  voice — second person, code-first, inline `// type: T` annotations, short
  paragraphs. American English. SITE-COPY.md sections: §1 Site shell
  (`docs/SITE-COPY.md:11`), §2 Homepage (`:68`), §3 Get-started page (`:225`),
  §4 Reference & catalog pages (`:271`), §5 Microcopy library (`:298`),
  §6 Voice & tone reference (`:388`), §7 Image cues — quick map (`:436`).
- **Build Command**: `mdbook build` (run from repo root; uses `book.toml`
  at `book.toml:1-10`). The CI invocation lives at
  `.github/workflows/pages.yml:34-39`.
- **Standard Files**: `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md` not present
  yet beyond `README.md`, `CHANGELOG.md`, `MILESTONES.md`, `PLAN.md`,
  `TESTING.md`, `RELEASE.md`, `CLAUDE.md` at repo root.

## Verification Commands

- **Test Command**: `cargo test --all-features` (CI: `.github/workflows/ci.yml`).
- **Lint Command**: `cargo clippy -D warnings`, `cargo fmt --check` (per
  `CLAUDE.md` "Commands" section).
- **Build Command** *(crate)*: `cargo build`. *(book)*: `mdbook build`.
- **Type Check**: `cargo check`. (No `mdbook test` configured — see Section 20.)

## Detailed Findings

### 1. Crate layout & public API surface

Workspace members at `Cargo.toml:3-9`:

- `crates/core/` → published as crate **`babar`** (`crates/core/Cargo.toml:2`).
  This is the single user-facing crate.
- `crates/macros/` → **`babar-macros`** (`crates/macros/Cargo.toml:2`),
  proc-macro only, re-exported through `babar`.
- `benches/pool-throughput/`, `benches/prepared-throughput/` — bench crates,
  not user-facing.

Top-level `use babar::…` re-exports (`crates/core/src/lib.rs:181-198`):

| Re-export | Defined in |
|---|---|
| `Config`, `TlsBackend`, `TlsMode` | `crates/core/src/config.rs:9-25` |
| `CopyIn` | `crates/core/src/copy.rs:60` |
| `Error`, `Result` | `crates/core/src/error.rs:10-15` |
| `MigrationError`, `Migrator`, `MigratorOptions` | `crates/core/src/migration/mod.rs:88,113,166` |
| `Pool`, `PoolConfig`, `PoolConnection`, `PoolError`, `HealthCheck`, `PooledPreparedQuery`, `PooledPreparedCommand`, `PooledRowStream`, `PooledTransaction`, `PooledSavepoint` | `crates/core/src/pool.rs:26-224` |
| `Session`, `PreparedCommand`, `PreparedQuery`, `RawRows`, `RowStream`, `ServerParams` | `crates/core/src/session/mod.rs:32-40` |
| `Savepoint`, `Transaction` | `crates/core/src/transaction.rs:17-24` |
| Macros: `sql!`, `query!`, `command!`, `#[derive(Codec)]` | `crates/core/src/lib.rs:178` (re-exports `babar_macros::*`) |
| `babar::codec::*` | `crates/core/src/codec/mod.rs` |
| `babar::query::{Query, Command, Fragment, Origin}` | `crates/core/src/query/mod.rs:16,25,111` + `crates/core/src/query/fragment.rs:19,49` |
| `babar::migration::*` (table, source, plan, model types) | `crates/core/src/migration/mod.rs:87-104` |
| `babar::types::{Oid, Type, *_OID consts}` | `crates/core/src/types.rs:10-233` |

`crates/core/src/lib.rs:163-176` lists internal vs public modules: `auth`,
`protocol`, `telemetry` are private; everything else surfaces.

### 2. Connecting

A user opens a connection with **`Session::connect(Config)`**
(`crates/core/src/session/mod.rs:52`). `Config` is a struct with a chained
builder API (`crates/core/src/config.rs:54-148`):

- `Config::new(host, port, user, database)` (`config.rs:54`)
- `Config::with_addr(IpAddr, port, user, database)` (`config.rs:74`) — pre-resolved IP
- `.password(s)` (`config.rs:96`)
- `.application_name(s)` (`config.rs:103`)
- `.connect_timeout(Duration)` (`config.rs:110`)
- `.tls_mode(TlsMode)` / `.require_tls()` (`config.rs:117,123`)
- `.tls_backend(TlsBackend)` (`config.rs:130`)
- `.tls_server_name(s)` (`config.rs:137`)
- `.tls_root_cert_path(path)` (`config.rs:144`)

`TlsMode` variants: `Disable | Prefer | Require` (`config.rs:9-15`).
`TlsBackend` variants: `Rustls | NativeTls` (`config.rs:19-25`).

**Not present** — there is no env-var / DSN / connection-URL parsing in
`Config`. Examples like `m0_smoke.rs` read `PG*` env vars manually
(`crates/core/examples/m0_smoke.rs:18-25`). Chapters that mention env vars must
say "the example reads them; `Config` itself is struct-only".

The actual TCP + TLS handshake is delegated to
`crates/core/src/tls.rs:110` (`connect_transport`) and
`crates/core/src/tls.rs:134` (`negotiate_tls`), called from
`crates/core/src/session/startup.rs` (`startup::connect`).

### 3. Selecting / queries

`Session` query surface (all on `crates/core/src/session/mod.rs`):

- `simple_query_raw(&str) -> Result<Vec<RawRows>>` (`mod.rs:62`) — runs through
  the simple-query protocol, returns text-format rows. `RawRows` is
  `type RawRows = Vec<Vec<Option<Bytes>>>` (`crates/core/src/session/driver.rs:55`).
- `query<A, B>(&self, &Query<A, B>, args: A) -> Result<Vec<B>>` (`mod.rs:153`)
  — buffered; collects every row into a `Vec<B>` via the decoder.
- `stream<A, B>(&self, &Query<A, B>, args: A) -> Result<RowStream<B>>`
  (`mod.rs:197`) — portal-backed streaming, default batch size
  `DEFAULT_BATCH_ROWS = 128` (`crates/core/src/session/stream.rs:13`).
- `stream_with_batch_size(...)` (`mod.rs:207`) — explicit batch size.
- `execute<A>(&self, &Command<A>, args: A) -> Result<u64>` (`mod.rs:83`) —
  returns rows-affected count parsed from the server's `CommandComplete` tag.

`Query<A, B>` is parameterized by input-tuple type `A` and output-row type `B`
(`crates/core/src/query/mod.rs:25`). `Command<A>` is the no-rows analogue
(`mod.rs:111`). `Query::raw(sql, encoder, decoder)` (`mod.rs:37`) and
`Command::raw(sql, encoder)` (`mod.rs:117`) are the constructors that don't go
through a macro.

`RowStream<B>` impls `futures_core::Stream`
(`crates/core/src/session/stream.rs:26`).

### 4. Parameterized commands

Parameters are passed positionally as a tuple matching the encoder:

```rust
let insert: Command<(i32, String)> = Command::raw(
    "INSERT INTO users (id, name) VALUES ($1, $2)",
    (int4, text),
);
session.execute(&insert, (1, "Ada".into())).await?;
```
(`crates/core/src/lib.rs:60-68`).

The codec model (`crates/core/src/codec/mod.rs:75-203`):

- **`Encoder<A>`** trait (`codec/mod.rs:146`): `encode(&self, value: &A,
  params: &mut Vec<Option<Vec<u8>>>) -> Result<()>`, plus `oids()`, `types()`,
  `format_codes()`.
- **`Decoder<A>`** trait (`codec/mod.rs:175`): `decode(columns: &[Option<Bytes>])
  -> Result<A>`, plus `n_columns()`, `oids()`, `types()`, `format_codes()`.
- **`Codec<A>`** trait (`codec/mod.rs:203`): blanket impl for any
  `Encoder<A> + Decoder<A>`.

Codec values are unit-struct singletons imported by lowercase name
(`codec/mod.rs:115-119`): `int2, int4, int8, float4, float8, bool, text,
varchar, bpchar, bytea`. `nullable(c)` lifts a codec to `Option<A>`
(`codec/nullable.rs:26`). Tuples up to arity 16 are codecs of tuple types
(`codec/tuple.rs`). Format constants: `FORMAT_TEXT = 0`, `FORMAT_BINARY = 1`
(`codec/mod.rs:138-140`).

### 5. Prepared queries & streaming

`Session::prepare_query<A, B>(&Query<A, B>) -> Result<PreparedQuery<A, B>>`
(`crates/core/src/session/mod.rs:276`) and
`Session::prepare_command<A>(&Command<A>) -> Result<PreparedCommand<A>>`
(`mod.rs:344`).

`PreparedQuery<A, B>` (`crates/core/src/session/prepared.rs:121`):
- `.query(&self, args: A) -> Result<Vec<B>>` (`prepared.rs:141`)
- `.name() -> &str` (`prepared.rs:182`)
- `.sql() -> &str` (`prepared.rs:187`)
- `.origin() -> Option<Origin>` (`prepared.rs:192`)
- `.param_oids()`, `.param_types()`, `.output_oids()`, `.output_types()`,
  `.n_columns()` (`prepared.rs:197-217`)
- `.close(self) -> Result<()>` (`prepared.rs:222`)

`PreparedCommand<A>` (`prepared.rs:243`): mirrors the above with `.execute`
(`:261`) instead of `.query`.

Streaming the result of a one-shot query: `Session::stream` /
`Session::stream_with_batch_size` (Section 3). There is **no** `prepared_stream`
public method today — streaming uses `Query` directly. Schema validation is
performed at prepare time inside the driver task; mismatches surface as
`Error::SchemaMismatch` (see Section 10).

The doc-comments at `crates/core/src/lib.rs:157-161` describe the
"background driver task owns the transport" architecture — the prepared and
streaming surfaces all flow through it.

### 6. Transactions

`Session::transaction(f)` (`crates/core/src/transaction.rs:30`) takes an
async closure receiving a `Transaction<'tx>` and runs `BEGIN` / `COMMIT` /
`ROLLBACK` automatically. Implementation: `transaction.rs:35-55` — `BEGIN`,
run the closure, `COMMIT` on `Ok` and `ROLLBACK` on `Err`, with a
`ScopeGuard` that rolls back on drop.

`Transaction<'a>` (`transaction.rs:17`) and `Savepoint<'a>` (`transaction.rs:24`)
share a common method set generated by `impl_scope_methods!` (`transaction.rs:60-130`):
`simple_query_raw`, `execute`, `query`, `stream`, `stream_with_batch_size`,
`prepare_query`, `prepare_command`. Both expose `.savepoint(f)`
(`transaction.rs:135` for `Transaction`, `:145` for `Savepoint`) — savepoints
nest. A monotonically increasing counter generates unique savepoint names
(`transaction.rs:13` `SAVEPOINT_COUNTER`).

Tracing: `telemetry::transaction_span("transaction")` is the root span
(`transaction.rs:34`).

### 7. Pooling

`Pool` lives at `crates/core/src/pool.rs:154`. Construction:
`Pool::new(connect_config: Config, pool_config: PoolConfig) -> Result<Self>`
(`pool.rs:245`). It is **babar's own implementation**, not deadpool — see the
struct fields (idle/used queues, `Notify`, `Mutex`, `AtomicU64`,
`MAINTENANCE_INTERVAL`, `Weak` self-reference at `pool.rs:1-21`).

**`PoolConfig`** builder (`pool.rs:38-107`):

| Method | Default | Source |
|---|---|---|
| `min_idle(usize)` | `0` | `pool.rs:48,68` |
| `max_size(usize)` | `16` | `pool.rs:49,75` |
| `acquire_timeout(Duration)` | `30s` (`DEFAULT_ACQUIRE_TIMEOUT`, `pool.rs:20`) | `pool.rs:82` |
| `idle_timeout(Duration)` | `None` | `pool.rs:51,89` |
| `max_lifetime(Duration)` | `None` | `pool.rs:52,96` |
| `health_check(HealthCheck)` | `HealthCheck::None` | `pool.rs:54,103` |

`HealthCheck` variants (`pool.rs:26-34`): `None | Ping | ResetQuery(String)`.
`Ping` runs `"SELECT 1"` (`pool.rs:19` `HEALTHCHECK_PING_SQL`).

**Acquire**: `Pool::acquire() -> Result<PoolConnection, PoolError>`
(`pool.rs:263`). `PoolError` (`pool.rs:140`) variants surface timeout vs
underlying `Error`. **Close**: `Pool::close()` (`pool.rs:322`).

**`PoolConnection`** (`pool.rs:182`) mirrors `Session`'s query API at
`pool.rs:492-568` (`simple_query_raw`, `execute`, `query`, `stream`,
`stream_with_batch_size`, `prepare_query`, `prepare_command`, `backend_key`)
and exposes `.transaction(f)` (`pool.rs:573`). Pooled prepared statements
return `PooledPreparedQuery<'a, …>` / `PooledPreparedCommand<'a, …>`
(`pool.rs:191-205`) and pooled streams are `PooledRowStream<'a, B>`
(`pool.rs:207-214`). Pooled scoped types: `PooledTransaction<'a>` (`pool.rs:216`)
and `PooledSavepoint<'a>` (`pool.rs:224`), both with the same scope-method
macro (`pool.rs:737-908`) and nested-savepoint support
(`pool.rs:889-908`).

### 8. COPY / bulk loads

`CopyIn<T>` is a typed `COPY ... FROM STDIN BINARY` statement
(`crates/core/src/copy.rs:60`). Constructor: `CopyIn::binary(sql, row_encoder)`
(`copy.rs:93`). Supplied row encoder is anything `Encoder<T> + Send + Sync +
'static` (`copy.rs:94-103`) — tuple codecs and `#[derive(Codec)]` structs work
directly.

Execution: `Session::copy_in<T, I, R>(&CopyIn<T>, rows: I) -> Result<u64>`
(`crates/core/src/session/mod.rs:121`), where `I: IntoIterator<Item = R>`,
`R: Borrow<T>`. Returns rows-ingested count.

Other public methods on `CopyIn`: `.sql() -> &str` (`copy.rs:104`),
`.column_oids() -> &'static [Oid]` (`copy.rs:109`), `.n_columns() -> usize`
(`copy.rs:114`).

**Scope** (`copy.rs:1-19, 86-92`, also `crates/core/src/lib.rs:24-27`): only
binary `COPY FROM STDIN`; no `COPY TO`, no text/CSV COPY, no streaming source
beyond `IntoIterator`. Chapters must call out explicitly that broader COPY is
deferred post-v0.1.

### 9. Migrations

Library-first migration engine at `crates/core/src/migration/`. Module rustdoc
at `migration/mod.rs:1-77` is the canonical reference (paired
`<version>__<name>.up.sql` / `.down.sql` files; `--! babar:transaction = none`
pragma; advisory-lock-coordinated `apply`).

Top-level types:

- **`Migrator<S>`** (`migration/mod.rs:166`) where `S: MigrationSource`.
  - `Migrator::new(source)` (`:174`)
  - `Migrator::with_options(source, MigratorOptions)` (`:183`)
  - `.catalog() -> Result<MigrationCatalog>` (`:211`)
  - `.status(applied) -> Result<MigrationStatus>` (`:216`)
  - `.plan_apply(applied) -> Result<MigrationPlan>` (`:221`)
  - `.plan_rollback(applied, steps)` (`:226`)
  - `.applied_migrations(&executor)` (`migration/runner.rs:84`)
  - `.apply(&executor) -> Result<MigrationPlan>` (`migration/runner.rs:93`)
  - `.rollback(&executor, steps)` (`migration/runner.rs:115`)
- **`MigratorOptions`** (`migration/mod.rs:113`): `.table(MigrationTable)`,
  `.advisory_lock_id(i64)`.
- **`MigrationSource` trait** (`migration/source.rs:52`).
  Implementations: `MemoryMigrationSource` (`:59`), `FileSystemMigrationSource`
  (`:85`).
- **`MigrationTable`** (`migration/table.rs:31`): `.new(schema, name)`,
  `.qualified_name()`, `.create_if_missing_sql()`. Defaults at
  `table.rs:4,7,10`: `DEFAULT_MIGRATION_SCHEMA = "public"`,
  `DEFAULT_MIGRATION_TABLE = "babar_schema_migrations"`,
  `DEFAULT_MIGRATION_ADVISORY_LOCK_ID = 0x0062_6162_6172` ("\0bbar").
- **`MigrationExecutor`** sealed trait (`migration/runner.rs:32`) — `Session`,
  `Transaction`, `Savepoint`, `Pool` connection types implement it.
- Plan / status / model types: `MigrationCatalog`, `MigrationStatus`,
  `MigrationStatusEntry`, `MigrationStatusState`, `MigrationPlan`,
  `MigrationPlanStep`, `MigrationPair`, `MigrationScript`,
  `MigrationScriptMetadata`, `MigrationTransactionMode`, `MigrationFilename`,
  `MigrationId`, `MigrationKind`, `MigrationChecksum`, `MigrationAsset`,
  `AppliedMigration` (re-exported `migration/mod.rs:87-104`).
- `MigrationError` enum (`migration/error.rs:6`) — separate from `Error` but
  wrapped in `Error::Migration` (`error.rs:95,192-196`).

The CLI shown to users is `crates/core/examples/migration_cli.rs` — there is
no installed binary; the example links those types to `clap`.

### 10. Error types

`crates/core/src/error.rs:10-15`: `pub type Result<T> = std::result::Result<T,
Error>;` and the top-level `pub enum Error` is `#[non_exhaustive]`.

Variants (`error.rs:17-95`):

| Variant | Shape | Notes |
|---|---|---|
| `Io(io::Error)` | newtype | Socket I/O. |
| `Closed { sql, origin }` | struct | Connection closed / driver task gone. |
| `Protocol(String)` | tuple | Protocol-level violation. |
| `Auth(String)` | tuple | Auth failed. |
| `UnsupportedAuth(String)` | tuple | Mechanism not supported. |
| `Server { code, severity, message, detail, hint, position, sql, origin }` | struct | `code` is the SQLSTATE string (e.g. `"23505"`). Built from server `ErrorResponse` fields by `Error::from_server_fields` (`error.rs:153`). |
| `Config(String)` | tuple | Pre-IO configuration problem. |
| `Codec(String)` | tuple | Encode/decode failure. |
| `ColumnAlignment { expected, actual, sql, origin }` | struct | Decoder column count vs `RowDescription` mismatch. |
| `SchemaMismatch { position, expected_oid, actual_oid, column_name, sql, origin }` | struct | First column whose OID disagrees. |
| `Migration(MigrationError)` | wrap | Migration parse/plan/exec failure. |

**SQLSTATE → variant**: babar does **not** map specific SQLSTATEs to their own
variants. Any server-side error becomes `Error::Server { code: "<SQLSTATE>", … }`.
The Reference "error catalog" should document the variant table above plus
note that callers match on `Error::Server { code, .. }` and consult the
PostgreSQL appendix for code semantics.

**`kind()`-style API**: not present. There is no `Error::kind()` method. The
internal helper `Error::with_sql` (`error.rs:114`) is `pub(crate)` only.
`Error: Display` renders SQL context with caret at `error.rs:198-260`. `Error`
is `Debug` but not currently `std::error::Error` per `derive` (it's a custom
`Display` impl — verify if needed when writing the chapter).

### 11. Codecs

All codec modules live under `crates/core/src/codec/`. Each opt-in module is
gated by a Cargo feature. Built-in primitives ship without a feature.

| Codec module | Codec values | Postgres type(s) | Rust type(s) | Cargo feature |
|---|---|---|---|---|
| `codec/primitive.rs:52-486` | `int2, int4, int8, float4, float8, bool, text, varchar, bpchar, bytea` | `INT2`/`INT4`/`INT8`/`FLOAT4`/`FLOAT8`/`BOOL`/`TEXT`/`VARCHAR`/`BPCHAR`/`BYTEA` (`types.rs:87-95,77-85`) | `i16, i32, i64, f32, f64, bool, String, Vec<u8>` | none (always on) |
| `codec/nullable.rs:15-26` | `nullable(c)` | any | `Option<A>` | none |
| `codec/tuple.rs` | tuple codecs arity 1-16 | matching arity | tuples | none |
| `codec/uuid.rs:12-15` | `uuid` | `UUID` (`types.rs:97`) | `uuid::Uuid` | `uuid` |
| `codec/time.rs:1-21` | `date, time, timestamp, timestamptz` (+ `DateCodec, TimeCodec, PrimitiveDateTimeCodec, OffsetDateTimeCodec`) | `DATE/TIME/TIMESTAMP/TIMESTAMPTZ` (`types.rs:99-105`) | `time::Date/Time/PrimitiveDateTime/OffsetDateTime` | `time` |
| `codec/chrono.rs:1-22` | `ChronoDateCodec, ChronoTimeCodec, ChronoNaiveDateTimeCodec, …` (re-exported `codec/mod.rs:80-84`) | same Postgres types as above | `chrono::NaiveDate/NaiveTime/NaiveDateTime/DateTime<Utc>` | `chrono` |
| `codec/json.rs:14-41` | `json, jsonb, typed_json::<T>(), typed_json_text::<T>()` | `JSON/JSONB` (`types.rs:107-109`) | `serde_json::Value`, generic `T: Serialize + DeserializeOwned` | `json` |
| `codec/numeric.rs:12-15` | `numeric` | `NUMERIC` (`types.rs:111`) | `rust_decimal::Decimal` | `numeric` |
| `codec/net.rs:14-23` | `inet, cidr` | `INET/CIDR` (`types.rs:113-115`) | `std::net::IpAddr`-shaped types | `net` |
| `codec/interval.rs:11-36` | `interval` (+ `Interval` value type) | `INTERVAL` (`types.rs:117`) | `Interval` (months / days / microseconds) | `interval` |
| `codec/macaddr.rs:14-105` | `macaddr, macaddr8` (+ `MacAddr`, `MacAddr8`) | `MACADDR/MACADDR8` (`types.rs:119-121`) | `MacAddr([u8;6])`, `MacAddr8([u8;8])` | `macaddr` |
| `codec/bits.rs:17-133` | `bit, varbit` (+ `BitString`) | `BIT/VARBIT` (`types.rs:123-125`) | `BitString` | `bits` |
| `codec/text_search.rs` | `tsvector, tsquery` (+ `TsVector, TsQuery`) | `TSVECTOR/TSQUERY` (`types.rs:127-129`) | `TsVector/TsQuery` (canonical SQL text) | `text-search` |
| `codec/array.rs:16-82` | `array(c)` (+ `Array<T>, ArrayDimension`) | `*_ARRAY` OIDs (`types.rs:132-172`) | `Array<T>` | `array` |
| `codec/range.rs:42-49` | `range(c)` (+ `Range<T>, RangeBound`) | scalar range OIDs (`types.rs:175-185`: `INT4/INT8/NUM/TS/TSTZ/DATE_RANGE`) | `Range<T>` | `range` |
| `codec/multirange.rs:14-50` | `multirange(c)` (+ `Multirange<T>`) | multirange OIDs (`types.rs:188-198`) | `Multirange<T>` | `multirange` (implies `range`) |
| `codec/postgis.rs:96-209` | `geometry(), geography()` (+ `Geometry<T>, Geography<T>, Srid, SpatialKind`) | extension types `geometry`, `geography` (`types.rs:201-203`) | `geo_types::Geometry<f64>` and 2D variants | `postgis` |
| `codec/pgvector.rs:22-` | `vector` (+ `Vector`) | extension type `vector` (`types.rs:205`) | `Vector(Vec<f32>)` | `pgvector` |
| `codec/hstore.rs:16-65` | `hstore` (+ `Hstore`) | extension type `hstore` (`types.rs:207`) | `Hstore(BTreeMap<String, Option<String>>)` | `hstore` |
| `codec/citext.rs:16-19` | `citext` | extension type `citext` (`types.rs:209`) | `String` | `citext` |

The OID constants live in `crates/core/src/types.rs:77-233`. Extension types
are described as `Type::extension(name, extension)` (`types.rs:201-209`) —
those codecs need the matching PostgreSQL extension installed (`lib.rs:30-36`).

**`#[derive(Codec)]`** entry point: `crates/macros/src/lib.rs:380-387`
(`derive_codec` proc-macro, attributes `#[pg(codec = "...")]`). Implementation:
`compile_codec_derive` and helpers around `crates/macros/src/lib.rs:460-610`.
The derive emits an associated `MyType::CODEC` const usable wherever a codec
is wanted (see usage in `crates/core/examples/derive_codec.rs:12-22`,
`crates/core/src/copy.rs:30-46`).

### 12. Feature flags

All features defined at `crates/core/Cargo.toml:18-46`:

| Feature | Enables | Source |
|---|---|---|
| `default = ["rustls"]` | `rustls` TLS backend | `Cargo.toml:18` |
| `rustls` | TLS via `rustls`/`tokio-rustls`/`rustls-native-certs`/`rustls-pemfile` | `Cargo.toml:19-24` |
| `native-tls` | TLS via `native-tls`/`tokio-native-tls` | `Cargo.toml:25` |
| `uuid` | `uuid::Uuid` codec | `Cargo.toml:26` |
| `time` | `time` crate codecs | `Cargo.toml:27` |
| `chrono` | `chrono` crate codecs | `Cargo.toml:28` |
| `json` | `serde`/`serde_json` JSON / JSONB codecs | `Cargo.toml:29` |
| `numeric` | `rust_decimal` NUMERIC codec | `Cargo.toml:30` |
| `net` | INET / CIDR codecs | `Cargo.toml:31` |
| `interval` | INTERVAL codec | `Cargo.toml:32` |
| `array` | Array codecs (pulls in `fallible-iterator`) | `Cargo.toml:33` |
| `range` | Range codecs | `Cargo.toml:34` |
| `postgis` | PostGIS spatial codecs (pulls in `geo-types`) | `Cargo.toml:35` |
| `pgvector` | `pgvector` extension codec | `Cargo.toml:36` |
| `text-search` | `tsvector` / `tsquery` codecs | `Cargo.toml:37` |
| `macaddr` | MAC-address codecs | `Cargo.toml:38` |
| `bits` | BIT / VARBIT codecs | `Cargo.toml:39` |
| `hstore` | `hstore` extension codec | `Cargo.toml:40` |
| `citext` | `citext` extension codec | `Cargo.toml:41` |
| `multirange` | Multirange codecs (implies `range`) | `Cargo.toml:42` |

`crates/macros/Cargo.toml` defines no Cargo features; the macro crate is always
linked.

### 13. Configuration knobs

User-visible configuration lives in two places.

**`Config`** (`crates/core/src/config.rs:27-148`):

| Field | Type | Default | Setter |
|---|---|---|---|
| `host` | `Host::Name(String)` or `Host::Addr(IpAddr)` | required | `Config::new` / `Config::with_addr` (`:54,74`) |
| `port` | `u16` | required | (constructor) |
| `user` | `String` | required | (constructor) |
| `database` | `String` | required | (constructor) |
| `password` | `Option<String>` | `None` | `.password()` (`:96`) |
| `application_name` | `Option<String>` | `None` | `.application_name()` (`:103`) |
| `connect_timeout` | `Option<Duration>` | `None` | `.connect_timeout()` (`:110`) |
| `tls_mode` | `TlsMode` | `Disable` | `.tls_mode()` / `.require_tls()` (`:117,123`) |
| `tls_backend` | `TlsBackend` | `Rustls` | `.tls_backend()` (`:130`) |
| `tls_server_name` | `Option<String>` | `None` | `.tls_server_name()` (`:137`) |
| `tls_root_cert_path` | `Option<PathBuf>` | `None` | `.tls_root_cert_path()` (`:144`) |

**`PoolConfig`** (`crates/core/src/pool.rs:38-107`): see Section 7 table.

**Stream batch size**: `DEFAULT_BATCH_ROWS = 128`
(`crates/core/src/session/stream.rs:13`); user override via
`Session::stream_with_batch_size`.

**Statement cache**: `Session` carries a private `Mutex<StatementCache>`
(`crates/core/src/session/mod.rs:43`, cache impl at
`crates/core/src/session/cache.rs`). It is not currently exposed for tuning;
pooled connections share the cache through `Session::connect_pooled`
(`session/mod.rs:56`).

**Migration knobs**: `MigratorOptions` (`migration/mod.rs:113-156`) — table
override (`MigrationTable::new(schema, name)`, `migration/table.rs:47`) and
advisory-lock id.

### 14. TLS / security

TLS is wired through `crates/core/src/tls.rs`. Backends compile-gated:

- **rustls** (default feature) at `tls.rs:32-34` (`StreamInner::Rustls`).
- **native-tls** (optional feature) at `tls.rs:35-36`
  (`StreamInner::NativeTls`).
- Plain TCP fallback at `tls.rs:31` (`StreamInner::Plain`).

Toggle is `Config::tls_mode(TlsMode::{Disable, Prefer, Require})`
(`config.rs:9-15,117`). Backend selection is `Config::tls_backend(TlsBackend::
{Rustls, NativeTls})` (`config.rs:19-25,130`). SNI override:
`Config::tls_server_name` (`config.rs:137`). Extra root: `Config::tls_root_cert_path`
(`config.rs:144`).

Negotiation: `connect_transport` (`tls.rs:110`) opens TCP, then
`negotiate_tls` (`tls.rs:134`) sends Postgres' `SSLRequest` magic
(`SSL_REQUEST_CODE = 80_877_103`, `tls.rs:17`) and upgrades the stream when
the server replies `S`.

Channel binding for SCRAM-SHA-256-PLUS is partially wired
(`tls.rs:19-23,53` `ChannelBindingState`, `auth/scram.rs`'s `ChannelBinding`).
`crates/core/src/lib.rs:170-175` documents that babar binds SCRAM to TLS when
the server offers it; the deferred-list in `CLAUDE.md` notes
SCRAM-SHA-256-PLUS as post-v0.1, so the chapter should describe it neutrally.

Auth mechanisms supported: cleartext, MD5, SCRAM-SHA-256
(`crates/core/src/auth/`: `md5.rs`, `scram.rs`, `mod.rs`).

### 15. Observability / tracing

All tracing instrumentation is in `crates/core/src/telemetry.rs:1-46`. Spans
created (target `babar`):

| Helper | Span name | Fields |
|---|---|---|
| `connect_span(&Config)` | `db.connect` | `db.system`, `db.user`, `db.name`, `net.peer.name`, `net.peer.port` (`telemetry.rs:7-16`) |
| `prepare_span(sql)` | `db.prepare` | `db.system`, `db.statement`, `db.operation` (`telemetry.rs:18-25`) |
| `execute_span(sql)` | `db.execute` | same fields (`telemetry.rs:27-34`) |
| `transaction_span(label)` | `db.transaction` | `db.system`, `db.operation` (`telemetry.rs:36-42`) |

Spans are entered via `tracing::Instrument`:
- `Session::simple_query_raw`, `execute`, `query`, `stream`,
  `prepare_query`, `prepare_command` all wrap with `execute_span` /
  `prepare_span` (`session/mod.rs:63-403`).
- `Session::transaction` wraps with `transaction_span` (`transaction.rs:34`).
- Connection startup uses `connect_span` (called from `session/startup.rs:11`
  and re-imports it via `tracing::Instrument`).

Internal `debug!`/`trace!`/`warn!` calls are in
`crates/core/src/session/driver.rs:42` and `session/startup.rs:11`. Field
naming follows OpenTelemetry semantic conventions (e.g.
`db.system = "postgresql"`).

### 16. Background driver task model

The architecture is summarized at `crates/core/src/lib.rs:147-161` (ASCII
diagram + paragraph) and elaborated at `crates/core/src/session/driver.rs:1-31`
(invariants, drop safety).

- The driver task lives in `crates/core/src/session/driver.rs` (1499 lines).
- `Session` is a lightweight handle holding a
  `tokio::sync::mpsc::Sender<Command>` (`crates/core/src/session/mod.rs:42`)
  plus shared state (`mod.rs:42-47`: `params`, `key_data`, `cache`,
  `type_registry`, `state`).
- Channel buffer: `COMMAND_BUFFER = 128` (`mod.rs:36`).
- Per-command reply: `tokio::sync::oneshot` channels (e.g.
  `session/driver.rs:60-61` `SimpleQueryReply`, and the typed reply variants
  declared as `enum Command { … }` in `driver.rs`).
- The driver uses `tokio_util::codec::FramedRead<…, BackendCodec>`
  (`session/driver.rs:46`) to decode incoming protocol frames; outgoing frames
  are written directly with `AsyncWriteExt` (`session/driver.rs:38-40`).
- `DriverState` is an `AtomicU8` state machine (`session/driver.rs:34-36`),
  with an explicit Idle / InTransaction / InFailedTransaction / InCopy model
  (per `CLAUDE.md` "Protocol implementation notes").
- Cancellation: dropping a future drops only the matching `oneshot` receiver;
  the driver keeps draining and discards the result, so the next command
  starts from a clean state (`driver.rs:23-31`).

### 17. Examples inventory

All examples live at `crates/core/examples/`. `playground.sh` is a wrapper
shell script for `playground.rs`.

| File | Demonstrates | Feeds chapter(s) |
|---|---|---|
| `m0_smoke.rs:1-25` | Connect, run `SELECT 1`, exit. The minimum viable path. | Get-started / Connecting |
| `quickstart.rs:1-22` | End-to-end `sql!` macro: temp table, parameterized inserts, parameterized SELECT, decoded rows. | Get-started, Selecting, Parameterized commands |
| `prepared_and_stream.rs:1-19` | Prepare one query and one command, execute prepared query repeatedly, stream full result set in batches via `futures_util::StreamExt`. | Prepared queries & streaming |
| `transactions.rs:1-13` | Scoped `Session::transaction`, nested `Savepoint`, rollback-on-error semantics. | Transactions |
| `pool.rs:1-14` | `Pool::new` with `PoolConfig` (max-size, idle-timeout, health check), pooled prepared statements. | Pooling |
| `copy_bulk.rs:1-21` | Typed binary COPY ingest with a `#[derive(Codec)]` row struct. | Bulk loads (COPY), Custom codecs |
| `derive_codec.rs:1-23` | `#[derive(Codec)]` with `#[pg(codec = "varchar")]` override; `MyRow::CODEC` for both insert and select. | Custom codecs / `derive(Codec)` |
| `migration_cli.rs:1-25` | Thin `clap`-driven CLI over `Migrator` + `FileSystemMigrationSource`: `status`, `plan`, `up`, `down --steps`. | Migrations |
| `axum_service.rs:1-26` | Tiny Axum HTTP service backed by `Pool`; widgets resource with int4/text codec usage. | Web service (Axum) |
| `todo_cli.rs:1-25` | `clap` CLI reading `PG*` env vars; demonstrates `Config` builder + `Command`/`Query` for a small CRUD. | Quickstart-adjacent / Connecting (env-var pattern) |
| `playground.rs:1-25` | Multi-section demo of the full surface, with `tracing_subscriber` setup; reads `PG*` env vars. | Observability / general reference |
| `playground.sh` | Shell wrapper (Bash) that runs `playground.rs` against a Docker Postgres. | (Operational, not a chapter source.) |

### 18. Project context

Drawn from repo-root files for the "Why babar" / Explanation section.

- **`README.md`**:
  - Tagline and pitch (`README.md:1-5`).
  - Highlights including OpenTelemetry-friendly span names (`README.md:7-16`).
  - Built-in codec table (`README.md:17-30`) — useful for the codec catalog.
  - Optional feature flags table (`README.md:31-79`) — alternate authoritative
    list to cross-check Section 12.
  - Quick-start snippet (`README.md:80-118`) — duplicates `crates/core/src/lib.rs:46-78`.
  - TLS / COPY / migration sections (`README.md:174-300`).
  - Comparison vs `sqlx` and `tokio-postgres` (`README.md:320-348`) —
    *primary* source for the comparison page; reproduces what to claim about
    each ecosystem partner.
  - Status: ready for `0.1.0` RC (`README.md:350-353`).

- **`CLAUDE.md`** (project ethos + design constraints):
  - Background-task model description (architecture box, "Protocol
    implementation notes", "Key design constraints": no unsafe, validate-early,
    binary format in M2, explicit codec composition, "one way to do things").
  - Resolved decisions and open decisions list.
  - Testing policy ("zero tolerance for flaky tests", benchmark gate).
  - Deferred post-v0.1 list — authoritative source for what to call "roadmap".

- **`PLAN.md`**:
  - Vision and design principles (`PLAN.md:9-46`) — primary source for the
    "design principles" Explanation page.
  - Crate structure (`PLAN.md:71-114`).
  - Core type surface (`PLAN.md:115-172`) — earlier articulation of the same
    types Section 1 lists.
  - Milestone summary (`PLAN.md:188-end`) — links to MILESTONES.md.

- **`MILESTONES.md`**:
  - M0–M6 sections (`MILESTONES.md:10,71,132,195,254,316,385`) and
    "Deferred (post-v0.1)" (`:435`) — primary source for the roadmap page.

- **`CHANGELOG.md`**, **`RELEASE.md`**, **`TESTING.md`** also exist at repo
  root and provide release context if a chapter wants to cite them.

### 19. Existing docs assets

- **`docs/SUMMARY.md`** (4 lines, `:1-4`):
  ```
  # Summary
  - [Tutorial home](index.md)
  - [Postgres API from Scratch](tutorials/postgres-api-from-scratch.md)
  ```
  Will be entirely replaced.

- **`docs/index.md`** (9 lines): existing stub landing page. Will be replaced
  with the doobie-style hero.

- **`docs/SITE-COPY.md`** (452 lines) — voice/microcopy authority. Top-level
  sections:
  - `:1` Title
  - `:11` §1 Site shell
  - `:68` §2 Homepage
  - `:225` §3 Get-started page
  - `:271` §4 Reference & catalog pages
  - `:298` §5 Microcopy library
  - `:388` §6 Voice & tone reference
  - `:436` §7 Image cues — quick map (table mapping site surface → illustration)

- **`docs/landing-mockup.html`** (603 lines) — structural elements (anchor IDs
  via `grep -nE "<section "`):
  - `:319` `<section class="hero">` — hero with `.hero-grid`, eyebrow
    "A Postgres driver for Rust", `<h1>` with accented `<span>`, `.lead`,
    `.cta-row` (`Start the tutorial` + `Read the design notes`),
    `.install-line` (`cargo add babar`), and `.hero-illo`.
  - `:362` `<section id="why">` — pillars / why-babar band.
  - `:389` `<section id="start">` — get-started CTA.
  - `:431` `<section id="guides">` — guides nav.
  - `:485` `<section class="ext-band">` — extensions panel
    (pgvector, postgis, pg_trgm, hstore, citext, pgcrypto, pg_partman, timescaledb).
  - `:542` `<section class="closing">` — closing CTA.
  - Header at `:301-318` (`<header class="site">`, brand `<svg class="crown">`,
    nav, GitHub link).

- **`docs/tutorials/postgres-api-from-scratch.md`** (1162 lines, 32468 bytes):
  - **Internal markdown links / image refs**: a `grep` for `]( ` / `![` shows
    **zero relative-path links and zero image references** in the file.
    There is no breakage risk from relocating images or changing the SUMMARY
    structure. `grep -c "\[.*\](.*)" docs/tutorials/postgres-api-from-scratch.md`
    returns 0.
  - First lines (`:1-13`) frame it as Tokio + Axum + babar; this is the
    Tutorial-quadrant marquee artifact preserved verbatim per Spec A4.

- **`images/`** at repo root (`ls -la images/`):

  | File | Bytes | Sidecar | Authoritative kebab target (confirmed by maintainer) | Content / use |
  |---|---|---|---|---|
  | `ChatGPT Image Apr 26, 2026, 10_48_17 PM.png` | 2,805,469 | `:Zone.Identifier` (delete) | `babar-extensions.png` | Postgres extensions showcase |
  | `ChatGPT Image Apr 26, 2026, 10_48_27 PM.png` | 2,285,426 | `:Zone.Identifier` (delete) | `babar-brand-sheet.png` | Master brand sheet — landing-page hero |
  | `ChatGPT Image Apr 26, 2026, 10_48_32 PM.png` | 2,817,010 | `:Zone.Identifier` (delete) | `babar-scenes.png` | Seven-panel scene grid |
  | `ChatGPT Image Apr 26, 2026, 10_48_23 PM.png` | 2,686,457 | `:Zone.Identifier` (delete) | `babar-collage-alt.png` | Alternate brand collage |

  All four `:Zone.Identifier` files are 25-byte Windows ADS sidecars and must
  not be copied into `docs/assets/img/` per Spec FR-010.

### 20. Build/test infrastructure

- **mdbook build CI**: `.github/workflows/pages.yml` (deploys to GitHub Pages).
  - Triggers on push to `main` for paths `book.toml`, `CNAME`, `docs/**`,
    workflow itself (`pages.yml:1-12`).
  - Installs `mdbook --locked` via cargo (`pages.yml:30-32`).
  - Runs `mdbook build` + `cp CNAME book/CNAME` (`pages.yml:34-39`).
  - Uploads `./book` as Pages artifact and deploys
    (`pages.yml:41-58`).
- **`mdbook test`**: **not present anywhere** in the workflows or the repo.
  Only `mdbook build` is the gate. Spec assumption A5 explicitly accepts this.
- **CI for the crate** (`.github/workflows/ci.yml`): `cargo test --all-features`
  matrix on Rust 1.75.0 and stable, plus a `lint` job. No book build.
- **`book.toml`** (`book.toml:1-10`): `src = "docs"` (so anything under
  `.design/` at repo root is **not** built into the site — Spec A7),
  `title = "babar tutorial"` (slated to become `"The Book of Babar"` per A1),
  `output.html.default-theme = "navy"`,
  `git-repository-url = "https://github.com/notthatbreezy/babar"`,
  `site-url = "https://babar.notthatbreezy.io/"`. No `[preprocessor.*]`
  section — stock mdbook only (Spec A8).

## Code References

- `crates/core/src/lib.rs:163-198` — public re-exports (the canonical map of
  the user-facing surface).
- `crates/core/src/config.rs:9-148` — `Config`, `TlsMode`, `TlsBackend`.
- `crates/core/src/session/mod.rs:40-423` — `Session` public methods.
- `crates/core/src/session/driver.rs:1-80` — driver task invariants & frames.
- `crates/core/src/session/prepared.rs:121-318` — `PreparedQuery` /
  `PreparedCommand`.
- `crates/core/src/session/stream.rs:13-142` — `RowStream` & batch defaults.
- `crates/core/src/transaction.rs:17-145` — `Transaction`, `Savepoint`.
- `crates/core/src/pool.rs:26-908` — `Pool`, `PoolConfig`, `HealthCheck`,
  pooled wrappers.
- `crates/core/src/copy.rs:60-114` — `CopyIn` API.
- `crates/core/src/migration/mod.rs:87-231` — migration top-level API.
- `crates/core/src/migration/runner.rs:32-125` — `MigrationExecutor` trait
  and `Migrator::apply` / `rollback`.
- `crates/core/src/migration/source.rs:8-103` — sources.
- `crates/core/src/migration/table.rs:4-75` — table & defaults.
- `crates/core/src/error.rs:10-260` — `Error`, `Result`, display + caret.
- `crates/core/src/codec/mod.rs:1-205` — codec traits & re-exports.
- `crates/core/src/codec/primitive.rs:52-486` — primitive codecs.
- `crates/core/src/codec/{array,range,multirange,nullable,uuid,time,chrono,json,
  numeric,net,interval,macaddr,bits,hstore,citext,pgvector,postgis,text_search,
  tuple}.rs` — all opt-in codec modules.
- `crates/core/src/types.rs:10-313` — OID constants and `Type` metadata.
- `crates/core/src/tls.rs:1-462` — TLS transport, channel binding.
- `crates/core/src/auth/{mod,md5,scram}.rs` — auth mechanisms.
- `crates/core/src/telemetry.rs:1-46` — tracing spans.
- `crates/core/Cargo.toml:18-46` — feature flags.
- `crates/macros/src/lib.rs:380-387,460-610` — `#[derive(Codec)]` macro.
- `crates/core/examples/*.rs` — see Section 17.
- `book.toml:1-10` — mdbook config.
- `docs/SUMMARY.md:1-4`, `docs/index.md:1-9`, `docs/SITE-COPY.md:1-452`,
  `docs/landing-mockup.html:1-603`,
  `docs/tutorials/postgres-api-from-scratch.md:1-1162` — current docs assets.
- `images/*.png` — brand PNGs at repo root (4 files + 4 `:Zone.Identifier`
  sidecars).
- `.github/workflows/pages.yml:1-58` — Pages build/deploy.
- `.github/workflows/ci.yml:1-30` — crate CI.

## Architecture Documentation

- **Single user-facing crate**: `babar` (`crates/core/`). All public types
  surface from `crates/core/src/lib.rs`. The `babar-macros` crate is internal
  to the user's perspective.
- **Background driver task** owns the transport; `Session` is an mpsc handle
  (`session/mod.rs:40-47`, `session/driver.rs:1-31`). This is the single most
  important architectural fact for the "why babar" / cancellation-safety
  chapter.
- **Codecs as values, not traits**: imported lowercase consts
  (`codec/mod.rs:115-130`) compose into tuples; `Encoder`/`Decoder`/`Codec`
  are traits but the user rarely names them — they pass codec values directly.
- **SQL as typed values**: `Query<A, B>`, `Command<A>`, `Fragment<A>` carry
  encoder, decoder, and origin metadata (`query/mod.rs:25-156`,
  `query/fragment.rs:19-225`).
- **Validate-early**: the driver runs a `Describe` round-trip on `prepare_*`
  (`session/mod.rs:276-403`), which is where `SchemaMismatch` /
  `ColumnAlignment` errors fire (`error.rs:67-92`).
- **No unsafe**: `crates/core/Cargo.toml:96` and `crates/macros/Cargo.toml:18`
  both set `unsafe_code = "forbid"`.
- **TLS feature-gated**: rustls default (`Cargo.toml:18`), native-tls optional
  (`Cargo.toml:25`); SSL request handshake at `tls.rs:17,134`.

## Open Questions

- **Final image-name mapping**: SITE-COPY.md §7 lists nine image cues but the
  `images/` directory only contains four PNGs. Implementation must visually
  inspect each PNG and pick the closest cue (proposed kebab targets in
  Section 19 are best-guess placeholders, not authoritative). Chapters that
  embed images must use whatever final name the implementation picks.
- **Feature-flag audit completeness**: `README.md:31-79` may include flag
  descriptions that are slightly more user-friendly than the raw
  `Cargo.toml:18-46` list. The Reference "feature flags" page should
  cross-check against both.
- **Error catalog scope**: there is no SQLSTATE→variant table in the codebase
  (Section 10). The chapter will need to decide whether to (a) document only
  babar's enum variants and link out to the PostgreSQL appendix, or
  (b) curate a hand-picked list of common SQLSTATE codes (`23505`, `40001`,
  `42P01`, …) with usage guidance — a planning decision, not a research one.
