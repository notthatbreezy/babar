# External Schemas

## Overview

External schemas let babar users declare reusable schema facts once in Rust and
share them across many `typed_query!` callsites. The as-built v0.1 model keeps
the query surface SQL-first: callers still write token-style `SELECT` queries,
but schema information can now live in a dedicated authored module instead of
being repeated inline at every callsite.

The implemented feature is intentionally focused on authored, Rust-visible
schemas for typed read-query validation. It improves reuse and ergonomics, not
schema management: there is no file-based schema loading, code generation, live
database introspection, or write-query expansion in this phase.

## Architecture and Design

### High-Level Architecture

The external schema flow builds on the existing typed-query pipeline rather than
replacing it:

1. `babar::schema!` parses an authored schema module with one or more `table`
   declarations.
2. The macro emits Rust-visible schema symbols (`SCHEMA`, `TABLE`, per-column
   accessors) plus a schema-scoped `typed_query!` wrapper.
3. `schema_module::typed_query!(SELECT ...)` feeds the generated authored schema
   metadata into the existing typed-query frontend.
4. The normal typed-query pipeline then parses SQL, canonicalizes named
   placeholders, normalizes the supported `SELECT` subset, resolves table and
   column references, and lowers the checked query into a normal `Query<P, R>`.

The old inline path remains available through
`babar::typed_query!(schema = { ... }, SELECT ...)`, but the schema-scoped
wrapper is now the recommended reusable pattern.

### Design Decisions

- **Recommended hybrid pattern:** v0.1 recommends a Rust-visible schema module
  plus `schema_module::typed_query!(...)`. This keeps the schema explicit and
  reusable while still letting individual fields stay terse and type-first.
- **Authored-only v1 scope:** schemas are handwritten Rust declarations. That
  keeps expansion deterministic and avoids path-sensitive or environment-driven
  schema loading.
- **Type-first fields with narrow markers:** most fields are just `name: text`
  or `deleted_at: nullable(timestamptz)`. Extra semantics only appear when they
  add meaning, currently through `primary_key(...)` / `pk(...)`.
- **SQL-first query surface preserved:** external schemas do not introduce a new
  query builder. They provide reusable schema context for the existing
  `typed_query!` read-query flow.
- **Declared-type surface may exceed lowered-type support:** the schema model can
  express more SQL types than typed-query v1 can currently lower into runtime
  codecs. That boundary is documented and surfaced as a compile-time diagnostic.

### Integration Points

- `README.md` now documents the authored-schema model, the recommended
  schema-scoped wrapper, and the v0.1 scope boundaries.
- `docs/book/02-selecting.md` and `docs/book/03-parameterized-commands.md` now
  teach the hybrid/schema-scoped wrapper pattern and explain when the older
  inline schema path still fits.
- `docs/getting-started/first-query.md` now points advanced readers at the
  reusable authored-schema path while keeping `Query::raw` as the starting
  point.
- `crates/core/src/lib.rs` crate docs now describe inline and schema-scoped
  typed-query entrypoints together and call out the current type-lowering
  boundary.

## User Guide

### Prerequisites

- Use `typed_query!` for a supported `SELECT` subset.
- Declare schemas in Rust with `babar::schema!` when you want reuse across
  multiple queries or multiple tables.
- Keep writes on the existing `Command::raw`, `command!`, or `sql!` paths.

### Basic Usage

```rust
use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
        table public.posts {
            id: pk(int8),
            author_id: int4,
            title: text,
        },
    }
}

let lookup: Query<(i32,), (i32, String)> = app_schema::typed_query!(
    SELECT users.id, users.name
    FROM users
    WHERE users.id = $id AND users.active = true
);
```

This is the recommended v0.1 pattern:

- one authored schema module owns the shared table declarations,
- multiple tables can live in the same module,
- query callsites stay SQL-first,
- the generated module also exposes reusable schema symbols such as
  `app_schema::SCHEMA` and `app_schema::users::id()`.

