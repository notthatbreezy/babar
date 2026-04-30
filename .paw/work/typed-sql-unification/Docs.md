# Typed SQL Unification

## Overview

Typed SQL unification makes babar's schema-aware typed SQL compiler the primary macro path for application SQL.

- `query!` is now the default typed entrypoint for schema-aware `SELECT` statements.
- `command!` is now the default typed entrypoint for schema-aware `INSERT`, `UPDATE`, and `DELETE` statements.
- `typed_query!` remains available as a compatibility alias to the same compiler.
- `schema!` now generates local `query!` / `command!` wrappers so authored schema modules are the normal reusable call-site surface.
- `Query::raw` / `Command::raw` remain the extended-protocol escape hatches, while `simple_query_raw` remains the simple-protocol lane for migrations and control-plane work.

This document is the as-built technical reference for the implemented Phase 1-4 behavior that Phase 5 documents.

## Architecture and Design

### High-Level Architecture

All schema-aware typed SQL macros now converge on one compiler pipeline:

1. parse macro input (`schema = { ... }` or authored `__babar_schema` bridge)
2. canonicalize SQL text, named placeholders, and optional groups
3. parse exactly one SQL statement
4. normalize into typed SQL IR
5. resolve tables, columns, placeholders, and result shape against the schema catalog
6. lower inferred SQL types into runtime codecs and a `Fragment`
7. emit either a `Query` or a `Command` over the shared runtime substrate

The shared runtime substrate is unchanged:

- `Fragment<A>` holds SQL plus parameter encoding behavior
- `Query<A, B>` wraps row-returning statements
- `Command<A>` wraps no-row statements

### Result-Shape Contract

The compiler decides the runtime shape from the checked statement result, not from the macro name alone.

- `SELECT` always lowers to `Query<A, B>`.
- `INSERT` / `UPDATE` / `DELETE` without `RETURNING` lower to `Command<A>`.
- `INSERT` / `UPDATE` / `DELETE` with explicit `RETURNING` lower to `Query<A, B>`.

This means `command!` is the write-statement entrypoint, but explicit-`RETURNING` writes are still query-shaped at runtime in this round.

### Public Macro Routing

Public routing is now:

- `query!` → unified typed SQL compiler, restricted to typed `SELECT`
- `command!` → unified typed SQL compiler, restricted to typed `INSERT` / `UPDATE` / `DELETE`
- `typed_query!` → compatibility alias to the same compiler, without the new public entrypoint naming guidance

Old explicit-codec `query!("...", params = ..., row = ...)` and `command!("...", params = ...)` forms are rejected with targeted migration diagnostics.

### Authored Schema Integration

`schema!` now generates schema-scoped wrappers that feed the same internal compiler bridge:

- `app_schema::query!(...)`
- `app_schema::command!(...)`
- compatibility aliases `app_schema::typed_query!(...)` and `app_schema::typed_command!(...)`

These wrappers are the primary authored-schema ergonomics. They work across crate boundaries because the generated module re-exports the local macros and the runtime schema metadata.

## Supported SQL Surface

### First-Round Read Subset

The implemented read subset is still intentionally narrow.

Supported `SELECT` shape:

- exactly one top-level statement
- top-level `SELECT`
- explicit projections only
- one `FROM` relation with optional joins
- `WHERE`
- `ORDER BY`
- `LIMIT`
- `OFFSET`
- plain table references in `FROM` / `JOIN`
- named placeholders and existing optional typed-SQL forms

Not supported in this round:

- wildcard projection (`SELECT *`)
- `WITH` / CTEs
- subqueries / derived top-level queries
- set operations
- `DISTINCT`
- `GROUP BY`, `HAVING`, windows, locking, `FETCH`, `FOR`, and other broader query features
- multi-statement input

### Typed DML Subset

Supported write shapes are:

- `INSERT ... VALUES ...`
- `UPDATE ... SET ... WHERE ...`
- `DELETE ... WHERE ...`
- optional explicit-column `RETURNING`

Current behavior by write shape:

- no `RETURNING` → command/no-row lane
- explicit `RETURNING col1, col2, ...` → query-shaped row lane

### Rejected DML Shapes

The compiler rejects unsupported write forms with typed SQL subset diagnostics instead of falling through to runtime errors.

Rejected shapes include:

- `INSERT ... SELECT`
- `ON CONFLICT` / other insert conflict clauses
- `UPDATE ... FROM`
- `DELETE ... USING`
- joined `UPDATE` / `DELETE` targets
- predicate-free `UPDATE`
- predicate-free `DELETE`
- tuple assignments in `UPDATE`
- wildcard `RETURNING *`
- multi-statement batches

### Supported Type Expansion

Runtime lowering now supports these SQL families for inferred parameters and returned columns:

- `bool`
- `bytea`
- `varchar`
- `text`
- `int2`
- `int4`
- `int8`
- `float4`
- `float8`
- `uuid`
- `date`
- `time`
- `timestamp`
- `timestamptz`
- `json`
- `jsonb`
- `numeric`
- nullable variants of the above

Notes:

- authored schema declarations and inline schema blocks accept the same SQL type names
- generated Rust/runtime types still depend on the matching babar feature surface being enabled (for example `time`, `uuid`, `json`, `numeric`)
- types outside this lowering set still fail during typed-SQL lowering with a targeted diagnostic

