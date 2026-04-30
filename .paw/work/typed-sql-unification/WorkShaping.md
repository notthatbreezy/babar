# Work Shaping

## Problem Statement

`babar` currently exposes multiple overlapping typed SQL entrypoints: `sql!`,
`query!`, `command!`, and the newer schema-aware `typed_query!`
([crates/macros/src/lib.rs:1-5](crates/macros/src/lib.rs#L1-L5),
[crates/macros/src/lib.rs:119-229](crates/macros/src/lib.rs#L119-L229)).
The desired product direction is to converge on schema-aware typed SQL as the
main path, even if that means removing older macro surface area and accepting
some migration friction.

The immediate goal is to evolve the current typed SQL direction so that the
public `query!` / `command!` story is backed by schema-aware typed SQL rather
than the current explicit codec DSL, while keeping raw/simple-query escape
hatches for migrations and advanced unsupported cases.

## Shaped Direction

### End-state API

- The long-term primary API should be **two explicit typed SQL entrypoints**:
  `query!` for row-returning SQL and `command!` for write / no-row SQL.
- Those names should ultimately replace the current `query!` / `command!`
  implementations rather than coexist as permanent peers.
- `sql!` should begin phasing out in this round and should not remain a
  permanent first-class surface once typed `query!` / `command!` are strong
  enough.
- `simple_query_raw` stays for migrations and as an explicit advanced escape
  hatch.

### Schema and verification model

- **Authored schema modules are primary.** The `schema!` module path is the real
  user story; inline schema blocks should remain only for examples and tests.
- Live verification should be built into the main typed SQL macro path and
  should eventually replace the current verification niche of the old
  `query!` / `command!`.
- Verification should check:
  - parameter compatibility,
  - returned / `RETURNING` column compatibility,
  - table and column existence,
  - agreement between authored schema facts and the live database.

## Work Breakdown

### Core functionality

1. Replace the old macro-facing `query!` / `command!` story with schema-aware
   typed SQL surfaces.
2. Expand statement coverage beyond the current `SELECT`-only implementation.
3. Expand runtime lowering so typed SQL supports more of the existing codec
   ecosystem, with this round prioritizing:
   - `uuid`
   - `date`
   - `time`
   - `timestamp`
   - `timestamptz`
   - `json`
   - `jsonb`
   - `numeric`
4. Build live verification into the typed SQL path so authored schemas can be
   checked against a real PostgreSQL instance during macro expansion.

### Supporting functionality

1. Update prepare/streaming ergonomics so the new typed path remains
   first-class, not just direct execution.
2. Start deprecating or repositioning `sql!` and the old explicit-codec macro
   documentation.
3. Keep migrations and other control-plane SQL on raw/simple-query execution.

## Initial SQL Subset

### In scope this round

#### Read path

Keep the existing typed SQL read subset centered on single-statement `SELECT`
queries with explicit projections, `FROM`, optional `JOIN ... ON`, `WHERE`,
`ORDER BY`, `LIMIT`, and `OFFSET`
([crates/macros/src/typed_sql/parse_backend.rs:18-35](crates/macros/src/typed_sql/parse_backend.rs#L18-L35),
[crates/macros/src/typed_sql/normalize.rs:45-145](crates/macros/src/typed_sql/normalize.rs#L45-L145)).

#### Write path

Support a narrow, useful typed write subset:

- `INSERT ... VALUES ...`
- `INSERT ... RETURNING <explicit columns>`
- `UPDATE ... SET ... WHERE ...`
- `UPDATE ... RETURNING <explicit columns>`
- `DELETE ... WHERE ...`
- `DELETE ... RETURNING <explicit columns>`

DDL stays on raw/simple-query for this round.

### Explicitly out of scope this round

- `ON CONFLICT`
- `INSERT ... SELECT`
- `WITH` / CTEs
- subqueries
- `UPDATE ... FROM`
- `DELETE ... USING`
- wildcard `RETURNING *`
- multi-statement batches
- typed DDL

## Edge Cases and Expected Handling

1. **Schema drift during live verification**
   - Expected handling: fail macro expansion with diagnostics that clearly show
     where authored schema facts disagree with the live database.

2. **Unsupported but valid Postgres SQL**
   - Expected handling: typed `query!` / `command!` reject it with clear subset
     diagnostics; users can fall back to raw/simple-query escape hatches.

3. **Optional-query shapes with prepare/streaming**
   - Current implementation rejects preparing runtime-dynamic optional typed
     queries ([crates/core/src/session/mod.rs:267-280](crates/core/src/session/mod.rs#L267-L280)).
   - Expected handling this round: preserve first-class ergonomics where
     possible and explicitly design around shape-dependent SQL rather than
     silently degrading behavior.

4. **Type support mismatch between authored schemas and runtime lowering**
   - Current authored declarations already accept more SQL types than runtime
     lowering can emit (`uuid`, temporal types, `json`, `jsonb`, `numeric`,
     etc.), while lowered runtime codecs are still limited to primitive scalar
     families ([crates/macros/src/lib.rs:190-205](crates/macros/src/lib.rs#L190-L205),
     [crates/macros/src/typed_sql/lower.rs:305-366](crates/macros/src/typed_sql/lower.rs#L305-L366)).
   - Expected handling: close that gap for the priority types in this round.

## Rough Architecture

### Reuse direction

Build on the existing typed SQL pipeline rather than introducing a second typed
SQL implementation:

1. macro frontend
2. SQL canonicalization / parsing
3. typed SQL normalization
4. schema-aware resolution
5. lowering to ordinary runtime `Query` / `Command` values

The current `typed_query!` path already goes through this structure
([crates/macros/src/lib.rs:190-229](crates/macros/src/lib.rs#L190-L229)).
The runtime `Query` and `Command` wrappers themselves are already general enough
to remain the execution substrate
([crates/core/src/query/mod.rs:25-170](crates/core/src/query/mod.rs#L25-L170)).

### Likely implementation shape

- Evolve the current typed SQL frontend so it can lower both row-returning and
  command-style statements.
- Repoint public `query!` / `command!` macro names to typed SQL semantics.
- Extend verification to compare authored-schema-derived expectations with live
  probe results.
- Preserve raw/simple-query execution as a separate lane for migrations and
  advanced unsupported SQL.

## Critical Analysis

### Why this is worth doing

This unifies the product story around the API the user actually prefers:
schema-aware SQL that feels close to handwritten SQL, not codec-tuple wiring.
It reduces conceptual fragmentation between `typed_query!` and the older
explicit-codec macros, and makes authored schema modules the main reusable unit.

### Build vs. modify tradeoff

This should be treated as a **modification/unification** effort, not a brand-new
parallel feature. The existing typed SQL stack already provides the parser,
normalizer, resolver, and lowering backbone; the right move is to widen and
generalize that path instead of keeping multiple typed SQL implementations alive
forever.

## Codebase Fit

- The current macro crate still treats `sql!`, `query!`, and `command!` as the
  primary public entrypoints, while `typed_query!` and `schema!` are the newer
  schema-aware additions
  ([crates/macros/src/lib.rs:1-5](crates/macros/src/lib.rs#L1-L5),
  [crates/macros/src/lib.rs:156-229](crates/macros/src/lib.rs#L156-L229)).
- The current explicit-codec `query!` / `command!` already perform live
  verification against a probe database when configured
  ([crates/macros/src/lib.rs:156-179](crates/macros/src/lib.rs#L156-L179)).
- The runtime execution layer already separates row-returning `Query` from
  no-row `Command`, which matches the desired two-surface end state
  ([crates/core/src/query/mod.rs:25-170](crates/core/src/query/mod.rs#L25-L170)).
- Migrations deliberately depend on `simple_query_raw` and transactional batches
  of raw SQL strings, so the raw lane is not accidental; it is required by the
  current migration architecture
  ([crates/core/src/migration/runner.rs:32-75](crates/core/src/migration/runner.rs#L32-L75)).

## Risks and Gotchas

1. **Public migration cost**
   - Replacing the old macros in place will create real user migration churn.

2. **Verification complexity**
   - Combining authored schema facts with live verification adds a richer failure
     mode than either authored-only or probe-only checking.

3. **Prepared-statement ergonomics**
   - Dynamic SQL shapes already block `prepare_query` in some typed-query cases
     ([crates/core/src/session/mod.rs:267-280](crates/core/src/session/mod.rs#L267-L280)).
     A broader typed SQL surface increases the importance of having a clear
     policy for preparable vs. non-preparable statements.

4. **Subset pressure**
   - Once `query!` / `command!` names are repointed to typed SQL semantics,
     unsupported PostgreSQL features will feel more like regressions unless the
     escape-hatch story is documented crisply.

## Open Questions for Spec/Planning

1. Should raw `Query::raw` / `Command::raw` remain public advanced escape
   hatches, or should the escape-hatch story focus primarily on
   `simple_query_raw`?
2. Should the public implementation this round literally replace the current
   `query!` / `command!` macros, or should there be a short compatibility period
   with internal forwarding / aliasing?
3. What is the right prepare/streaming design for shape-dependent typed SQL so
   the new path remains first-class?
4. Should `RETURNING` lower through the same row-resolution pipeline as `SELECT`,
   or should command-returning statements have a distinct internal lowering
   abstraction?
5. How much of current codec parity can realistically land in this round beyond
   the agreed priority types?

## Session Notes

- User wants typed SQL to become the **main** API path even if older API surface
  area is lost.
- End-state macro names should be `query!` and `command!`, but backed by the
  schema-aware typed SQL direction rather than the old explicit codec DSL.
- Authored schema modules are the primary model; inline schemas should remain
  only for examples/tests.
- Live verification belongs in the typed SQL path and should eventually replace
  the old verification niche of the current `query!` / `command!`.
- Keep `simple_query_raw` for migrations and advanced unsupported SQL.
- Typed DML is in scope this round; typed DDL is not.
- Long-term type support should aim for parity with the current non-typed
  query/command codec setup, with `uuid`, temporal types, `json`/`jsonb`, and
  `numeric` prioritized now.
