# What makes babar babar

> See also: [Why babar](./why-babar.md),
> [Design principles](./design-principles.md), and
> [The typed-SQL macro pipeline](./typed-sql-macro-pipeline.md).

This page explains where babar sits, what its public API is optimizing for, and
which trade-offs stay visible in the design.

## Where babar sits

```text
┌─────────────────────────────────────────┐
│ your app                                │
├─────────────────────────────────────────┤
│ babar  (typed Query/Command values,     │
│         codecs, pool, COPY, migrations) │
├─────────────────────────────────────────┤
│ tokio  (TcpStream, tasks, cancellation) │
├─────────────────────────────────────────┤
│ Postgres wire protocol v3               │
└─────────────────────────────────────────┘
```

babar speaks the PostgreSQL wire protocol directly on top of Tokio. There is no
`libpq`, no other Rust Postgres client under the surface, and no generic
multi-database layer between your application and the server.

That keeps the exposed shapes recognizably Postgres-shaped: extended-protocol
prepares, binary results, SCRAM authentication, channel binding over TLS, and
binary `COPY FROM STDIN`.

## Four design choices that show up everywhere

### 1. One background driver task owns the socket

```rust
let session: Session = Session::connect(cfg).await?;
```

`Session` is a handle. The connection itself lives in a Tokio task started by
`Session::connect`. Public methods send requests to that task over channels and
wait for the reply.

That design does two things:

- it keeps public calls cancellation-safe
- it guarantees there is one writer to the socket, even when `Session` is cloned
  and shared across tasks

The [background driver task page](./driver-task.md) covers the runtime mechanics
in more detail.

### 2. Types describe the database boundary

`Query<A, B>` says which value shape goes in and which row shape comes back.
`Command<A>` says which value shape goes in when no rows come back.

```rust
use babar::query::{Command, Query};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
    parent_id: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserLookup {
    id: i32,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    name: String,
    parent_id: Option<i32>,
}

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            parent_id: nullable(int4),
        }
    }
}

let insert: Command<NewUser> =
    app_schema::command!(INSERT INTO users (id, name, parent_id) VALUES ($id, $name, $parent_id));

let select: Query<UserLookup, UserRow> = app_schema::query!(
    SELECT users.name, users.parent_id
    FROM users
    WHERE users.id = $id
);
```

Those statement values are plain Rust values. The type system prevents mixing up
row-returning and rowless operations, and it keeps parameter and row shapes
visible at the call site.

### 3. Schema-aware macros and explicit raw builders are separate tools

The main application path is authored schema plus schema-scoped `query!` /
`command!`. That keeps ordinary SQL concise and lets babar infer parameter and
row shapes from the statement.

The explicit fallback path stays available too:

- `Command::raw` / `Command::raw_with`
- `Query::raw` / `Query::raw_with`
- `sql!` for lower-level fragment composition

That split is intentional. babar does not try to hide which statements are in the
schema-aware typed-SQL subset and which ones need explicit codecs.

### 4. Validation happens as early as the API can make it happen

babar prefers to surface mismatches at the statement boundary.

- At bind time, the parameter shape is already part of the statement type.
- At prepare time, row decoders are checked against `RowDescription` so schema
  drift shows up before row decoding begins.
- At display time, server-positioned errors include SQL text and origin
  information so failures point back to the authored statement.
- At macro expansion time, supported schema-aware `SELECT` statements can be
  checked against a live database when `BABAR_DATABASE_URL` or `DATABASE_URL` is
  set.

The result is not “every bug is impossible.” The result is that several classes
of query-shape mistakes become impossible or fail earlier than runtime row
handling.

## What babar is deliberately not

- **Not multi-database.** babar is for Postgres.
- **Not synchronous.** The runtime model is async Tokio.
- **Not an ORM.** SQL stays visible.
- **Not a fluent AST builder.** babar keeps SQL text front and center.
- **Not a full migration platform.** It ships a focused migration runner instead
  of a separate migration product surface.

Those boundaries keep the API small and keep the implementation aligned with the
Postgres protocol it is built on.

## When babar is a good fit

Reach for babar when you want:

- Postgres-specific behavior without a lowest-common-denominator abstraction
- typed statement values at the database boundary
- schema-aware typed SQL for common application queries and commands
- explicit raw fallbacks for the cases that need them
- early feedback when statement shape and schema drift apart

If you need multi-database support, a full ORM, or a much broader SQL rewrite
surface, another tool will fit better.

## Where to read next

- [Why babar](./why-babar.md) — a shorter statement of intent.
- [Design principles](./design-principles.md) — the API rules behind these
  choices.
- [The background driver task](./driver-task.md) — runtime mechanics and
  shutdown.
- [The typed-SQL macro pipeline](./typed-sql-macro-pipeline.md) — how the typed
  SQL surface lowers into runtime statement values.
