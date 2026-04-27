# The Book of Babar

*Ergonomic Postgres for Rust.*
*Typed, async, no surprises.*

![The Babar brand sheet — wordmark, palette, and the herd at work](assets/img/babar-brand-sheet.png)

`babar` is a typed, async Postgres driver for Tokio that speaks the wire
protocol directly. No `libpq`. No magic. Just queries, codecs, and clear
errors — composed the way you'd compose any other Rust value.

```
cargo add babar
```

## Why babar

| Pillar | Headline | What you get |
|---|---|---|
| **Ergonomic by Design** | Read it once, understand it forever. | Queries are typed values. Codecs are imported by name. There is one way to start a transaction, one way to bind a parameter, one way to run a migration. |
| **Postgres at Heart** | The wire protocol, faithfully. | Extended-protocol prepares, binary results, SCRAM-SHA-256, channel binding over TLS, and binary `COPY FROM STDIN` for bulk ingest. No translation layer between you and the server. |
| **Built for the Herd** | Predictable under load. | A single background task owns the socket and serializes wire I/O, so every public call is cancellation-safe. Pool, statement cache, and `tracing` spans are first-class — not bolted on later. |

## Connect, type, query

Three values: a `Config`, a `Command`, and a `Query`. Codecs come in by
name so the compiler can read your intent.

```rust
use babar::codec::{int4, text};
use babar::query::Query;
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(           // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("secret")
            .application_name("hello-babar"),
    )
    .await?;

    let select: Query<(), (i32, String)> =             // type: Query<(), (i32, String)>
        Query::raw(
            "SELECT 1::int4 AS id, 'Ada'::text AS name",
            (),
            (int4, text),
        );

    let rows: Vec<(i32, String)> = session.query(&select, ()).await?; // type: Vec<(i32, String)>
    println!("{rows:?}");

    session.close().await?;
    Ok(())
}
```

You wrote three things: a `Config` describing where to connect, a
`Query<A, B>` describing the round-trip (parameters in, rows out), and
the call that ties them together. The codec tuple `(int4, text)`
**is** the schema of the rows you'll get back.

## Where to go next

- **[Your first query →](getting-started/first-query.md)** — the same
  flow, walked one line at a time, with a real Postgres handy.
- **[The Book of Babar →](book/01-connecting.md)** — thirteen short
  chapters covering connecting, querying, transactions, pooling, COPY,
  migrations, errors, codecs, web services, TLS, and observability.
- **[Reference →](reference/codecs.md)** — codec catalog, error
  catalog, feature flags, configuration knobs.
- **[Why babar →](explanation/why-babar.md)** — the design notes.
