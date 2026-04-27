# babar

Typed, async PostgreSQL driver for Tokio that speaks the PostgreSQL wire protocol directly.

`babar` is intentionally explicit: queries and commands are typed values, codecs are imported values, SQL composition is opt-in via `sql!`, `#[derive(Codec)]` infers common struct fields and lets `#[pg(codec = "...")]` override the outliers, and a background driver task owns the socket so public API calls stay cancellation-safe.

## Highlights

- direct wire-protocol implementation on Tokio — no `libpq`, no `tokio-postgres`
- typed `Query`, `Command`, `PreparedQuery`, `PreparedCommand`, `Transaction`/`Savepoint`, and `Pool` APIs
- typed binary `CopyIn<T>` for `COPY FROM STDIN` bulk ingest from `Vec<T>` / iterators
- SQL composition with `sql!` and `#[derive(Codec)]` (inference first, explicit overrides when needed)
- rich errors with SQLSTATE fields plus SQL/caret rendering
- OpenTelemetry-friendly `tracing` spans: `db.connect`, `db.prepare`, `db.execute`, `db.transaction`
- TLS via `rustls` (default) or `native-tls`

## Built-in codecs

These ship in the core crate with no extra Cargo feature flag:

| Codec family | Included surface |
| --- | --- |
| integers | `int2`, `int4`, `int8` |
| floating point | `float4`, `float8` |
| booleans | `bool` |
| text / strings | `text`, `varchar`, `bpchar` |
| binary | `bytea` |
| nullability | `nullable(codec)` |
| composition | tuple codecs (arities 1-16) |

## Optional feature flags

| Feature | Purpose | Default |
| --- | --- | --- |
| `rustls` | TLS with pure-Rust certificates / SNI / verification | ✅ |
| `native-tls` | Alternate TLS backend using the platform stack | ❌ |
| `uuid` | `uuid::Uuid` codecs | ❌ |
| `time` | `time` date/time codecs | ❌ |
| `chrono` | `chrono` date/time codecs | ❌ |
| `json` | `json`, `jsonb`, typed JSON codecs | ❌ |
| `numeric` | `rust_decimal::Decimal` codec | ❌ |
| `net` | `inet` / `cidr` codecs | ❌ |
| `interval` | PostgreSQL interval codec | ❌ |
| `array` | binary array codec/combinators | ❌ |
| `range` | binary range codec/combinators | ❌ |
| `postgis` | PostGIS `geometry` / `geography` codecs for common 2D `geo-types` shapes | ❌ |
| `pgvector` | `Vector` wrapper plus dynamic-`vector` codec | ❌ |
| `text-search` | `TsVector` / `TsQuery` wrappers plus text-search codecs | ❌ |
| `macaddr` | `macaddr` / `macaddr8` codecs with `MacAddr` / `MacAddr8` values | ❌ |
| `bits` | `bit` / `varbit` codecs with explicit `BitString` length tracking | ❌ |
| `hstore` | `hstore` codec backed by a stable `Hstore` map wrapper | ❌ |
| `citext` | `citext` codec value mapped to Rust `String` | ❌ |
| `multirange` | binary multirange codec/combinators layered on `Range` | ❌ |

Advanced codecs now mix fixed-OID families (`macaddr`, `bits`) with
extension-resolved families (`hstore`, `citext`, `postgis`). The `postgis`
feature now ships binary PostGIS codecs on top of that dynamic type-resolution
path: `geo-types` values stay primary, while babar's `Geometry<T>` /
`Geography<T>` wrappers carry optional `Srid` metadata and keep the SQL type
distinction explicit. v1 deliberately supports common 2D shapes (`Point`,
`LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon`) and
does not yet cover Z/M geometries, `GeometryCollection`, or PostgreSQL's
built-in geometric types. The `multirange` feature builds directly on the same
`Range<T>` model used by the `range` family, adding a thin `Multirange<T>`
wrapper rather than a separate shape.

Important caveats for the new families:

- `postgis`, `pgvector`, `hstore`, and `citext` require the matching PostgreSQL
  extension to be installed in the target database.
- `pgvector` uses a dedicated `Vector` wrapper, requires at least one finite
  `f32` dimension, and resolves the extension OID dynamically per session.
