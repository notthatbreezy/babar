# Why babar?

> See also: [Get started](../getting-started/first-query.md), the
> [Book](../book/01-connecting.md).

babar is a Rust client for Postgres. There are several already. Why
another one?

The short answer is *one obvious way to do each thing*. Connect, run a
typed query, run a command, stream a result, manage a transaction,
hold a pool, ingest with COPY, run migrations — there is one shape per
task, and the codecs are values you import by name. The next time you
read your own code (or some code that AI wrote), you can read it.

## Three pillars

### Ergonomic by design

Read it once, understand it forever. Queries are typed values. Codecs
are imported by name. There is one way to start a transaction, one way
to bind a parameter, one way to run a migration. You will not spend an
afternoon learning which of seven options to use.

### Postgres at heart

The wire protocol, faithfully. babar speaks Postgres directly —
extended-protocol prepares, binary results, SCRAM-SHA-256, channel
binding over TLS, and binary `COPY FROM STDIN` for bulk ingest. There
is no translation layer between you and the server.

Over time `babar` will take advantage of unique features and advantages of PostgreSQL because it does not need to worry about the lowest common denominator among databases.

### Built for the herd

Predictable under load. A single background task owns the socket and
serializes wire I/O, so every public call is cancellation-safe. Pool,
statement cache, and `tracing` spans are first-class — not bolted on
later.

## What "typed query" actually means

In babar, a `Query<Params, Row>` is a runtime value. It carries:

- The SQL text.
- A parameter encoder (`Encoder<Params>`).
- A row decoder (`Decoder<Row>`).

When the type system says `Query<(i32,), (Uuid, String, i64)>`, the
compiler knows the parameter shape, the row shape, and which codecs
participate. There is no magic — `Query::raw` constructs one
explicitly, `query!` builds the same thing with explicit codecs and
optional live verification, and the query-only `typed_query!` macro can
lower a small schema-aware `SELECT` from inline schema + token-style SQL
into the same runtime `Query<P, R>` shape.

## Where to read next

- [Design principles](./design-principles.md) — typed, async, native
  protocol, validate-early, no-unsafe.
- [The driver task](./driver-task.md) — the per-connection background
  task that makes every call cancellation-safe.
- [Comparisons](./comparisons.md) — a trade-off-focused comparison table
  for `tokio-postgres`, `sqlx`, and `diesel`.
- [Roadmap](./roadmap.md) — what's in, what's deferred, and where the
  project is going.
