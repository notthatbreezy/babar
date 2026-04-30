# Design principles

> See also: [Why babar](./why-babar.md),
> [What makes babar babar](./what-makes-babar-babar.md), and
> [The typed-SQL macro pipeline](./typed-sql-macro-pipeline.md).

These principles explain why the public API looks the way it does.

## 1. Typed at the boundary

Database operations are represented as typed values:

- `Query<P, R>` for row-returning statements
- `Command<P>` for statements without rows

That keeps the contract visible in the type itself. A reader can see the bound
value shape and the decoded row shape without reconstructing it from runtime
logic.

The same rule shapes the macro surface. `schema!`, `query!`, and `command!`
produce the same `Query<P, R>` and `Command<P>` values you could build by hand
with the raw constructors.

## 2. Async by construction

Each `Session` is backed by a background task that owns the `TcpStream`. Public
API calls communicate with that task over channels and await the reply.

That runtime structure is what makes babar's cancellation-safety story work: if a
waiting future is dropped, the driver task still completes the in-flight
protocol exchange before moving to the next request.

## 3. Native Postgres protocol, not a translation layer

babar speaks the Postgres v3 wire protocol directly. It does not wrap `libpq`,
it does not shell out to a C client, and it does not flatten Postgres behavior
behind a generic SQL abstraction.

That makes Postgres features show up directly in the API surface:

- binary results
- extended-protocol prepared statements
- SCRAM authentication and channel binding
- binary `COPY FROM STDIN`
- row metadata checked against declared decoder OIDs

## 4. Validate, then run

babar pushes verification toward the earliest useful point.

- Parameter encoders fix the bind shape before any network I/O.
- Decoder column counts and OIDs are checked against `RowDescription` at prepare
  time.
- Schema-aware `query!` can optionally verify supported `SELECT` statements
  against a live database during macro expansion.
- Error values carry SQL text and origin information so failures stay tied to the
  authored statement.

This principle is why the typed-SQL surface stays narrow. babar would rather make
unsupported cases explicit than claim broader coverage with weaker guarantees.

## 5. Explicit layers beat hidden magic

There is a deliberate separation between:

- schema-aware macros for ordinary application SQL
- `sql!` for lower-level fragment composition
- raw builders for explicit codec-driven statements
- simple-protocol raw execution for bootstrap and advanced escape hatches

That separation keeps each layer legible. It also means the docs can teach one
primary path without pretending every SQL statement belongs in the same tool.

## 6. No `unsafe`

babar keeps `unsafe` out of the implementation. The macro crate forbids unsafe
code, and the rest of the codebase follows the same line.

## 7. Small dependency surface, small feature surface

The default feature set is intentionally small. Optional codec families and TLS
backends are feature-gated so applications only compile the integrations they
need.

That reduces compile time, narrows dependency risk, and keeps the default build
focused on the core Postgres client surface.

## 8. Operability is part of the API

Pools, statement caches, and `tracing` spans are not bolt-ons. Connection,
prepare, and execute paths emit spans that fit standard database observability
conventions, and `application_name` flows through to `pg_stat_activity`.

The point is not to ship an observability product. The point is to expose the
seams production services need.

## Where to read next

- [The driver task](./driver-task.md) for the cancellation-safety runtime story.
- [The typed-SQL macro pipeline](./typed-sql-macro-pipeline.md) for the macro
  architecture.
- [Comparisons](./comparisons.md) for trade-offs against other Rust Postgres
  clients.
- [Book Chapter 9 — Error handling](../book/09-error-handling.md) for how
  validate-early decisions show up at runtime.
