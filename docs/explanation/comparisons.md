# Comparisons

> See also: [Why babar](./why-babar.md), [Design principles](./design-principles.md).

> **Trade-offs, not scorekeeping.** These tools solve overlapping
> problems from different angles. The useful question is which shape
> fits your team, database scope, and operating model.

The table below compares `babar` with three common Rust choices:
`tokio-postgres`, `sqlx`, and `diesel`.

| Dimension | `babar` | `tokio-postgres` | `sqlx` | `diesel` |
| --- | --- | --- | --- | --- |
| Primary shape | Typed Postgres client | Async Postgres driver | Async SQL toolkit | ORM / query DSL |
| Database scope | Postgres only | Postgres only | Multiple databases | Multiple databases |
| Query API | Typed runtime `Query<P, R>` / `Command<P>` values | Raw SQL strings plus codec traits | Raw SQL, macros, row mapping helpers | Schema-aware DSL and derives |
| SQL checking style | Optional online verification plus prepare-time validation | Mostly runtime | Strong compile-time emphasis | Schema-driven compile-time DSL |
| Explicit codec model | Yes, codecs are imported values | Usually trait-based (`ToSql` / `FromSql`) | Mostly inferred / mapped through traits and macros | Mostly hidden behind derives / schema mapping |
| Current maturity | Newer, intentionally focused surface | Most battle-tested async Postgres option | Large ecosystem and polished tooling | Mature ORM ecosystem |
| Strong fit | Postgres-specific apps that want explicit typed values and protocol visibility | Teams that want established async Postgres coverage today | Teams that want compile-time SQL workflows or multi-database support | Teams that want an ORM and schema-driven query construction |

## Reading the trade-offs

### `babar` and `tokio-postgres`

These two are the closest in scope: both are Postgres-specific async
clients. The trade-off is mostly about API shape.

- Choose **`babar`** when you want query and row shape visible in the
  type signature, explicit codec values, prepare-time schema checks,
  and richer SQL-origin error rendering.
- Choose **`tokio-postgres`** when you want the most established async
  Postgres driver in Rust today, broader production history, or a
  feature babar still [defers](./roadmap.md) such as broader `COPY`,
  `LISTEN` / `NOTIFY`, or cancellation surface.

### `babar` and `sqlx`

These overlap most for teams that like hand-written SQL but care about
types and validation.

- Choose **`babar`** when you want Postgres-specific APIs, explicit
  runtime codecs, and normal builds that do not depend on compile-time
  database connectivity.
- Choose **`sqlx`** when compile-time SQL checking is the center of your
  workflow, you want offline-cache tooling, or you need a single client
  across multiple databases.

### `babar` and `diesel`

Here the trade-off is more architectural than incremental.

- Choose **`babar`** when you want SQL to stay SQL and prefer the
  protocol seam — codecs, prepare, COPY, transactions, pooling — to be
  the visible API.
- Choose **`diesel`** when you want an ORM, schema-driven query
  construction, and a workflow built around derives, generated schema,
  and migration tooling.

## Summary

| If you want… | Reach for |
|---|---|
| A typed Postgres client with one obvious way to do each thing | **babar** |
| The most battle-tested async Postgres driver in Rust | `tokio-postgres` |
| Compile-time-verified SQL, multi-database support | `sqlx` |
| A schema-aware ORM with a strong DSL | `diesel` |

## Where to read next

- [Roadmap](./roadmap.md) — what's deferred (and therefore what
  `tokio-postgres` covers today that babar doesn't).
- [Design principles](./design-principles.md) — the *why* behind the
  trade-offs above.
