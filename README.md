# babar

Typed, async PostgreSQL driver for Tokio that speaks the PostgreSQL wire protocol directly.

`babar` is explicit: queries and commands are typed values, schema-aware `query!` / `command!` are the default typed SQL path, `typed_query!` remains available as a compatibility alias during the unification transition, `sql!` is the lower-level fragment builder, `#[derive(Codec)]` infers common struct fields and lets `#[pg(codec = "...")]` override the outliers, and a background driver task owns the socket so public API calls stay cancellation-safe.

## Highlights

- direct wire-protocol implementation on Tokio — no `libpq`, no `tokio-postgres`
- typed `Query`, `Command`, `PreparedQuery`, `PreparedCommand`, `Transaction`/`Savepoint`, and `Pool` APIs
- typed binary `CopyIn<T>` for `COPY FROM STDIN` bulk ingest from `Vec<T>` / iterators
- schema-aware typed SQL with `query!` / `command!`, authored schemas via `schema!`, and `typed_query!` as a compatibility alias during migration
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

## Optional codecs enabled via feature flags

| Feature | Purpose | On by Default |
| --- | --- | --- |
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
use babar::query::{Command, Query};
use babar::{Config, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct DemoUser {
    id: i32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct MinUserId {
    min_id: i32,
}

babar::schema! {
    mod app_schema {
        table demo_users {
            id: primary_key(int4),
            name: text,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let cfg = Config::new("localhost", 5432, "postgres", "postgres")
        .password("secret")
        .application_name("babar-readme");
    let session = Session::connect(cfg).await?;

    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE demo_users (id int4 PRIMARY KEY, name text NOT NULL)");
    session.execute(&create, ()).await?;

    let insert: Command<DemoUser> =
        app_schema::command!(INSERT INTO demo_users (id, name) VALUES ($id, $name));
    session
        .execute(
            &insert,
            DemoUser {
                id: 1,
                name: "Ada".to_string(),
            },
        )
        .await?;

    let select: Query<MinUserId, DemoUser> = app_schema::query!(
        SELECT demo_users.id, demo_users.name
        FROM demo_users
        WHERE demo_users.id >= $min_id
        ORDER BY demo_users.id
    );
    let rows = session.query(&select, MinUserId { min_id: 1 }).await?;
    assert_eq!(
        rows,
        vec![DemoUser {
            id: 1,
            name: "Ada".to_string(),
        }]
    );

    session.close().await?;
    Ok(())
}
```

## Development

Local commands that match every CI gate one-to-one. Run the **Pre-push checklist**
below before `git push` to a PR branch — it covers everything CI runs and surfaces
the same failures.

### Toolchain setup

`babar`'s MSRV is in `Cargo.toml` under `rust-version`. CI tests both the MSRV
floor and current `stable`. To exercise both locally:

```bash
# Install rustup + the MSRV toolchain (one-time)
MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2)
rustup toolchain install "$MSRV" --profile minimal --component clippy,rustfmt
rustup toolchain install stable --profile minimal --component clippy,rustfmt

# Tools used by the hygiene job (one-time install; slow first build)
cargo install --locked cargo-deny cargo-audit cargo-semver-checks cargo-msrv
cargo install --locked mdbook
```

> Running `cargo check` against your *current* toolchain does **not** catch
> `requires rustc X.Y` errors from transitive deps. Always run the MSRV toolchain
> for that gate (the pre-push checklist below does it for you).

### Local Postgres for tests and tutorials

Most chapters in [`docs/`](docs/) and the integration tests assume a local
Postgres reachable on `localhost:5432`. Run one in the foreground with verbose
query logging so you can watch every statement land:

```bash
docker run --rm -it \
  --name babar-pg \
  -p 5432:5432 \
  postgres:17 \
  -c log_statement=all \
  -c log_destination=stderr \
  -c log_min_duration_statement=0 \
  -c log_connections=on \
  -c log_disconnections=on
```

Default credentials baked into the image: user `postgres`, password `postgres`,
db `postgres`. Connection string: `postgres://postgres:postgres@localhost:5432/postgres`.
Ctrl-C kills the container; `--rm` discards data — exactly what you want for
local dev.

### Pre-push checklist

This block reproduces every CI gate. Run it from the repo root before pushing
to any branch with an open PR:

```bash
MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2)

# 1. Format (CI: lint job)
cargo fmt --check

# 2. Clippy on stable AND MSRV with -D warnings (CI: lint job)
cargo +stable clippy --all-targets --all-features -- -D warnings
cargo +"$MSRV" clippy --all-targets --all-features -- -D warnings

# 3. Rustdoc with -D warnings (CI: lint job)
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# 4. Tests on MSRV AND stable (CI: test matrix)
cargo +"$MSRV" test --all-features
cargo +stable test --all-features

# 5. Hygiene (CI: hygiene job)
cargo deny check
cargo audit
cargo msrv verify --manifest-path crates/core/Cargo.toml --all-features -- cargo check --all-features
cargo msrv verify --manifest-path crates/macros/Cargo.toml -- cargo check
cargo semver-checks --workspace --baseline-rev origin/main
cargo publish --dry-run --allow-dirty -p babar-macros

# 6. mdbook builds clean (CI: pages workflow)
mdbook build
```

If any step fails, fix it locally first — don't push and let CI catch it. The
matrix is intentionally redundant: `cargo +stable clippy` and `cargo +$MSRV
clippy` can disagree (newer rustc adds new lints; older deps may lint
differently). CI runs both, so you should too.

### Faster iteration loops

The full checklist takes a few minutes from a cold cache. While iterating on a
single change, `cargo check --all-features` and `cargo test -p <crate>` are
fine; just run the full block before push.

For doc-only changes, only steps 3 and 6 are required. For source-only changes
that don't touch `Cargo.toml` / `Cargo.lock`, you can skip step 5's
`cargo audit` / `cargo deny` (they validate the dependency graph, which hasn't
moved).

### Common failures

- **`feature edition2024 is required`** — a transitive dep needs a newer rustc
  than your MSRV floor. Either bump `rust-version` in `Cargo.toml` (and the CI
  matrix in `.github/workflows/ci.yml`) or pin the offending crate via
  `cargo update -p <name> --precise <older-version>`.
- **`-D warnings` clippy failure that doesn't reproduce** — run with
  `cargo +stable` *and* `cargo +$MSRV`. Newer rustc adds lints that older
  toolchains don't know about.
- **`cargo publish --dry-run` failure** — usually a missing `description`,
  `license`, or `repository` field, or a path-only dependency on a workspace
  crate without a corresponding `version =`. `babar-macros` can be verified
  directly; `babar` itself must wait until `babar-macros` is visible in the
  crates.io index.

### Continuous integration

CI is defined in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) and
[`.github/workflows/pages.yml`](.github/workflows/pages.yml). After pushing,
read live status without leaving the terminal:

```bash
gh pr checks            # status of the PR linked to the current branch
gh run watch            # follow the most recent run live
gh run view --log-failed   # only the failed jobs' logs
```

## Tutorial

For a guided build from an empty directory, start with
[`docs/tutorials/postgres-api-from-scratch.md`](docs/tutorials/postgres-api-from-scratch.md).
It is the long-form path for readers with basic Rust experience and little
Tokio background who want to build a small Postgres-backed API with Axum,
babar, and Dial9-backed observability. The README stays focused on reference
material; the tutorial owns the end-to-end walkthrough.

The same tutorial is published via GitHub Pages at
[`https://babar.notthatbreezy.io`](https://babar.notthatbreezy.io).

## Compile-time SQL verification

`babar`'s primary typed SQL surface is now schema-aware `query!` /
`command!`:

```rust
use babar::query::{Command, Query};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct LookupArgs {
    min_id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
    active: bool,
}

let lookup: Query<LookupArgs, UserRow> = babar::query!(
    schema = {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    },
    params = LookupArgs,
    row = UserRow,
    SELECT users.id, users.name
    FROM users
    WHERE users.id >= $min_id AND users.active = true
);

let insert: Command<NewUser> = babar::command!(
    schema = {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    },
    params = _,
    INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
);

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    }
}

let lookup: Query<LookupArgs, UserRow> = app_schema::query!(
    params = _,
    row = _,
    SELECT users.id, users.name
    FROM users
    WHERE users.id >= $min_id AND users.active = true
);
```

`query!` / `command!` now share the same schema-aware compiler:

- use inline `schema = { ... }` blocks for one-off examples and tests
- use `schema! { mod app_schema { ... } }` plus `app_schema::query!(...)` /
  `app_schema::command!(...)` for reusable authored schemas
- `typed_query!` remains available as a compatibility alias to the same
  compiler during this transition, and schema modules also re-export
  `typed_query!` / `typed_command!`
- during macro expansion, babar first checks `BABAR_DATABASE_URL`, then
  `DATABASE_URL`
- today, live verification runs for schema-aware `SELECT` statements
  (`query!`, `typed_query!`, and schema-scoped wrappers), checking authored
  schema facts, parameters, and returned columns against a live PostgreSQL
  server
- non-`RETURNING` `command!` calls and explicit-`RETURNING` DML are not yet
  probe-verified through that path
- without config, the macros still compile and emit the same runtime `Query` /
  `Command` values
- v0.1 does not ship an offline cache, generated schema snapshot, file-based
  schema input, or live schema introspection flow
- unique table names stay available as `app_schema::users`; if two SQL schemas
  share a table name, use namespaces like `app_schema::public::users`
- authored fields stay type-first: `type_name`, `nullable(type_name)`,
  `primary_key(type_name)`, and `pk(type_name)`
- authored declarations accept `bool`, `bytea`, `varchar`, `text`, `int2`,
  `int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`, `timestamp`,
  `timestamptz`, `json`, `jsonb`, and `numeric`
- typed SQL currently lowers inferred parameters and projected columns across
  that same family, including nullable variants; the matching babar feature
  must still be enabled for families such as `uuid`, `time`, `json`, and
  `numeric`
- named placeholders reuse slots when repeated, and optional forms stay
  explicit: `$value?` only for supported `WHERE` / `JOIN` comparisons or full
  `LIMIT` / `OFFSET` expressions, `(...)?` only for a full parenthesized
  `WHERE` / `JOIN` predicate or a single `ORDER BY` expression

The supported subset is intentionally small. v1 expects exactly one statement
and keeps reads narrow: explicit projections, one `FROM` relation plus optional
joins, optional `WHERE` / `ORDER BY` / `LIMIT` / `OFFSET`, and no `SELECT *`,
`WITH` / CTEs, subqueries, `DISTINCT`, `GROUP BY` / `HAVING`, set operations,
or multi-statement input. Writes are limited to `INSERT ... VALUES`,
`UPDATE ... WHERE`, and `DELETE ... WHERE`, with explicit-column `RETURNING`
lowering through the query-shaped row path. v1 does not cover
`INSERT ... SELECT`, `ON CONFLICT`, `UPDATE ... FROM`, `DELETE ... USING`,
wildcard `RETURNING *`, or `UPDATE` / `DELETE` without a `WHERE` predicate. It
is not a general SQL rewrite engine, ORM, or codegen workflow.

Schema-aware typed SQL can also pin or expose the struct contract at the macro
site:

- `query!(schema = { ... }, params = LookupArgs, row = UserRow, SELECT ...)`
- `command!(schema = { ... }, params = NewUser, INSERT ...)`
- `params = _` / `row = _` when you want surrounding `Query<A, B>` /
  `Command<A>` types to stay the source of inference
- omit the selection entirely when the inferred tuple contract is the right fit

Explicit `params = Type` / `row = Type` selections win over surrounding type
inference. The old string-literal explicit-codec forms are still gone; these
shape selections only apply to the schema-aware token-style macros.

Current limitation: `params = Type` and `params = _` are not yet supported for
typed SQL statements that use optional placeholders (`$value?`) or toggle
groups (`(...)?`). Those statements must omit the `params` selection and keep
the default tuple-shaped parameter contract.

## Choosing the right SQL surface

Use the highest-level surface that still fits the statement:

- **`query!` / `command!`** — default path for schema-aware typed SQL in the
  supported subset
- **`Query::raw` / `Command::raw`** — fallback for unsupported extended-protocol
  SQL when you still want typed parameters, typed rows, prepare support, or
  streaming
- **`sql!`** — lower-level fragment builder for named-placeholder composition
  and fragment nesting; useful, but secondary to schema-aware typed SQL
- **`simple_query_raw`** — simple-protocol escape hatch for raw SQL strings,
  especially multi-statement bootstrap/migration work or commands where you do
  not need typed params/results. It does not participate in typed prepared or
  streaming flows.

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

## Choosing a Rust Postgres tool

Different Rust data-access libraries optimize for different trade-offs. `babar`
is aimed at teams that want a Postgres-specific client with explicit typed query
values, explicit codecs, and early validation around prepare-time schema drift.

| If you care most about... | `babar` | `sqlx` | `tokio-postgres` |
| --- | --- | --- | --- |
| Database scope | Postgres only | Postgres, MySQL, SQLite, MSSQL | Postgres only |
| Query model | Typed runtime `Query<P, R>` / `Command<P>` values | Raw SQL plus compile-time macros | Raw SQL strings plus codec traits |
| Compile-time SQL checking | Optional, online-only macros | Strongest emphasis here, including offline workflows | Minimal |
| Runtime explicitness | Very explicit codecs and row shapes | More macro- and trait-driven | More trait-driven |
| Feature coverage / maturity today | Focused `0.1` surface | Broad ecosystem and tooling | Most battle-tested async Postgres driver in Rust |
| Best fit | Postgres-specific apps that want explicit typed values | Teams prioritizing compile-time SQL workflows or multi-database support | Teams prioritizing mature Postgres coverage and established operational history |

None of those are "wrong" choices. If your team prefers compile-time SQL by
default, `sqlx` is a strong fit. If you need the widest async Postgres feature
coverage today, `tokio-postgres` remains the reference point. If you want a
single Postgres-focused API where query shape and codec shape stay visible in
the types, `babar` is designed for that workflow.

## Status

`babar` `0.1.0` is now published on crates.io alongside `babar-macros`, and the
book is published via GitHub Pages at
[`https://babar.notthatbreezy.io`](https://babar.notthatbreezy.io). `RELEASE.md`
remains the maintenance runbook for future releases.