- `text-search` intentionally keeps `TsVector` / `TsQuery` as canonical SQL text
  wrappers in v0.1 rather than exposing a parsed Rust AST.
- `range` / `multirange` currently support PostgreSQL's built-in scalar range
  families with binary inner codecs (`int4`, `int8`, `numeric`, `date`,
  `timestamp`, `timestamptz`); they are not a generic wrapper for arbitrary
  extension types.

## Quick start

```rust,no_run
use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let cfg = Config::new("localhost", 5432, "postgres", "postgres")
        .password("secret")
        .application_name("babar-readme");
    let session = Session::connect(cfg).await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE demo_users (id int4 PRIMARY KEY, name text NOT NULL)",
        (),
    );
    session.execute(&create, ()).await?;

    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO demo_users (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    session.execute(&insert, (1, "Ada".to_string())).await?;

    let select: Query<(), (i32, String)> = Query::raw(
        "SELECT id, name FROM demo_users ORDER BY id",
        (),
        (int4, text),
    );
    let rows = session.query(&select, ()).await?;
    assert_eq!(rows, vec![(1, "Ada".to_string())]);

    session.close().await?;
    Ok(())
}
```

## Compile-time SQL verification

`babar` keeps `Query::raw` / `Command::raw` as the default runtime path, but it
also offers optional macro-driven online verification:

```rust
use babar::codec::{int4, text};

let lookup = babar::query!(
    "SELECT id, name FROM users WHERE id = $1",
    params = (int4,),
    row = (int4, text),
);

let insert = babar::command!(
    "INSERT INTO users (id, name) VALUES ($1, $2)",
    params = (int4, text),
);
```

During macro expansion, babar first checks `BABAR_DATABASE_URL`, then
`DATABASE_URL`. If either is set, `query!` / `command!` verify declared
parameter and row metadata against a live Postgres server, while `sql!`
best-effort verifies parameter metadata when every binding uses the v1
verifiable subset: `int2`, `int4`, `int8`, `bool`, `text`, `varchar`, `bytea`,
`nullable(...)`, and tuples of those.

Without config, the macros still compile and emit the same `Query`, `Command`,
or `Fragment` values. For verified workflows, prefer `query!` / `command!`;
`sql!` intentionally does not validate row shapes. v0.1 does not ship an
offline cache or generated schema snapshot.

## TLS

TLS is opt-in at runtime and explicit in configuration:

```rust,no_run
use babar::{Config, TlsMode};

let _cfg = Config::new("db.example.com", 5432, "app", "app")
    .password("secret")
    .tls_mode(TlsMode::Require);
```

When connecting by IP address, set `tls_server_name("db.example.com")` so SNI and hostname verification still use the certificate's DNS name. For self-signed deployments, point `tls_root_cert_path(...)` at the CA PEM file. Over TLS, babar automatically upgrades SCRAM to `SCRAM-SHA-256-PLUS` when PostgreSQL offers channel binding.

## Bulk ingest with COPY

`babar` ships a dedicated typed API for **binary `COPY FROM STDIN`** bulk ingest. `#[derive(Codec)]` infers the common field codecs here; add `#[pg(codec = "...")]` only when you want a different mapping or inference does not apply:

```rust,no_run
use babar::{CopyIn, Session};
use babar::query::Query;
use babar::Config;

#[derive(Clone, Debug, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    email: String,
    note: Option<String>,
    #[pg(codec = "varchar")]
    handle: String,
}

# async fn demo() -> babar::Result<()> {
let session = Session::connect(
    Config::new("localhost", 5432, "postgres", "postgres").password("secret"),
)
.await?;

session
    .simple_query_raw(
        "CREATE TEMP TABLE copy_users (id int4 PRIMARY KEY, email text NOT NULL, note text, handle varchar NOT NULL)",
    )
    .await?;

let rows = vec![
    UserRow { id: 1, email: "ada@example.com".into(), note: Some("first".into()), handle: "ada".into() },
    UserRow { id: 2, email: "bob@example.com".into(), note: None, handle: "bob".into() },
];

let copy = CopyIn::binary(
    "COPY copy_users (id, email, note, handle) FROM STDIN BINARY",
    UserRow::CODEC,
);
session.copy_in(&copy, rows).await?;

let select: Query<(), UserRow> = Query::raw(
    "SELECT id, email, note, handle FROM copy_users ORDER BY id",
    (),
    UserRow::CODEC,
);
assert_eq!(session.query(&select, ()).await?.len(), 2);
session.close().await?;
# Ok(())
# }
```

