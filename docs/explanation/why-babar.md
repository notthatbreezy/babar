# Why babar?

> See also: [Get started](../getting-started/first-query.md), the
> [Book](../book/01-connecting.md).

babar is a Rust client for Postgres. There are several already. Why
another one?

The short answer is *one obvious way to do each thing*. Connect, run a
typed query, run a command, stream a result, manage a transaction,
hold a pool, ingest with COPY, run migrations — there is one shape per
task, and the codecs are values you import by name. The next time you
read your own code, you can read it.

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
explicitly, and the `query!` macro builds the same thing with optional
compile-time SQL verification.

## What babar deliberately does not do

- It does not require a compile-time database. `query!` against
  `BABAR_DATABASE_URL` is opt-in; the default `Query::raw` path runs
  without any dev-loop infrastructure.
- It does not hide errors behind `&dyn Error`. `babar::Error` is a
  plain enum with eleven variants, each carrying the fields you need
  to decide what to do.

## Where to read next

- [Design principles](./design-principles.md) — typed, async, native
  protocol, validate-early, no-unsafe.
- [The driver task](./driver-task.md) — the per-connection background
  task that makes every call cancellation-safe.
- [Comparisons](./comparisons.md) — honest trade-offs against
  `tokio-postgres`, `sqlx`, and `diesel`.
- [Roadmap](./roadmap.md) — what's in, what's deferred, and where the
  project is going.