## Verification

### Live Verification Behavior

Compile-time live verification is opt-in and online-only.

- `BABAR_DATABASE_URL` is consulted first
- `DATABASE_URL` is used as a fallback
- if neither is set, expansion continues without live verification
- there is no offline cache or generated schema snapshot

For schema-aware typed SQL, the verifier:

1. connects to the configured PostgreSQL instance
2. checks referenced schema tables/columns (type and nullability) against the live database
3. prepares the lowered SQL against the live database
4. compares inferred parameter metadata
5. compares inferred row metadata when rows are part of the verified shape

### Current Verification Limits

The implemented typed-SQL verification hook currently runs only for schema-aware `SELECT` statements.

That means:

- typed `query!` / `typed_query!` `SELECT` statements can be live-verified
- non-`RETURNING` typed commands are not currently probe-verified through this path
- explicit-`RETURNING` DML is not currently probe-verified through this path, even though it lowers to a query-shaped runtime value

So the verification transport is integrated into unified typed SQL, but its current reach is narrower than the full statement surface.

## Execution Model

### Static vs Dynamic Typed SQL

Typed SQL keeps the existing optional placeholder and optional group system.

- static statements lower to `Fragment::__from_parts`
- statements with runtime-dependent optional placeholders/groups lower to `Fragment::__from_dynamic_parts`

Dynamic typed SQL is still executable: the runtime renderer activates only the selected optional pieces and renumbers placeholders for the final SQL sent to PostgreSQL.

### Prepare and Streaming Constraints

The execution contract remains intentionally conservative:

- static typed `SELECT` and static typed DML can be prepared as named server-side statements
- runtime-dynamic typed `SELECT` cannot be prepared
- runtime-dynamic typed commands cannot be prepared
- runtime-dynamic typed `SELECT` can still be executed normally
- runtime-dynamic typed `SELECT` can still be streamed

Preparation fails early with an explicit error when a typed statement depends on runtime SQL shape.

## Raw Fallback Guidance

### `Query::raw` / `Command::raw`

Use `Query::raw` or `Command::raw` when you still want the extended query protocol but the unified typed SQL subset is not a fit.

Typical reasons:

- unsupported SQL syntax outside typed SQL v1
- explicit manual codec control
- advanced statements where you still want extended-protocol execution, prepared statements, or query streaming behavior

These builders take explicit codecs and native `$1`, `$2`, ... placeholders.

### `simple_query_raw`

Use `simple_query_raw` for the simple-query lane, not as the default application path.

This remains the correct tool for:

- migration runner work
- explicit `BEGIN` / `COMMIT` / `ROLLBACK` script execution
- health checks and pool reset queries
- multi-result or multi-statement simple-protocol workflows
- control-plane SQL that is not trying to model typed parameters/rows

If the need is “unsupported but still extended-protocol SQL”, prefer `Query::raw` / `Command::raw`. If the need is “simple protocol / batch script / control plane”, prefer `simple_query_raw`.

## Migration and Compatibility

### Public Macro Migration Contract

The public migration contract implemented in this branch is:

- `query!` and `command!` no longer accept the old explicit-codec DSL
- callers must provide schema facts inline or through a schema-scoped wrapper
- diagnostics point users toward schema-aware typed SQL, schema-generated wrappers, and raw fallbacks

### `typed_query!` Compatibility Alias

`typed_query!` still exists so existing typed-SQL users do not lose the previous name immediately. It now shares the same compiler and can lower to either:

- `Query<A, B>` for `SELECT` and explicit-`RETURNING` writes
- `Command<A>` for non-`RETURNING` writes

It is compatibility-only in the current guidance, not the preferred public surface.

### `sql!` Demotion

`sql!` still exists and still builds reusable `Fragment`s, but it is no longer the primary recommended application entrypoint for routine typed SQL.

Current guidance is:

1. prefer schema-aware `query!` / `command!`
2. use schema-scoped wrappers from `schema!` when multiple statements share one authored schema
3. drop to `Query::raw` / `Command::raw` for unsupported extended-protocol shapes
4. reserve `simple_query_raw` for migrations/control-plane/simple-query work
5. use `sql!` when fragment composition itself is the goal

## How to Exercise the Implementation

Human verification paths already covered by implementation tests include:

- schema-aware `query!` read queries executing against Postgres
- schema-aware `command!` non-`RETURNING` DML executing as commands
- explicit-`RETURNING` DML executing as query-shaped rows
- schema-scoped `query!` / `command!` wrappers matching inline-schema behavior
- targeted migration diagnostics for legacy `query!` / `command!` forms
- live verification success and schema/config mismatch failures for typed `SELECT`
- expanded runtime type support for `uuid`, temporal types, `json` / `jsonb`, and `numeric`
- dynamic typed SQL executing and streaming while remaining non-preparable

## Limitations and Future Work

Current limits that remain intentionally visible in the implementation:

- typed SQL is still a v1 subset, not general SQL coverage
- live verification is not yet wired across the full DML surface
- explicit-`RETURNING` writes still reuse query-shaped rows rather than a dedicated row-returning command abstraction
- prepare support still excludes runtime-dynamic typed SQL
- `Query::raw`, `Command::raw`, and `simple_query_raw` remain necessary escape hatches rather than legacy leftovers
