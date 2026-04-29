# Your first query

In this chapter we'll connect to a Postgres server, run a single query, 
and decode the response into Rust values you can pattern-match
on. Three values do the work: a `Config`, a `Query`, and a `Session`.

## Setup

Add `babar` and a Tokio runtime to your `Cargo.toml`, then drop the
following into `src/main.rs`.

```rust
use babar::codec::{int4, text};
use babar::query::Query;
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    // 1. Describe the connection.
    let cfg = Config::new("localhost", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("first-query");

    // 2. Open a Session. The Session owns one Postgres connection.
    let session: Session = Session::connect(cfg).await?;        // type: Session

    // 3. Build a typed Query. () means "no parameters"; the codec
    //    tuple at the end describes each column in the result row.
    let q: Query<(), (i32, String)> = Query::raw(               // type: Query<(), (i32, String)>
        "SELECT 1::int4 AS id, 'Ada'::text AS name",
        (),
        (int4, text),
    );

    // 4. Run it. `query` returns Vec<B> — one decoded tuple per row.
    let rows: Vec<(i32, String)> = session.query(&q, ()).await?; // type: Vec<(i32, String)>

    for (id, name) in &rows {
        println!("id={id} name={name}");
    }

    session.close().await?;
    Ok(())
}
```

Run it with a Postgres reachable on `localhost:5432`:

```text
cargo run
# id=1 name=Ada
```

## Breaking this down

**`Config::new(host, port, user, database)`** is a constructor
that takes the four required fields by position. Optional fields are
chained on after: `.password(...)`, `.application_name(...)`,
`.connect_timeout(...)`. There is no `Config::from_env()` and no DSN
parser — `Config` is a plain struct, and you set its fields.

**`Session::connect(cfg)`** returns a `Session`. A `Session` owns one
Postgres connection plus a background task that owns the socket. Every
method you call on `Session` is cancellation-safe: dropping the future
won't leave the connection half-spoken-to.

**`Query<(), (i32, String)>`** is the heart of the typed surface. The
two type parameters are the *input* (parameters you bind) and the
*output* (the row shape after decoding). Here we pass `()` because the
SQL has no parameters, and `(i32, String)` because the codec tuple
`(int4, text)` decodes each row into `(i32, String)`.

**`Query::raw(sql, encoder, decoder)`** is the most direct way to build
a `Query`, which is why this guide starts there. The `sql!` macro
produces a different thing — a `Fragment` that knows about named
placeholders — and you'd build a `Query` from it with
`Query::from_fragment(fragment, decoder)`. The chain is always:
**fragment → query → run**. You cannot pass a `Fragment` straight to
`session.query`.

There is also a newer `typed_query!` path: a query-only, narrow
schema-aware macro that takes token-style `SELECT` input plus authored
schema facts and expands directly to a `Query<P, R>`. For one-off uses
you can keep the schema inline; for reusable read models the recommended
pattern is `babar::schema! { ... }` plus a schema-scoped
`app_schema::typed_query!(...)` wrapper. Its supported subset also
includes explicit optional ownership markers such as `$value?` and
`(...)?`, but for a first read the explicit `Query::raw(...)` form
remains the best foundation.

**`session.query(&q, args)`** is the run step. It returns
`Vec<B>` — fully decoded rows, where each `B` is whatever your decoder
tuple produces. babar does not expose an intermediate `Row` type and
there is no `.get::<T, _>()` accessor: by the time you have the `Vec`,
the bytes are already typed Rust values.

## What happened

You connected to PostgreSQL via the wire protocol, prepared a statement, bound zero
parameters, fetched one row, decoded `int4` into `i32` and `text` into
`String`, and closed the session.

## Next

Head into [Chapter 1: Connecting](../book/01-connecting.md) to see what
else lives on `Config`, what the background driver task is doing, and
how to recover when the server is unreachable.
