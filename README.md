# babar

Typed, async PostgreSQL driver for Tokio that speaks the PostgreSQL wire protocol directly.

`babar` is intentionally explicit: queries and commands are typed values, codecs are imported values, SQL composition is opt-in via `sql!`, and a background driver task owns the socket so public API calls stay cancellation-safe.

## Highlights

- direct wire-protocol implementation on Tokio — no `libpq`, no `tokio-postgres`
- typed `Query`, `Command`, `PreparedQuery`, `PreparedCommand`, `Transaction`/`Savepoint`, and `Pool` APIs
- typed binary `CopyIn<T>` for `COPY FROM STDIN` bulk ingest from `Vec<T>` / iterators
- SQL composition with `sql!` and `#[derive(Codec)]`
- rich errors with SQLSTATE fields plus SQL/caret rendering
- OpenTelemetry-friendly `tracing` spans: `db.connect`, `db.prepare`, `db.execute`, `db.transaction`
- TLS via `rustls` (default) or `native-tls`

## Feature matrix

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

`babar` ships a dedicated typed API for **binary `COPY FROM STDIN`** bulk ingest:

```rust,no_run
use babar::{CopyIn, Session};
use babar::query::Query;
use babar::Config;

#[derive(Clone, Debug, PartialEq, babar::Codec)]
struct UserRow {
    #[pg(codec = "int4")]
    id: i32,
    #[pg(codec = "text")]
    email: String,
    #[pg(codec = "nullable(text)")]
    note: Option<String>,
}

# async fn demo() -> babar::Result<()> {
let session = Session::connect(
    Config::new("localhost", 5432, "postgres", "postgres").password("secret"),
)
.await?;

session
    .simple_query_raw(
        "CREATE TEMP TABLE copy_users (id int4 PRIMARY KEY, email text NOT NULL, note text)",
    )
    .await?;

let rows = vec![
    UserRow { id: 1, email: "ada@example.com".into(), note: Some("first".into()) },
    UserRow { id: 2, email: "bob@example.com".into(), note: None },
];

let copy = CopyIn::binary(
    "COPY copy_users (id, email, note) FROM STDIN BINARY",
    UserRow::CODEC,
);
session.copy_in(&copy, rows).await?;

let select: Query<(), UserRow> = Query::raw(
    "SELECT id, email, note FROM copy_users ORDER BY id",
    (),
    UserRow::CODEC,
);
assert_eq!(session.query(&select, ()).await?.len(), 2);
session.close().await?;
# Ok(())
# }
```

The COPY surface is intentionally limited to bulk ingest with binary `COPY FROM STDIN`. `COPY TO`, text COPY, and CSV COPY are not implemented.

## Examples

Real-world example programs live in `crates/core/examples/`:

- `quickstart` — smallest typed end-to-end example
- `prepared_and_stream` — prepared statements plus streaming
- `transactions` / `pool` — M4 lifecycle walkthroughs
- `copy_bulk` — `Vec<Struct>` bulk ingest with `CopyIn<T>`
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
- no compile-time DB connectivity story to set up or cache
- SQL-origin-aware runtime errors with caret rendering

What `sqlx` does better:

- compile-time checked query macros
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
