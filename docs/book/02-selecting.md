# 2. Selecting

In this chapter we'll go from a connected `Session` to typed Rust
values: a `SELECT`, a decoder tuple, and a `Vec<B>` you can iterate.

## Setup

```rust
use babar::codec::{bool, int4, nullable, text};
use babar::query::Query;
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch02-selecting"),
    )
    .await?;

    // No parameters; one row of three columns.
    let q: Query<(), (i32, String, bool)> = Query::raw(               // type: Query<(), (i32, String, bool)>
        "SELECT 1::int4 AS id, 'alice'::text AS name, true AS active",
        (),
        (int4, text, bool),
    );

    let rows: Vec<(i32, String, bool)> = session.query(&q, ()).await?; // type: Vec<(i32, String, bool)>
    for (id, name, active) in &rows {
        println!("{id}\t{name}\t{active}");
    }

    session.close().await?;
    Ok(())
}
```

## The shape of a query

Every `Query<A, B>` carries two type parameters:

- `A` — the *parameter* tuple you bind at call time. `()` if there are
  no `$N` placeholders.
- `B` — the *row* tuple you'll get back, one per row.

The codec tuple at the end of `Query::raw` decides `B`. `(int4, text,
bool)` decodes columns into `(i32, String, bool)`. There is no
intermediate `Row` type and no `.get::<T, _>()` accessor: by the time
`session.query(...).await?` returns, the bytes are already typed
values.

## Nullable columns

Postgres columns are nullable by default. babar refuses to guess: if
the column might be NULL, wrap its codec in `nullable(...)` and let
the row tuple use `Option<T>`.

```rust
use babar::codec::{int4, nullable, text};

let q: Query<(), (i32, Option<String>)> = Query::raw(
    "SELECT id, note FROM users ORDER BY id",
    (),
    (int4, nullable(text)),
);
```

If you forget the `nullable(...)` wrapper and Postgres sends a NULL,
the codec returns a clear decode error rather than a panic or a silent
`String::default()`. For example, decoding the `note` column as plain
`text` against a row where `note IS NULL`:

```rust
use babar::codec::{int4, text};

// Wrong: `text` (not `nullable(text)`) and `String` (not `Option<String>`).
let q: Query<(), (i32, String)> = Query::raw(
    "SELECT id, note FROM users WHERE id = 1",
    (),
    (int4, text),
);

match session.query(&q, ()).await {
    Ok(rows) => println!("{rows:?}"),
    Err(e) => eprintln!("decode failed: {e}"),
}
```

…prints something like:

```text
decode failed: decode error at column 1 ("note"): unexpected NULL for non-nullable codec `text`;
  wrap it in `nullable(text)` and decode into `Option<String>`
```

The fix is the one-line change shown above: swap `text` for `nullable(text)`
and `String` for `Option<String>` in the row tuple. babar would rather make
you spell it out than quietly hand you an empty string.

## Multiple rows

`session.query(&q, args)` always returns `Vec<B>` — one tuple per row,
in server order. For one-row reads it's perfectly idiomatic to write:

```rust
let row = session.query(&q, (id,)).await?.into_iter().next();
```

…and treat `None` as "no such row". For large result sets, prefer
streaming — see [Chapter 4](./04-prepared-and-streaming.md).

## When a row doesn't fit your tuple

If your decoder asks for `(i32, String)` but the SQL returns three
columns, decoding fails with a clear `Error::Codec(_)`. Make the
column list explicit (`SELECT id, name FROM ...`) so the row shape and
the codec tuple stay in lockstep — `SELECT *` is allowed but a
liability for typed code.

## Next

[Chapter 3: Parameterized commands](./03-parameterized-commands.md)
introduces `Command<A>`, the `sql!` macro, and the `Encoder<A>` /
`Decoder<A>` traits at a user level.
