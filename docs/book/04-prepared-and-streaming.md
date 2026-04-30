# 4. Prepared queries & streaming

In this chapter we'll prepare a statement on the server, run it many times
without re-parsing, and stream a large result set in batches instead of
buffering it all into a `Vec`.

## Setup

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};
use futures_util::StreamExt;

babar::schema! {
    mod app_schema {
        table prepared_demo {
            id: primary_key(int4),
            title: text,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(
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

    let insert: Command<(i32, String)> =
        app_schema::command!(INSERT INTO prepared_demo (id, title) VALUES ($id, $title));
    let prepared = session.prepare_command(&insert).await?;
    for (id, title) in [(1, "alpha"), (2, "beta"), (3, "gamma"), (4, "delta"), (5, "epsilon")] {
        prepared.execute((id, title.into())).await?;
    }
    prepared.close().await?;

    let scan: Query<(), (i32, String)> = app_schema::query!(
        SELECT prepared_demo.id, prepared_demo.title
        FROM prepared_demo
        ORDER BY prepared_demo.id
    );
    let mut rows = session.stream_with_batch_size(&scan, (), 2).await?;
    while let Some(row) = rows.next().await {
        let (id, title) = row?;
        println!("streamed {id}: {title}");
    }

    session.close().await?;
    Ok(())
}
```

## `prepare_command` and `prepare_query`

When you call `session.prepare_command(&cmd).await?` (or `prepare_query` for a
`Query<A, B>`), babar sends `Parse` once and gets back a server-side prepared
statement that you can call as many times as you want. Each call avoids the
`Parse` round-trip — the server already has the plan, parameter OIDs, and row
description cached.

The prepared handle exposes the same `execute(args)` / `query(args)` methods
you'd use on `Session`, just bound to one statement value. When you're done,
call `.close().await` to release the server-side name.

## Streaming with `stream_with_batch_size`

For result sets that don't fit comfortably in memory, swap `session.query` for
`session.stream_with_batch_size(&q, args, n)`. It returns a `RowStream<B>` that
pulls rows from the server `n` at a time using a Postgres portal.

A few things to note:

- **Back-pressure** — the driver only fetches the next batch when the consumer pulls
- **Cancellation is safe** — dropping the stream closes the portal cleanly
- **Each item is `Result<B, Error>`** — decode errors surface row-by-row

## Choosing the statement surface before you prepare or stream

Prepared statements and streaming work with the runnable `Command` / `Query`
values you hand to the session. That means the same surface ordering still
applies:

| Pattern | Use it for |
|---|---|
| `query!` / `command!` + `prepare_*` / `query` / `stream_*` | Default path for supported schema-aware typed SQL. |
| `Query::raw` / `Command::raw` + `prepare_*` / `query` / `stream_*` | Unsupported extended-protocol SQL where you still want typed params/rows. |
| `simple_query_raw` | Simple-protocol bootstrap or multi-statement raw SQL; not the path for prepared or streaming typed work. |

`sql!` stays available when you want fragment composition, but it is not a
prepared statement on its own. Convert it to a `Command` or `Query` first.

## Next

[Chapter 5: Transactions](./05-transactions.md) introduces
`Session::transaction()` and how to compose all of the above inside `BEGIN` /
`COMMIT`.