The COPY surface is intentionally limited to bulk ingest with binary `COPY FROM STDIN`. `COPY TO`, text COPY, and CSV COPY are not implemented.

## Schema migrations

`babar` ships a library-first migration engine plus a thin CLI example wrapper.

- file names are paired as `<version>__<name>.up.sql` and `<version>__<name>.down.sql`
- `version` is a `u64`; `name` must be lowercase `snake_case`
- each migration must provide both files
- scripts are transactional by default; opt out per file with `--! babar:transaction = none`
- applied history lives in `public.babar_schema_migrations` by default

Use the library API during startup before serving traffic:

```rust,no_run
use babar::migration::FileSystemMigrationSource;
use babar::{Config, Migrator, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session = Session::connect(
        Config::new("localhost", 5432, "postgres", "app").password("secret"),
    )
    .await?;
    let migrator = Migrator::new(FileSystemMigrationSource::new("migrations"));
    let plan = migrator.apply(&session).await?;
    println!("applied {} migration(s)", plan.steps().len());
    session.close().await?;
    Ok(())
}
```

That startup path is safe to call from multiple processes: babar creates the
state table if needed, acquires a PostgreSQL advisory lock before changing
state, and treats re-running `apply` as a no-op once the applied prefix matches
disk.

The CLI example wraps the same engine:

```text
cargo run -p babar --example migration_cli -- status
cargo run -p babar --example migration_cli -- plan
cargo run -p babar --example migration_cli -- up
cargo run -p babar --example migration_cli -- down --steps 1
```

Key operational rules:

- `status`, `plan`, `up`, and `down` all enforce checksum and transaction-mode
  drift detection for already-applied migrations
- advisory locking only serializes babar migration runners that share the same
  lock id; override it with `MigratorOptions` or `--migration-lock-id` only on
  purpose
- non-transactional scripts run outside an explicit transaction so PostgreSQL
  features like `CREATE INDEX CONCURRENTLY` work, but partial effects may remain
  if such a script fails
- rollbacks only cover the currently applied prefix and only what the checked-in
  `down` scripts can reverse; requesting more steps than are applied just rolls
  back the whole applied prefix

## Examples

Real-world example programs live in `crates/core/examples/`:

- `quickstart` — smallest typed end-to-end example
- `derive_codec` — struct mapping with inferred `#[derive(Codec)]` defaults
- `prepared_and_stream` — prepared statements plus streaming
- `transactions` / `pool` — M4 lifecycle walkthroughs
- `copy_bulk` — `Vec<Struct>` bulk ingest with `CopyIn<T>`
- `migration_cli` — migration status/plan/apply/rollback wrapper over the shared engine
- `todo_cli` — CLI app using `clap`
- `axum_service` — small Axum JSON API backed by `Pool`

Run one with:

```text
cargo run -p babar --example todo_cli -- --help
cargo run -p babar --example axum_service
```

## Comparison

### vs `sqlx`

What `babar` does better:

- explicit runtime codec values instead of trait-driven inference
- normal builds do not require a compile-time database connection or offline cache
- SQL-origin-aware runtime errors with caret rendering

What `sqlx` does better:

- broader compile-time checked query macros, including offline-cache workflows
- broader database coverage
- much larger ecosystem and production maturity today

### vs `tokio-postgres`

What `babar` does better:

- typed query/command values are the API, not a thin wrapper on raw SQL strings
- explicit prepare-time schema validation with codec metadata
- richer user-facing error rendering and `sql!` origin tracking

What `tokio-postgres` does better:

- battle-tested stability and wider operational history
- broader feature coverage today (notably `COPY TO`, text/CSV COPY, cancel, and LISTEN/NOTIFY style surface)
- no need to buy into babar's explicit codec model

## Status

`babar` is ready for a `0.1.0` release candidate in this repository. Remaining release work that cannot be completed purely in-repo (for example, publishing to crates.io or pushing a Git tag) is captured in `RELEASE.md`.
