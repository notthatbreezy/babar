# The Book of Babar

*Ergonomic Postgres for Rust.*
*Typed, async, no surprises.*

![The Babar brand sheet — wordmark, palette, and the herd at work](assets/img/babar-brand-sheet.png)

`babar` is a typed, async Postgres driver for Tokio that speaks the PostgreSQL wire
protocol directly. No `libpq`. No magic. Just queries, codecs, and clear
errors — composed the way you'd compose any other Rust value.

```
cargo add babar
```

## Why babar

| Pillar | Headline | What you get |
|---|---|---|
| **Ergonomic by Design** | Read it once, understand it forever. | Queries are typed values. Codecs are imported by name. There is one way to start a transaction, one way to bind a parameter, one way to run a migration. |
| **Postgres at Heart** | Why use any other database? | Extended-protocol prepares, binary results, SCRAM-SHA-256, channel binding over TLS, and binary `COPY FROM STDIN` for bulk ingest. No translation layer between you and the server. |
| **Built for the Herd** | Predictable under load. | A single background task owns the socket and serializes wire I/O, so every public call is cancellation-safe. Pool, statement cache, and `tracing` spans are first-class. |

## Connect, type, query

Three values: a `Config`, a `Command`, and a `Query`. The default SQL path is
schema-aware `query!` / `command!`; raw SQL stays available as an explicit
fallback.

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};

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
    let session: Session = Session::connect(           // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("secret")
            .application_name("hello-babar"),
    )
    .await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE demo_users (id int4 PRIMARY KEY, name text NOT NULL)",
        (),
    );
    session.execute(&create, ()).await?;

    let insert: Command<(i32, String)> =
        app_schema::command!(INSERT INTO demo_users (id, name) VALUES ($id, $name));
    session.execute(&insert, (1, "Ada".to_string())).await?;

    let select: Query<(), (i32, String)> =             // type: Query<(), (i32, String)>
        app_schema::query!(
            SELECT demo_users.id, demo_users.name
            FROM demo_users
            ORDER BY demo_users.id
        );

    let rows: Vec<(i32, String)> = session.query(&select, ()).await?; // type: Vec<(i32, String)>
    println!("{rows:?}");

    session.close().await?;
    Ok(())
}
```

You wrote three things:
  1. a `Config` describing where to connect
  2. a `Query<A, B>` describing the round-trip (parameters in, rows out)
  3. an authored schema module that drives `query!` / `command!`, while
     `Command::raw` stays available for unsupported setup SQL

## Where to go next

> **New here?** Read **[What makes babar babar →](explanation/what-makes-babar-babar.md)**
> first — a one-page tour of where babar sits and what makes it
> distinctive.

- **[Prerequisites →](getting-started/prerequisites.md)** — start a postgreSQL instance locally and see every query, command, and operation `babar` makes to the server.
- **[Your first query →](getting-started/first-query.md)** — make your first query, with explanations of every step broken down along the way.
- **[The Book of Babar →](book/01-connecting.md)** — covers everything A-Z when it comes to babary: connecting, querying, transactions, pooling, COPY,
  migrations, errors, codecs, web services, TLS, and observability.
- **[Reference →](reference/codecs.md)** — codec catalog, error catalog, feature flags, configuration knobs.
- **[Why babar →](explanation/why-babar.md)** — understand the philosophy behind the design and what makes babar different.
