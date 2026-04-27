# What makes babar babar

> See also: [Why babar](./why-babar.md), [Design principles](./design-principles.md), [Comparisons](./comparisons.md).

If you only read one explanation page, read this one. This page
describes *where* babar sits, *what* makes it distinctive, what it
deliberately is **not**, and *when* it is the right tool to reach
for.

## Where babar sits

```text
┌─────────────────────────────────────────┐
│ your app                                │
├─────────────────────────────────────────┤
│ babar  (typed Query/Command, codecs,    │
│         pool, COPY, migrations)         │
├─────────────────────────────────────────┤
│ tokio  (TcpStream, tasks, cancellation) │
├─────────────────────────────────────────┤
│ Postgres wire protocol v3               │
└─────────────────────────────────────────┘
```

There is no `libpq`, no `tokio-postgres` underneath, and no abstraction
layer that pretends Postgres is a generic SQL backend. babar speaks the
Postgres v3 protocol directly on top of Tokio. That is the whole stack.

This is a deliberate choice. A driver that supports four databases has
to find the lowest common denominator across four protocols. babar
picks one protocol and exposes its shape — extended-protocol prepare,
binary results, channel binding, binary `COPY FROM STDIN` — without
flattening it.

## What's distinctive

Four properties show up everywhere in the API and why I created it.

### 1. The background driver task

```rust
let session: Session = Session::connect(cfg).await?;   // type: Session
```

`session` is a thin handle. The TCP socket lives in a Tokio task that
`Session::connect` spawned for you. Every public call on `Session`
sends a request down an `mpsc` channel and awaits a `oneshot` reply;
the driver task is the only thing that ever reads or writes the
socket.

Two things fall out of that.

First, every public call is **cancellation-safe**. If you
`tokio::select!` away from a query halfway through, the driver task
keeps reading the in-flight messages and returns the protocol to a
consistent state. You don't end up with a half-parsed `RowDescription`
hanging off your socket the next time you ask for a query.

Second, there is exactly **one writer** to the socket. You can `clone`
the `Session` handle, share it across tasks, and the driver still
serializes commands. There is no locking on top of
the socket — the channel *is* the lock. The
[Driver task](./driver-task.md) page goes into more depth on what the
task owns and how shutdown works.

### 2. Typestate at the boundary

The shape of every database operation is in the type signature.

```rust
use babar::codec::{int4, text, nullable};
use babar::query::Query;
use babar::command::Command;

let select: Query<(i32,), (String, Option<i32>)> =        // type: Query<(i32,), (String, Option<i32>)>
    Query::raw(
        "SELECT name, parent_id FROM users WHERE id = $1",
        (int4,),
        (text, nullable(int4)),
    );

let insert: Command<(String, i32)> =                      // type: Command<(String, i32)>
    Command::raw(
        "INSERT INTO users(name, parent_id) VALUES ($1, $2)",
        (text, int4),
    );
```

`Query<P, R>` says "I take parameters of shape `P` and produce rows of
shape `R`." `Command<P>` says "I take parameters of shape `P` and
produce nothing readable." You cannot accidentally call
`session.query(&insert, ...)` — it doesn't compile.

Transactions extend the same idea. `session.transaction(|tx| ...)`
hands you a `Transaction<'_>` whose lifetime is tied to the closure
body, and the borrow checker prevents you from using the underlying
`Session` while the `Transaction` is alive. There is no "did I forget
to commit?" question because the compiler verifies it for you. See
[Transactions](../book/05-transactions.md) for the full pattern,
including savepoints.

Prepared queries are a separate type:

```rust
let prepared: PreparedQuery<'_, (i32,), (String,)> =      // type: PreparedQuery<'_, (i32,), (String,)>
    session.prepare(&select).await?;
```

A `PreparedQuery` is *not* a `Query`. The compiler knows it has been
sent to the server, and once you have one you can stream rows from it
without re-prepare overhead. Streaming `COPY FROM STDIN` ingest works
the same way: `CopyIn<T>` has its own type, and the compiler tracks
when you've finalized it.