### Advanced Usage

Supported authored-schema API options are:

1. **Inline schema path** — `babar::typed_query!(schema = { ... }, SELECT ...)`
   for local examples, tests, or one-off queries.
2. **Schema module pattern** — `babar::schema! { mod app_schema { ... } }` to
   define one reusable module containing one or more tables.
3. **Recommended hybrid pattern** — a schema module plus its generated
   `app_schema::typed_query!(...)` wrapper, optionally using field markers such
   as `primary_key(...)` / `pk(...)`.

The hybrid pattern is recommended because it balances explicit reusable schema
context with terse, Rust-native field declarations. Users keep a clear module
boundary for shared schema facts without forcing every query to repeat the
entire schema block inline.

Field syntax is intentionally small:

- `name: text`
- `deleted_at: nullable(timestamptz)`
- `id: primary_key(int4)`
- `id: pk(int8)`

The schema-scoped wrapper and the inline path both feed the same typed-query
pipeline. That means the same query-only and optional-suffix boundaries still
apply:

- supported `SELECT` subset only,
- named placeholders such as `$id`,
- `$value?` for supported optional comparisons / limit / offset ownership,
- `(...)?` for supported parenthesized predicates or single `ORDER BY`
  expressions.

## API Reference

### Key Components

- **`babar::schema!`** — declares an authored schema module.
- **`schema_module::typed_query!`** — generated schema-scoped query wrapper and
  the recommended reusable typed-query entrypoint.
- **`schema_module::SCHEMA`** — `SchemaDef` metadata for the authored module.
- **`schema_module::<table>::TABLE`** — reusable `TableRef<T>` symbol.
- **`schema_module::<table>::column_name()`** — reusable `Column<T>` accessors.
- **`babar::typed_query!(schema = { ... }, ...)`** — the older inline schema
  path, still supported for one-off use.

### Configuration Options

The public authored-schema options in v0.1 are all in macro input:

- **Table declaration:** `table public.users { ... }` or another qualified table
  name.
- **Field declaration markers:** plain `type_name`, `nullable(type_name)`,
  `primary_key(type_name)`, and `pk(type_name)`.
- **Declared SQL types:** `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`,
  `int8`, `float4`, `float8`, `uuid`, `date`, `time`, `timestamp`,
  `timestamptz`, `json`, `jsonb`, and `numeric`.

Current typed-query runtime lowering for authored schemas is narrower for query
parameters and row projections:

- supported lowered types: `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`,
  `int8`, `float4`, and `float8`,
- unsupported declared-but-not-lowered types currently fail at compile time with
  a diagnostic naming the SQL type, for example `timestamptz`.

## Testing

### How to Test

- Build docs with `mdbook build`.
- Build rustdoc with `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- Check the public docs for consistent wording around:
  - authored Rust-visible schemas only,
  - the recommended schema-scoped wrapper pattern,
  - the inline path as the secondary/one-off option,
  - the current declared-type versus lowered-type boundary.

### Edge Cases

- Unknown tables or columns from authored schemas should fail with diagnostics
  that explicitly mention authored external schema lookup.
- Unsupported field markers should fail during `schema!` expansion and list the
  supported markers.
- Mixing a schema-scoped wrapper with a second inline `schema = { ... }` block
  is rejected because the wrapper already supplies the external schema context.
- Declared types such as `timestamptz` can exist in the authored schema module
  even though typed-query v1 cannot yet lower them for query params or row
  outputs.

## Limitations and Future Work

- v0.1 is authored-schema-only: no file inputs, schema snapshots, code
  generation, or database introspection.
- The feature is still query-only and limited to the current typed-query
  `SELECT` subset.
- The semantic marker surface is intentionally narrow and currently stops at the
  primary-key marker.
- The authored schema declaration surface is wider than the current runtime
  lowering surface. Broader typed-query codec lowering remains future work.
