# 2. Selecting

This chapter shows the standard read path in babar: authored schema facts,
schema-scoped `query!`, a typed parameter value, and typed rows returned from
`session.query`.

## Setup

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct ActiveUsers {
    active: bool,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserSummary {
    id: i32,
    name: String,
    active: bool,
}

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
    );
    session.execute(&create, ()).await?;

    let insert: Command<NewUser> =
        app_schema::command!(INSERT INTO users (id, name, active) VALUES ($id, $name, $active));
    session
        .execute(
            &insert,
            NewUser {
                id: 1,
                name: "alice".to_string(),
                active: true,
            },
        )
        .await?;
    session
        .execute(
            &insert,
            NewUser {
                id: 2,
                name: "bert".to_string(),
                active: false,
            },
        )
        .await?;

    let active_users: Query<ActiveUsers, UserSummary> = app_schema::query!(
        SELECT users.id, users.name, users.active
        FROM users
        WHERE users.active = $active
        ORDER BY users.id
    );

    let rows: Vec<UserSummary> = session
        .query(&active_users, ActiveUsers { active: true })
        .await?;
    for row in &rows {
        println!("{}	{}	{}", row.id, row.name, row.active);
    }

    session.close().await?;
    Ok(())
}
```

## The shape of a query

Every `Query<A, B>` has two public-facing type parameters:

- `A` — the bound parameter value
- `B` — the decoded row value returned for each result row

That type is the contract for the round-trip. In the example above,
`Query<ActiveUsers, UserSummary>` means:

- call `session.query(&query, ActiveUsers { ... })`
- get back `Vec<UserSummary>`

`query!` is the main way to build that value. With authored schema facts, the
macro can infer both shapes directly from the SQL you wrote.

## Schema-scoped wrappers are the reusable pattern

A `schema!` module gives application SQL a stable namespace and lets you keep the
schema facts close to the code that depends on them.

```rust
use babar::query::Query;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserById {
    id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserSummary {
    id: i32,
    name: String,
}

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        }
    }
}

let user_by_id: Query<UserById, UserSummary> = app_schema::query!(
    SELECT users.id, users.name
    FROM users
    WHERE users.id = $id AND users.active = true
);
```

For one-off examples or tests, inline schema works too:

```rust
use babar::query::Query;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserById {
    id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserSummary {
    id: i32,
    name: String,
}

let user_by_id: Query<UserById, UserSummary> = babar::query!(
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

## Supported subset and explicit fallbacks

Schema-aware typed SQL stays intentionally small:

- exactly one statement per macro call
- named placeholders like `$id`, with repeated names reusing the same slot
- explicit optional forms only where supported: `$value?` and `(...)?`
- authored Rust schema only — no generated schema modules or offline cache

Supported authored column families include `bool`, `bytea`, `varchar`, `text`,
`int2`, `int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`,
`timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric`, plus nullable
variants. Feature-gated families such as `uuid`, `time`, `json`, and `numeric`
still require the matching Cargo feature.

When a statement sits outside that subset, use an explicit raw fallback:

- `Query::raw(sql, decoder)` for zero-parameter raw queries
- `Query::raw_with(sql, encoder, decoder)` for parameterized raw queries
- `Command::raw(sql)` and `Command::raw_with(sql, encoder)` for commands

## Nullable columns

Postgres columns are nullable by default. In authored schema, declare that with
`nullable(...)` so the inferred row shape becomes `Option<T>`.

```rust
use babar::query::Query;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserNote {
    id: i32,
    note: Option<String>,
}

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            note: nullable(text),
        }
    }
}

let notes: Query<(), UserNote> = app_schema::query!(
    SELECT users.id, users.note
    FROM users
    ORDER BY users.id
);
```

With raw SQL, you spell the same choice yourself through the decoder.

## Multiple rows

`session.query(&query, args)` always returns `Vec<B>` in server order. For a
one-row lookup, taking the first element is perfectly normal:

```rust
let user = session
    .query(&user_by_id, UserById { id: 7 })
    .await?
    .into_iter()
    .next();
```

For larger result sets, prepare once and stream rows — see
[Chapter 4](./04-prepared-and-streaming.md).

## When to reach for raw queries

Use raw queries when the SQL shape is correct for Postgres but outside the
schema-aware subset. Raw builders still keep typed parameters, typed rows,
prepare support, and streaming; they just ask you to provide the codecs
explicitly.

## Next

[Chapter 3: Parameterized commands](./03-parameterized-commands.md) covers write
statements, `sql!` as a lower-level fragment builder, and the raw-command
fallbacks.