### 3. Codecs are values you import by name

```rust
use babar::codec::{int4, text, nullable};

let row_codec = (int4, text, nullable(int4));   // type: (Int4, Text, Nullable<Int4>)
```

Codecs are runtime values, not derived types. The tuple `(int4, text,
nullable(int4))` *is* the schema of the row, written by hand, sitting
in your source file where you can read it. The `i32`, `String`, and
`Option<i32>` that come back are determined by the codec, not by
inference from a SQL string.

This means three things in practice:

- You don't need a live database at compile time to write a query.
- Adding a new type — say, an enum with a custom OID — means writing a
  `Codec` impl and importing the value. There is no proc-macro to
  re-run, no `schema.rs` to regenerate.
- The codec tuple is the documentation. You can read a `Query` value
  and know exactly what wire types it expects and what Rust types it
  produces, without leaving the file.

The trade-off is honest: the cost is paid once per query and the legibility is
paid back every time you read it.

### 4. Validate early

babar pushes "is this query well-formed?" as far left as it can.

- **At bind time**, the parameter codec tuple is statically the same
  shape as `P` in `Query<P, R>`. You cannot under- or over-bind.
- **At prepare time** `Session::prepare` cross-checks
  the row codec tuple `(int4, text, nullable(int4))` against the
  `RowDescription` Postgres sends back. If the column types or order
  drifted, you get an `Error::SchemaMismatch { position, expected_oid,
  actual_oid, column_name, sql, origin }` at *prepare* time, not when
  you decode a row in production.
- **At display time**, errors carry the `sql` and `origin` (file +
  line where you wrote the SQL). The `Display` impl renders a `^`
  caret under the offending byte for `Error::Server { position, .. }`
  so you don't have to re-count columns by hand.

The net effect is that "compiles + prepares" is a strong signal. You
still have to test, but you don't have to test for "did I bind two
parameters when the SQL wants three" — the type system already knows.

## What babar deliberately is **not**

A short list, because every "not" saves us from a feature you didn't
want.

- **Not multi-database.** No MySQL, no SQLite, no MSSQL. If you need
  multi-database, reach for a multi-database driver. We point at
  `sqlx` in [Comparisons](./comparisons.md).
- **Not synchronous.** babar is async-only on Tokio.
- **Not an ORM.** There is no `Queryable` derive, no `Insertable`, no
  schema-aware DSL. SQL is SQL.
- **Not a query builder.** `Query::raw` and the `sql!` macro give you
  composable SQL fragments; we do not provide a typed AST you build up
  with `.select().from().where_(...)`.
- **Not a migration tool.** babar ships a small migration runner for
  the `embed_migrations!` workflow, but if you want a full migration
  CLI with rollbacks and squashing, `refinery` or `sqlx-cli` are
  better-fit tools.

## When babar is the right pick

Reach for babar when:

- You target **Postgres specifically** and you'd rather see protocol
  features (channel binding, binary `COPY`, prepared statements as a
  type) than have them hidden behind a generic abstraction.
- You want **types on the query** — `Query<P, R>`, `Command<P>`,
  `Transaction<'_>`.
- You want **`validate-early` semantics**: schema drift surfaces at
  prepare time as `Error::SchemaMismatch`, not at row 4,723.

Reach for something else when you need multi-database support, a
mature ORM, or a feature babar has [deferred](./roadmap.md) — those
are real needs and there are good answers for them.

## Where to read next

- [Why babar](./why-babar.md) — the elevator pitch.
- [Design principles](./design-principles.md) — the rule book.
- [The background driver task](./driver-task.md) — how the task,
  channels, and shutdown work.
- [Comparisons](./comparisons.md) — head-to-head with
  `tokio-postgres`, `sqlx`, and `diesel`.
- [Roadmap](./roadmap.md) — what's shipped, what's next, what's
  deferred by design.
