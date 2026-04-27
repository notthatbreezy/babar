# Comparisons

> See also: [Why babar](./why-babar.md), [Design principles](./design-principles.md).

> **Honest about trade-offs.** Pick the tool that fits the job. babar
> is the right fit when you want one obvious way to do things and
> you'd rather see a typed value than a clever macro.

The comparisons below name three other Rust Postgres clients —
`tokio-postgres`, `sqlx`, and `diesel`. The wording is taken from the
project's own `SITE-COPY.md` band where it exists, and tightened to
fit reference prose. It is meant to be balanced; if a claim here is
out of date or unfair, file an issue.

## babar vs `tokio-postgres`

`tokio-postgres` is the reference Tokio-native Postgres client — the
async cousin of `postgres-protocol`, which babar also depends on.

**Where babar wins**: typed `Query<P, R>` / `Command<P>` values are
the API, codecs are imported by name as runtime values, prepare-time
schema validation surfaces drift as `Error::ColumnAlignment` /
`Error::SchemaMismatch`, and errors carry SQL-origin caret rendering.

**Where `tokio-postgres` wins**: years of production hardening,
broader feature coverage today (notably `COPY TO`, text/CSV `COPY`,
`LISTEN`/`NOTIFY`, out-of-band cancellation), and you don't have to
buy into the explicit codec model — `to_sql` / `from_sql` traits on
your types are enough.

**Pick `tokio-postgres` when**: you need a feature babar has
[deferred](./roadmap.md), you're already comfortable with
`to_sql`/`from_sql`, or you want the most battle-tested option in
the Rust Postgres ecosystem.

**Pick babar when**: you want the row and parameter shape visible
in the type signature, you want `validate-early` semantics on
prepare, and you'd rather have one obvious way to start a
transaction than several to choose between.

## babar vs `sqlx`

`sqlx` is a multi-database (Postgres, MySQL, SQLite, MSSQL)
async client with a strong focus on compile-time SQL verification.

**Where babar wins**: explicit runtime codecs (you can write
`Query::raw` without any dev-loop database), no compile-time database
required for normal builds, SQL-origin caret rendering on every
error, and a single Postgres focus that means the protocol surface
isn't an abstraction over four backends.

**Where `sqlx` wins**: broader compile-time macros (its `query!`
introspects against a live database with very polished tooling),
broader database coverage (it's not just Postgres), a larger
ecosystem, and a longer production track record.

**Pick `sqlx` when**: you want compile-time-checked SQL by default,
you target multiple databases, or you value a larger third-party
ecosystem.

**Pick babar when**: you target Postgres specifically, you want the
typed `Query<P, R>` value to be the API rather than a macro
expansion, and you prefer the validate-early-against-the-server
discipline at prepare time over compile-time DSN-driven validation.

## babar vs `diesel`

`diesel` is the established synchronous-by-default Rust ORM, with
first-class support for Postgres, MySQL, and SQLite. Async support
exists via `diesel-async`.

**Where babar wins**: babar is a *typed Postgres client*, not an
ORM. There is no DSL between you and SQL; `Query::raw` and the
`sql!` macro give you composable SQL fragments and the row decoder
is whatever `Decoder<R>` you supply. Spans, the driver task, and
the pool are first-class.

**Where `diesel` wins**: a mature schema-aware DSL, schema
introspection via `diesel print-schema`, an established migration
story, and an ecosystem of crates built on its `Queryable` /
`Insertable` traits.

**Pick `diesel` when**: you want an ORM with schema-driven query
construction, your team prefers a DSL over hand-written SQL, or
you target a non-Postgres backend.

**Pick babar when**: you want SQL to be SQL, you want types to ride
along on the query and command values, and you want the protocol
seam (codecs, prepare, COPY, transactions) to be the API surface
rather than a layer underneath a DSL.

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
