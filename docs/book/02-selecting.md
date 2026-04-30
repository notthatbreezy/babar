# 2. Selecting

In this chapter we'll go from a connected `Session` to typed Rust values:
authored schema facts, a schema-aware `SELECT`, and the `Vec<B>` returned by
`session.query`.

## Setup

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};

babar::schema! {
    mod app_schema {
        table users {
            id: primary_key(int4),
            name: text,
            active: bool,
            note: nullable(text),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch02-selecting"),
    )
    .await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE users (
            id int4 PRIMARY KEY,
            name text NOT NULL,
            active bool NOT NULL,
            note text
         )",
        (),
    );
    session.execute(&create, ()).await?;

    let insert: Command<(i32, String, bool, Option<String>)> =
        app_schema::command!(INSERT INTO users (id, name, active, note) VALUES ($id, $name, $active, $note));
    session
        .execute(&insert, (1, "alice".to_string(), true, Some("first".to_string())))
        .await?;

    let q: Query<(bool,), (i32, String, bool)> = app_schema::query!(
        SELECT users.id, users.name, users.active
        FROM users
        WHERE users.active = $active
        ORDER BY users.id
    );

    let rows: Vec<(i32, String, bool)> = session.query(&q, (true,)).await?;
    for (id, name, active) in &rows {
        println!("{id}\t{name}\t{active}");
    }

    session.close().await?;
    Ok(())
}
```

## The shape of a query

Every `Query<A, B>` carries two type parameters:

- `A` — the parameter tuple you bind at call time
- `B` — the per-row output type you get back

There is no intermediate `Row` type and no `.get::<T, _>()` accessor: by the
time `session.query(...).await?` returns, the bytes are already typed Rust
values.

`query!` is the default path to that `Query<A, B>` value. The recommended
reusable pattern is a Rust-visible schema module plus its schema-scoped wrapper:

```rust
use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        }
    }
}

let q: Query<(i32,), (i32, String)> = app_schema::query!(
    SELECT users.id, users.name
    FROM users
    WHERE users.id = $id AND users.active = true
);
```

For one-off examples or tests, inline schema works too:

```rust
use babar::query::Query;

let q: Query<(i32,), (i32, String)> = babar::query!(
    schema = {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    },
    SELECT users.id, users.name
    FROM users
    WHERE users.id = $id AND users.active = true
);
```

`typed_query!` still exists as a compatibility alias to the same compiler, but
new docs and new code should prefer `query!`.

## Supported subset and explicit non-goals

The schema-aware compiler is intentionally narrow in v1:

- exactly one statement per macro call
- named placeholders like `$id`, with repeated names reusing the same slot
- explicit optional ownership markers only where supported:
  `$value?` for a direct `WHERE` / `JOIN` comparison or the full `LIMIT` /
  `OFFSET` expression, `(...)?` for a whole parenthesized `WHERE` / `JOIN`
  predicate or a single `ORDER BY` expression
- authored Rust schema only — no file-based schema input, codegen, or offline cache

Authored schema declarations accept `bool`, `bytea`, `varchar`, `text`, `int2`,
`int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`, `timestamp`,
`timestamptz`, `json`, `jsonb`, and `numeric`. Schema-aware typed SQL now lowers
inferred parameters and projected rows across that same family, including
nullable variants. The matching babar feature still needs to be enabled for
optional families such as `uuid`, `time`, `json`, and `numeric`.

This is not a general SQL rewrite engine or ORM layer. Unsupported statements
should fall back to `Query::raw` / `Command::raw`.

## Nullable columns

Postgres columns are nullable by default. In authored schemas, mark them
explicitly with `nullable(...)` so the inferred row type becomes `Option<T>`:

```rust
use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            note: nullable(text),
        }
    }
}

let q: Query<(), (i32, Option<String>)> = app_schema::query!(
    SELECT users.id, users.note
    FROM users
    ORDER BY users.id
);
```

If you use `Query::raw` instead, you must keep the decoder tuple in sync
yourself by pairing nullable columns with `nullable(codec)` and `Option<T>`.
babar would rather force that explicitness than guess.

## Multiple rows

`session.query(&q, args)` always returns `Vec<B>` — one tuple per row, in
server order. For one-row reads it's perfectly idiomatic to write:

```rust
let row = session.query(&q, (id,)).await?.into_iter().next();
```

…and treat `None` as "no such row". For large result sets, prefer streaming —
see [Chapter 4](./04-prepared-and-streaming.md).

## When to reach for `Query::raw`

`Query::raw` is the typed fallback when the SQL you need is outside the current
typed SQL subset but you still want the extended protocol, typed params, typed
rows, prepare support, or streaming. `simple_query_raw` is lower-level still:
it sends a raw SQL string through PostgreSQL's simple-query protocol and is best
reserved for bootstrap or multi-statement work.

## Next

[Chapter 3: Parameterized commands](./03-parameterized-commands.md) introduces
`Command<A>`, the migration path from the old explicit-codec macros, and where
`sql!`, raw statements, and simple-query fallbacks fit.
