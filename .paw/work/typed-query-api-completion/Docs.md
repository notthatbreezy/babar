# Typed Query API Completion

## Overview

`typed_query!` is now documented as babar's schema-aware, query-only typed SQL surface for a supported `SELECT` subset. The completed v1 shape stays SQL-first: callers write token-style SQL plus an inline `schema = { ... }` block, the macro resolves that query at expansion time, and the output is still an ordinary `Query<P, R>`.

This phase also documents the optional-suffix forms added to that subset. `$value?` marks an optional placeholder with an explicit ownership boundary, and `(...)?` marks an owned parenthesized fragment. The important constraint is that these suffixes are intentionally narrow: they describe omission ownership for supported query fragments, not a general SQL templating or rewrite language.

## Architecture and Design

### High-Level Architecture

The typed-query pipeline remains:

1. Parse `schema = { ... }` and token-style SQL from the proc-macro input.
2. Canonicalize named placeholders to positional SQL while preserving source spans.
3. Parse the supported `SELECT` subset.
4. Normalize into typed-query IR with explicit optional placeholders/groups.
5. Resolve bindings, columns, placeholder types, and clause ownership rules.
6. Lower the checked query into a normal `Query<P, R>`.

The optional-suffix work hooks into the public SQL frontend, normalized IR, resolver, and lowering tests. Public docs should describe the feature at that same level: explicit ownership markers inside a constrained query-only subset.

### Design Decisions

- **Query-only scope stays explicit.** `typed_query!` remains for reads only; writes still use `Command::raw`, `command!`, or `sql!`.
- **SQL-first over builders.** The user-facing model is still SQL text with small suffix markers, not a fluent AST builder.
- **Ownership must be explicit.** Optional behavior is only supported where the macro can tell exactly which predicate, group, or tail expression is owned.
- **No general rewriting promise.** The docs must keep saying that this is intentionally narrower than arbitrary SQL rewriting.

### Integration Points

- `README.md` now describes `typed_query!` as the query-only schema-aware macro rather than an “early POC”.
- The book chapters and getting-started guide now explain the supported optional suffix forms and restate the query-only boundary.
- The explanation docs now align with the as-built framing: useful for schema-aware `SELECT`s, intentionally not an ORM or general query builder.
- `crates/core/examples/axum_service.rs` now describes its read path in the same query-only terms.

## User Guide

### Prerequisites

- Use `babar::typed_query!(...)`.
- Provide an inline `schema = { ... }` DSL.
- Stay within the supported typed-query `SELECT` subset.

### Basic Usage

```rust
let lookup: Query<(i32,), (i32, String)> = babar::typed_query!(
    schema = {
        table public.users {
            id: int4,
            name: text,
            active: bool,
        },
    },
    SELECT users.id, users.name
    FROM users
    WHERE users.id = $id AND users.active = true
);
```

This remains the core mental model: schema-aware `SELECT` in, ordinary `Query<P, R>` out.

### Advanced Usage

Supported optional suffix forms are:

- **`$value?`** — supported only when the placeholder directly owns:
  - a whole `WHERE` / `JOIN ... ON` comparison predicate, or
  - the full `LIMIT` or `OFFSET` expression.
- **`(...)?`** — supported only when the group owns:
  - an entire parenthesized `WHERE` / `JOIN ... ON` predicate, or
  - a single `ORDER BY` expression.

Examples of supported shapes:

```sql
WHERE (users.id = $id?)?
WHERE (users.id >= $min_id? AND users.id <= $max_id?)?
ORDER BY (users.name)? DESC
LIMIT $limit?
OFFSET $offset?
```

Unsupported shapes are documented as unsupported on purpose:

- suffixes in projections or arbitrary expressions,
- wrapping whole clause keywords like `(ORDER BY users.id)?`,
- `(...)?` around `LIMIT` / `OFFSET`,
- ambiguous ownership such as unary operators attached to `$value?`.

Repeated named placeholders still reuse a single positional slot, including repeated optional placeholders.

## API Reference

### Key Components

- **`typed_query!`**: schema-aware, query-only proc macro for a supported `SELECT` subset.
- **Inline schema DSL**: `schema = { table ... { column: type, ... } }`.
- **Optional placeholder syntax**: `$value?`.
- **Optional owned-group syntax**: `(...)?`.

### Configuration Options

There are no new runtime configuration knobs for this phase. The public configuration remains the macro input itself:

- inline schema,
- token-style SQL,
- supported optional suffix placement.

As built on this branch, the macro still emits an ordinary `Query<P, R>` with concrete SQL and inferred codecs. The optional-suffix behavior is therefore documented as part of the supported typed-query subset and ownership model, not as a separate general runtime SQL-rewriting system.

## Testing

### How to Test

- Build the project docs with `mdbook build`.
- Run the full verification suite with `cargo test --all-features`.
- Check the book, README, and `axum_service` example for consistent query-only wording and optional-suffix examples.

### Edge Cases

- Incomplete optional groups omit the owned predicate/group rather than leaving partial comparisons behind.
- `ORDER BY` optionality is expression-owned; clause-level wrapping is rejected.
- `LIMIT` / `OFFSET` optionality must use direct `$value?` placeholders.
- Unsupported ownership placements should fail with location-aware diagnostics that mention `$value?` or `(...)?`.

## Limitations and Future Work

- `typed_query!` is still query-only and `SELECT`-only.
- The public schema story is still inline-schema-first.
- The supported grammar remains intentionally narrower than full SQL.
- Broader schema reuse, wider SQL coverage, and any future runtime multi-shape strategy remain later work rather than promises of this phase.
