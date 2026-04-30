# 3. Parameterized commands

This chapter covers write statements in babar: how `Command<A>` differs from
`Query<A, B>`, how schema-scoped `command!` handles the common path, and where
`sql!` plus the raw builders fit when you need a lower-level tool.

## Setup

```rust
use babar::query::{Command, Query};
use babar::{Config, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewTodo {
    id: i32,
    title: String,
    done: bool,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct TodoId {
    id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct TodoFilter {
    done: bool,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct TodoRow {
    id: i32,
    title: String,
    done: bool,
}

babar::schema! {
    mod todo_schema {
        table todo {
            id: primary_key(int4),
            title: text,
            done: bool,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch03-params"),
    )
    .await?;

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE todo (
            id int4 PRIMARY KEY,
            title text NOT NULL,
            done bool NOT NULL DEFAULT false
         )",
    );
    session.execute(&create, ()).await?;

    let insert: Command<NewTodo> =
        todo_schema::command!(INSERT INTO todo (id, title, done) VALUES ($id, $title, $done));
    session
        .execute(
            &insert,
            NewTodo {
                id: 1,
                title: "buy milk".into(),
                done: false,
            },
        )
        .await?;

    let mark_done: Command<TodoId> =
        todo_schema::command!(UPDATE todo SET done = true WHERE todo.id = $id);
    let affected: u64 = session.execute(&mark_done, TodoId { id: 1 }).await?;
    println!("updated {affected} row(s)");

    let lookup: Query<TodoFilter, TodoRow> = todo_schema::query!(
        SELECT todo.id, todo.title, todo.done
        FROM todo
        WHERE todo.done = $done
        ORDER BY todo.id
    );
    for row in session.query(&lookup, TodoFilter { done: true }).await? {
        println!("{}	{}	{}", row.id, row.title, row.done);
    }

    session.close().await?;
    Ok(())
}
```

## `Command<A>` vs `Query<A, B>`

A `Command<A>` describes a round-trip that does not return rows.
`session.execute(&command, args).await?` returns a `u64` affected-row count.

A `Query<A, B>` describes a round-trip that returns typed rows.
`session.query(&query, args).await?` returns `Vec<B>`.

Both keep the same mental model: one Rust value in, one typed database round-trip
out. The only difference is whether the server returns rows.

## The default path: schema-aware `query!` / `command!`

Public `query!` and `command!` are the main typed-SQL entrypoints. They accept
inline schema for one-off use, but the reusable pattern is a `schema!` module and
its schema-scoped wrappers.

```rust
use babar::query::Command;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewTodo {
    id: i32,
    title: String,
}

let insert: Command<NewTodo> = babar::command!(
    schema = {
        table public.todo {
            id: primary_key(int4),
            title: text,
        },
    },
    INSERT INTO todo (id, title) VALUES ($id, $title)
);
```

Or, with a reusable schema module:

```rust
use babar::query::Query;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct TodoId {
    id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct TodoPreview {
    id: i32,
    title: String,
}

babar::schema! {
    mod todo_schema {
        table public.todo {
            id: primary_key(int4),
            title: text,
            done: bool,
        }
    }
}

let preview: Query<TodoId, TodoPreview> = todo_schema::query!(
    SELECT todo.id, todo.title
    FROM todo
    WHERE todo.id = $id AND todo.done = false
);
```

Schema-aware typed SQL stays intentionally narrow:

- exactly one statement per macro call
- authored Rust schema only
- supported writes include `INSERT ... VALUES`, `UPDATE ... WHERE`, and
  `DELETE ... WHERE`
- `RETURNING` remains explicit and row-shaped
- optional ownership forms stay explicit: `$value?` and `(...)?`

Unsupported constructs fall back to raw SQL rather than expanding the macro
surface into a general query builder or ORM.

## Optional verification during macro expansion

If `BABAR_DATABASE_URL` or `DATABASE_URL` is set, supported schema-aware `SELECT`
statements can be checked against a live Postgres server during macro expansion.
That validation confirms schema facts, placeholders, and projected columns.

`command!` still expands into the same runtime `Command<A>` values and shares the
same authored-schema pipeline; it just does not currently participate in that
live verification hook.

For a technical walk through of how `schema!`, `query!`, and `command!` lower into
runtime values, see [The typed-SQL macro pipeline](../explanation/typed-sql-macro-pipeline.md).

## `sql!` is the lower-level fragment builder

`sql!` is the tool for composing SQL fragments with named placeholders. It is not
a runnable statement by itself.

```rust
use babar::codec::{bool, int4, text};
use babar::query::Query;

let titles: Query<(i32, bool), (String,)> = babar::sql!(
    "SELECT title FROM todo WHERE ($predicate) AND done = $done",
    predicate = babar::sql!("id = $id", id = int4),
    done = bool,
)
.query((text,));
```

The shape is always:

```text
fragment -> command/query -> run
```

Reach for `sql!` when you need fragment composition or when authored schema is
not the right abstraction for the SQL you are assembling.

## Raw builders

Use the raw constructors when you want one explicit statement value without the
schema-aware macro layer:

- `Command::raw(sql)` — zero-parameter raw command
- `Command::raw_with(sql, encoder)` — parameterized raw command
- `Query::raw(sql, decoder)` — zero-parameter raw query
- `Query::raw_with(sql, encoder, decoder)` — parameterized raw query

These builders still use the extended protocol. They remain useful when you want
prepare support, typed parameters, typed rows, or streaming, but the statement is
outside the schema-aware subset.

`simple_query_raw` is the lower-level simple-protocol escape hatch for raw SQL
strings, especially multi-statement bootstrap or migration-style work.

## What the codec traits are doing

The raw builders and `sql!` operate on codec values. Each codec knows which
Postgres OIDs it speaks and how to encode or decode the binary representation for
that type.

- `Encoder<A>` turns a Rust `A` into parameter bytes.
- `Decoder<B>` turns one row into a Rust `B`.

Schema-aware macros generate statements that use the same codec machinery; they
just let authored schema facts and SQL tokens describe the shapes for you.

## Next

[Chapter 4: Prepared queries & streaming](./04-prepared-and-streaming.md) shows
how to prepare a statement once, execute it repeatedly, and stream the results.
