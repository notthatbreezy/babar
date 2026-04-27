# 3. Parameterized commands

In this chapter we'll bind parameters, write to the database, and meet
the `Encoder<A>` / `Decoder<A>` codec traits behind the scenes.

## Setup

```rust
use babar::codec::{bool, int4, text};
use babar::query::{Command, Query};
use babar::{sql, Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                              // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch03-params"),
    )
    .await?;

    // CREATE TABLE — no parameters, no rows back.
    let create: Command<()> = Command::raw(                               // type: Command<()>
        "CREATE TEMP TABLE todo (id int4 PRIMARY KEY, title text NOT NULL, done bool NOT NULL DEFAULT false)",
        (),
    );
    session.execute(&create, ()).await?;

    // INSERT — bind (i32, String).
    let insert: Command<(i32, String)> = Command::raw(                    // type: Command<(i32, String)>
        "INSERT INTO todo (id, title) VALUES ($1, $2)",
        (int4, text),
    );
    session.execute(&insert, (1, "buy milk".into())).await?;

    // UPDATE — bind one parameter; capture rows-affected.
    let mark_done: Command<(i32,)> = Command::raw(
        "UPDATE todo SET done = true WHERE id = $1",
        (int4,),
    );
    let affected: u64 = session.execute(&mark_done, (1,)).await?;
    println!("updated {affected} row(s)");

    // SELECT it back, this time with the sql! macro and named placeholders.
    let lookup: Query<(bool,), (i32, String, bool)> =
        Query::from_fragment(
            sql!(
                "SELECT id, title, done FROM todo WHERE done = $done ORDER BY id",
                done = bool,
            ),
            (int4, text, bool),
        );
    for (id, title, done) in session.query(&lookup, (true,)).await? {
        println!("{id}\t{title}\t{done}");
    }

    session.close().await?;
    Ok(())
}
```

## `Command<A>` vs `Query<A, B>`

A `Command<A>` describes a round-trip that *doesn't* return rows —
DDL, INSERT, UPDATE, DELETE. `session.execute(&cmd, args).await?`
returns a `u64` rows-affected count.

A `Query<A, B>` describes a round-trip that returns typed rows.
`session.query(&q, args).await?` returns `Vec<B>`.

Both take the same `A` type parameter for parameters: a tuple of
encoders for `Command::raw` / `Query::raw`, or a fragment that knows
its own parameter shape if you use the `sql!` macro.

## Two ways to spell the SQL

### `Command::raw` and `Query::raw`

The most direct form. You write Postgres positional placeholders
(`$1`, `$2`, …) and pass an explicit codec tuple in matching order.
This is what the `todo_cli` example uses.

### The `sql!` macro

`sql!` lets you write *named* placeholders (`$id`, `$title`) and pair
each name with its codec inline. It produces a `Fragment<A>` whose
parameter type `A` is derived from the names you used. Then you wrap
the fragment in either `Command::from_fragment(...)` or
`Query::from_fragment(fragment, decoder_tuple)` to get the runnable
value:

```rust
let f = sql!(
    "INSERT INTO todo (id, title) VALUES ($id, $title)",
    id = int4,
    title = text,
);
let insert: Command<(i32, String)> = Command::from_fragment(f);
```

A `Fragment` on its own is *not* runnable — you cannot call
`session.execute(sql!(...))` or `session.query(sql!(...))` directly.
The chain is always **fragment → command/query → run**.

## What the codec types are doing

When you write `(int4, text)` you're constructing a tuple of
`Encoder<A>` / `Decoder<A>` values. Each one knows two things:

- the Postgres OID it speaks for (`int4` ↔ OID 23, `text` ↔ OID 25),
- how to encode/decode that OID's binary representation to/from its
  Rust counterpart (`i32`, `String`, …).

The `Encoder<A>` trait turns a Rust `A` into the parameter byte
buffer; the `Decoder<A>` trait turns one column's bytes back into a
Rust `A`. Both traits are generic over the value type, which is why
the row tuple in `Query<(), (i32, String, bool)>` is the codec
tuple's value-type, not some opaque `Row` shape.

Codecs you'll reach for first: `int4`, `int8`, `text`, `bool`,
`bytea`, `float4`, `float8`, `nullable(c)`. The full set lives in
`babar::codec`; we walk through them in
[reference/codecs.md](../reference/codecs.md).

## Next

[Chapter 4: Prepared queries & streaming](./04-prepared-and-streaming.md)
shows how to prepare a statement once, run it many times, and stream
results in batches.
