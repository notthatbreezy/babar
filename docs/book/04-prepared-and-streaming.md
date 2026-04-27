# 4. Prepared queries & streaming

In this chapter we'll prepare a statement on the server, run it many
times without re-parsing, and stream a large result set in batches
instead of buffering it all into a `Vec`.

## Setup

```rust
use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Session};
use futures_util::StreamExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch04-prepared"),
    )
    .await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE prepared_demo (id int4 PRIMARY KEY, title text NOT NULL)",
        (),
    );
    session.execute(&create, ()).await?;

    // Prepare once, execute five times.
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO prepared_demo (id, title) VALUES ($1, $2)",
        (int4, text),
    );
    let prepared = session.prepare_command(&insert).await?;           // type: PreparedCommand<(i32, String)>
    for (id, title) in [(1, "alpha"), (2, "beta"), (3, "gamma"), (4, "delta"), (5, "epsilon")] {
        prepared.execute((id, title.into())).await?;
    }
    prepared.close().await?;

    // Stream the full table in batches of 2.
    let scan: Query<(), (i32, String)> = Query::raw(
        "SELECT id, title FROM prepared_demo ORDER BY id",
        (),
        (int4, text),
    );
    let mut rows = session.stream_with_batch_size(&scan, (), 2).await?;
    while let Some(row) = rows.next().await {
        let (id, title) = row?;                                       // type: (i32, String)
        println!("streamed {id}: {title}");
    }

    session.close().await?;
    Ok(())
}
```

## `prepare_command` and `prepare_query`

When you call `session.prepare_command(&cmd).await?` (or
`prepare_query` for a `Query<A, B>`), babar sends `Parse` once and
gets back a server-side prepared statement that you can call as many
times as you want. Each call avoids the `Parse` round-trip — the
server already has the plan, the parameter OIDs, and the result
description cached.

The prepared handle exposes the same `execute(args)` / `query(args)`
methods you'd use on `Session`, just bound to that one statement. When
you're done, call `.close().await` to release the server-side name —
or drop the handle and the next prepared statement under the same
name will replace it.

## Streaming with `stream_with_batch_size`

For result sets that don't fit comfortably in memory, swap
`session.query` for `session.stream_with_batch_size(&q, args, n)`. It
returns a `RowStream<B>` (an `impl Stream<Item = babar::Result<B>>`)
that pulls rows from the server `n` at a time using a Postgres portal.

A few notes you'll want in your back pocket:

- **Back-pressure is real**. The driver task only fetches the next
  batch when the consumer pulls. If you stop polling the stream, the
  server stops sending rows; nothing buffers indefinitely on either
  side.
- **Cancellation is safe**. Dropping the stream or `tokio::select!`ing
  away closes the portal cleanly. The `Session` is ready for its next
  call as soon as the portal close completes.
- **Each `Item` is `Result<B, Error>`**. Decode errors surface
  per-row, so you can recover from a single bad row without losing the
  rest of the batch.

## When to prepare, when to stream

| Pattern | Use it for |
|---|---|
| `Command::raw` / `Query::raw` + `session.execute` / `session.query` | One-shot statements, ad hoc queries. |
| `prepare_command` / `prepare_query` + repeated `execute` / `query` | Hot paths called many times with different parameters. |
| `stream_with_batch_size` | Result sets larger than you want to materialize at once. |

## Next

[Chapter 5: Transactions](./05-transactions.md) introduces
`Session::transaction()` and how to compose all of the above inside
`BEGIN` / `COMMIT`.
