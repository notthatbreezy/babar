# 5. Transactions

In this chapter we'll wrap a sequence of statements in `BEGIN` /
`COMMIT`, recover from a partial failure with a savepoint, and let
babar's closure-based API decide when to commit and when to roll
back.

## Setup

```rust
use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Error, Savepoint, Session, Transaction};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch05-tx"),
    )
    .await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE tx_demo (id int4 PRIMARY KEY, note text NOT NULL)",
        (),
    );
    session.execute(&create, ()).await?;

    session.transaction(|tx: Transaction<'_>| async move {            // type: Transaction<'_>
        let insert: Command<(i32, String)> = Command::raw(
            "INSERT INTO tx_demo (id, note) VALUES ($1, $2)",
            (int4, text),
        );
        tx.execute(&insert, (1, "outer-before".into())).await?;

        // Savepoint that intentionally rolls back.
        let middle = tx.savepoint(|sp: Savepoint<'_>| async move {
            sp.execute(&insert, (2, "savepoint".into())).await?;
            Err::<(), _>(Error::Config("rolling back inner savepoint".into()))
        }).await;
        assert!(matches!(middle, Err(Error::Config(_))));

        tx.execute(&insert, (3, "outer-after".into())).await?;
        Ok(())
    }).await?;

    let select: Query<(), (i32, String)> = Query::raw(
        "SELECT id, note FROM tx_demo ORDER BY id",
        (),
        (int4, text),
    );
    for (id, note) in session.query(&select, ()).await? {
        println!("{id}: {note}");                                     // committed: 1, 3
    }

    session.close().await?;
    Ok(())
}
```

## `session.transaction` is closure-shaped

`Session::transaction(body)` takes an async closure that receives a
`Transaction<'_>`. babar opens the transaction with `BEGIN`, runs
your body, and:

- if the closure returns `Ok(_)` — commits.
- if the closure returns `Err(_)` — rolls back and surfaces your
  error.
- if the closure panics — rolls back and re-raises the panic.

You never write `COMMIT` or `ROLLBACK` yourself, and you can't forget
to. The borrow checker won't let you call methods on the underlying
`Session` while the `Transaction` is alive — there's exactly one
in-flight request on the connection at a time.

## Savepoints compose the same way

`tx.savepoint(body)` is the closure-shaped sibling for nested rollback
scopes. Same rules: `Ok` releases the savepoint, `Err` rolls back to
the savepoint and propagates the error. Savepoints can nest.

In the example above, the inner savepoint rolls back, but the outer
transaction continues and commits rows 1 and 3. Row 2 is gone — as if
the savepoint body had never run.

## Returning values from a transaction

The closure's `Ok` value is the transaction's return value:

```rust
let next_id: i32 = session.transaction(|tx| async move {
    let q: Query<(), (i32,)> = Query::raw(
        "SELECT COALESCE(MAX(id), 0) + 1 FROM tx_demo",
        (),
        (int4,),
    );
    Ok(tx.query(&q, ()).await?[0].0)
}).await?;
```

`tx` carries the same `execute` / `query` / `prepare_*` /
`stream_with_batch_size` methods you've used on `Session`, scoped to
the transaction. When the closure returns, babar commits and you get
your value.

## Errors and isolation

If a statement inside the body fails, the closure typically returns
`Err`, babar rolls back, and the transaction is gone. If you want to
*observe* an error and keep going, wrap that one statement in a
savepoint — the inner failure rolls the savepoint back without aborting
the outer transaction.

Isolation level isn't set by babar; if you need `SERIALIZABLE` or a
read-only transaction, run `SET TRANSACTION ...` as the first
statement in the body.

## Next

[Chapter 6: Pooling](./06-pooling.md) introduces `Pool`, which hands
you transaction-capable sessions from a pool of warm connections.
