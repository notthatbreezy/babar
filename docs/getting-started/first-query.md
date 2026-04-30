# Your first query

This guide gets one complete round-trip working: connect to Postgres, create a
small table, insert a row, and read typed rows back with babar's primary SQL
surface.

## Setup

Add `babar` and Tokio to your `Cargo.toml`, then put this in `src/main.rs`.

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
}

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
    let cfg = Config::new("localhost", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("first-query");

    let session: Session = Session::connect(cfg).await?;

    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE demo_users (id int4 PRIMARY KEY, name text NOT NULL)");
    session.execute(&create, ()).await?;

    let insert: Command<NewUser> =
        app_schema::command!(INSERT INTO demo_users (id, name) VALUES ($id, $name));
    session
        .execute(
            &insert,
            NewUser {
                id: 1,
                name: "Ada".to_string(),
            },
        )
        .await?;

    let users: Query<(), UserRow> = app_schema::query!(
        SELECT demo_users.id, demo_users.name
        FROM demo_users
        ORDER BY demo_users.id
    );

    let rows: Vec<UserRow> = session.query(&users, ()).await?;
    for row in &rows {
        println!("id={} name={}", row.id, row.name);
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

## What to notice

### `Config` is explicit

`Config::new(host, port, user, database)` takes the required connection fields by
position. Optional settings are chained on after that:
`.password(...)`, `.application_name(...)`, `.connect_timeout(...)`, and more.

### `Session` is the connection handle

`Session::connect(cfg)` opens one Postgres connection and starts babar's
background driver task for it. Public calls on `Session` are cancellation-safe:
dropping the waiting future does not leave the wire protocol half-consumed.

### `schema!` gives application SQL a typed home

`babar::schema! { ... }` records schema facts in Rust and generates
schema-scoped wrappers like `app_schema::query!(...)` and
`app_schema::command!(...)`.

That is the main path for application SQL:

- write schema facts once
- use named placeholders like `$id`
- let the macro infer the parameter and row shapes

### `query!` and `command!` define runtime values

The important types are still `Query<A, B>` and `Command<A>`:

- `A` is the Rust value you bind when the statement runs
- `B` is the per-row Rust value returned by a query

In the example above:

- `Command<NewUser>` inserts a `NewUser`
- `Query<(), UserRow>` returns `Vec<UserRow>`

There is no intermediate `Row` object and no `.get::<T, _>()` step after the
query finishes. By the time `session.query(...).await?` returns, the bytes are
already decoded into `UserRow` values.

### Raw SQL is explicit

The setup `CREATE TEMP TABLE` uses `Command::raw(...)` because DDL sits outside
babar's schema-aware typed-SQL subset.

When raw SQL still needs explicit parameters or row decoders, use the `_with`
constructors:

- `Command::raw_with(sql, encoder)`
- `Query::raw(sql, decoder)`
- `Query::raw_with(sql, encoder, decoder)`

That keeps the two paths easy to read:

- schema-aware macros for normal application SQL
- raw builders for bootstrap and advanced cases

### Optional compile-time verification is available

If `BABAR_DATABASE_URL` or `DATABASE_URL` is set at macro expansion time,
schema-aware `SELECT` queries can be checked against a live Postgres server.
That verification confirms authored schema facts, placeholders, and projected
columns for supported `query!` calls.

Without that environment variable, the same code still expands into the same
runtime `Query` / `Command` values.

## Next

Continue with [Chapter 1: Connecting](../book/01-connecting.md) for more on
connection settings, shutdown, and what the background driver task owns.
