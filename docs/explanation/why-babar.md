# Why babar?

> See also: [Get started](../getting-started/first-query.md), the
> [Book](../book/01-connecting.md).

babar is a Rust client for Postgres. The project is organized around a simple
idea: one clear shape for each database task.

Connect, run a typed query, run a command, stream a result, manage a
transaction, hold a pool, ingest with COPY, run migrations — each task has a
small surface area, and the codec layer stays explicit when you need it.

## Three pillars

### Ergonomic by design

Read it once, understand it forever. Queries are typed values, commands are
typed values, and codecs are imported by name when you work at the raw layer.

### Postgres at heart

babar speaks Postgres directly: extended-protocol prepares, binary results,
SCRAM-SHA-256, channel binding over TLS, and binary `COPY FROM STDIN` for bulk
ingest.

### Built for the herd

A single background task owns the socket and serializes wire I/O, so public calls
stay cancellation-safe. Pools, statement caches, and `tracing` spans are part of
the design.

## What “typed query” means here

In babar, a `Query<Params, Row>` is a runtime value. It carries:

- SQL text
- a parameter encoder for `Params`
- a row decoder for `Row`

The schema-aware path uses `schema!`, `query!`, and `command!` to build those
runtime values from authored schema facts and SQL. The explicit fallback path
uses `Query::raw`, `Query::raw_with`, `Command::raw`, and `Command::raw_with`
when you want to provide codecs yourself.

## Where to read next

- [Design principles](./design-principles.md) — typed boundaries, validation,
  and runtime model.
- [The driver task](./driver-task.md) — how the background task keeps the socket
  consistent.
- [Comparisons](./comparisons.md) — trade-offs against other Rust Postgres
  clients.
- [The typed-SQL macro pipeline](./typed-sql-macro-pipeline.md) — how the public
  typed-SQL surface is assembled.
