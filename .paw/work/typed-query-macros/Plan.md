# Plan: Typed Query Macros

## Problem

`babar` currently exposes SQL-first macros built around string literals plus
codec declarations:

- `sql!(...)` rewrites named placeholders into `Fragment<A>`
- `query!(...)` and `command!(...)` validate declared parameter/row codecs
  against a live database when configuration is present

That gives good runtime ergonomics and some live verification, but it does not
provide a **schema-aware, fully type-checked query language** at compile time.

The new goal is to design the **new primary querying direction for `babar`** as
a greenfield system that uses a PostgreSQL parser and schema symbols so callers
can write literal query syntax that is type-checked against:

- known tables / columns
- required parameters
- projected row shape
- basic query composition rules

Backwards compatibility is explicitly **not** a design constraint for this work.
The current architecture is useful context, but it should not constrain the new
design if cleaner foundations are available.

## Product Direction

Build the most promising stack around **`pg_parse`**:

- PostgreSQL-specific parser using `libpg_query`
- lighter dependency profile than the official `pg_query.rs`
- parse tree output that can support a schema-aware resolver/type-checker

This choice is a deliberate trade:

- **Pro:** lighter-weight and focused, suitable for greenfield experimentation
- **Con:** less canonical / less up-to-date than the official `pg_query.rs`

For this workflow, the intent is to optimize for **fast iteration on the typed
macro architecture**, not long-term parser neutrality.

This is not being treated as an experimental sidecar. Planning should assume
that, if successful, this stack can become the canonical long-term query story
for `babar`.

## Constraints

- Treat this as greenfield: existing macro syntax, runtime query wrappers, and
  API compatibility do not need to be preserved.
- Favor **developer ergonomics** and **helpful compile errors** over perfect SQL
  coverage in the first iteration.
- Query composition must be part of the design, not bolted on later.
- It is acceptable to require **fully qualified names** such as `users.id`,
  `users.name`, etc. to simplify name resolution and diagnostics.
- Stay Postgres-specific and explicit rather than drifting into a generic ORM.
- Do not assume current `Fragment` / `Query` / `Command` shapes are the final
  target abstractions; replacing them is allowed if it materially improves the
  typed-query system.

## Most Promising Stack

### Parsing

- `pg_parse` for SQL parsing to PostgreSQL parse trees
- proc-macro front-end using `proc_macro2`, `syn`, and `quote`

### Schema / symbol source of truth

Introduce a generated or handwritten Rust schema module that defines:

- tables
- columns
- column SQL types / nullability
- optional join / relation metadata later

The first pass should prefer **generated Rust symbols** over runtime-only
database introspection so the macro can resolve names without a live database.

### Type-checking layers

1. **Parser layer** — convert macro input into a parsed SQL tree
2. **Resolver layer** — resolve tables / columns against schema symbols
3. **Type-check layer** — validate expressions, predicate compatibility,
   placeholders, and projected row shapes
4. **Lowering layer** — generate either existing or replacement typed runtime
   statement values

### Macro ergonomics

Two possible macro surfaces:

1. **Literal token-style SQL macro**
   ```rust
   typed_sql!(SELECT users.id, users.name FROM users WHERE users.id = $id)
   ```
2. **Slightly more structured SQL-like macro**
   ```rust
   typed_sql!(
       SELECT users.id, users.name
       FROM users
       WHERE users.id = $id
   )
   ```

The plan should optimize for the second: it is still literal-feeling, but much
easier to produce good diagnostics and maintain stable parsing.

## Core Architectural Decision

Use **schema symbols plus full qualification** as the baseline compromise.

That means:

- `users.id` is valid
- bare `id` is rejected unless later explicitly supported
- aliases may come later
- joins and more advanced scope rules should be a second-phase feature

This reduces ambiguity and makes both parsing and error reporting much simpler.

## Proposed Scope for v1

### In scope

- `SELECT ... FROM ... WHERE ...`
- explicit table-qualified columns
- placeholders / bound parameters
- typed projections into tuple/decoder-compatible shapes
- basic `ORDER BY`
- basic `LIMIT` / `OFFSET`
- direct composition with typed predicates or a typed query fragment layer

### Deferred

- CTEs
- subqueries
- aliases beyond a minimal subset
- window functions
- aggregate/type-inference edge cases beyond a narrow supported set
- full join graph inference
- automatic relation discovery
- every PostgreSQL expression form

## Query Composition Strategy

Composition should not depend solely on string concatenation.

The most promising direction:

- parsed literal query macros for full statements
- typed predicate / clause values for reusable query pieces
- a composition layer that works at a semantic level rather than just text

Likely shape:

- full typed statement macro for end-to-end queries
- reusable predicate / ordering / clause symbols generated from schema-aware
  types
- lowering to a single checked internal statement representation

That preserves ergonomic literal queries while still enabling reusable filters
and safe query-tail composition.

## Error Reporting Principles

The system must feel helpful, not hostile.

Priorities:

- point at the exact token/span that failed
- say what the macro expected vs found
- reference schema symbols in diagnostics
- suggest qualified alternatives when names are ambiguous or missing
- explain unsupported SQL constructs clearly rather than silently degrading

Examples of desired errors:

- `unknown column users.nmae; did you mean users.name?`
- `column users.id has type int4 but placeholder $id was inferred/declared as text`
- `projection (users.id, users.name) does not match requested row type (i32,)`
- `bare column id is not allowed; use a fully qualified column such as users.id`

## Integration Strategy

### Phase 1: Architectural spike

- integrate `pg_parse` into the macros crate or a helper crate
- prove parse-tree access for a narrow `SELECT ... FROM ... WHERE ...` subset
- verify compile-time build ergonomics and binary/build impact

### Phase 2: Schema symbol system

- define how schema metadata is authored or generated
- establish Rust symbol naming conventions
- make full qualification the default rule

### Phase 3: Resolver / type checker

- resolve tables and columns
- type-check predicates and placeholders
- validate projected row shape

### Phase 4: Lowering / runtime surface

- lower checked statements into existing runtime values or new greenfield typed
  statement/runtime types
- decide whether existing `sql!` / `query!` / `command!` stay as low-level APIs,
  become compatibility layers, or are superseded entirely

### Phase 5: Composition and ergonomics

- define reusable typed predicates / orderings
- add fun diagnostics and docs
- benchmark macro cost and dev-loop impact

## Risks

- `pg_parse` may lag upstream PostgreSQL behavior relative to `pg_query.rs`
- parse tree fidelity may still leave significant resolver/type-check work to do
- build times may grow materially once schema codegen and checking are added
- compile errors could become noisy if span mapping is poor
- full SQL support is a trap; v1 must stay intentionally narrow

## Recommendation

Proceed with a **narrow, schema-aware, fully qualified v1**:

- parser: `pg_parse`
- qualification rule: always require `table.column`
- row typing: explicit and checked
- composition: typed predicates / clauses plus typed statement macro
- compatibility: greenfield, no effort spent preserving current macro syntax or
  the current runtime query API if a better design emerges

This gives the best chance of landing a useful, fun, productive system without
immediately getting buried in SQL completeness problems.

## Work Items

1. Prove `pg_parse` integration and parse-tree extraction inside the macros
   crate.
2. Design a schema symbol/module system for tables, columns, SQL types, and
   nullability.
3. Define a minimal checked SQL subset and full-qualification rules.
4. Design resolver/type-check passes for placeholders, predicates, and
   projections.
5. Design lowering from checked query IR to executable `babar` query values or
   replacement typed statement/runtime abstractions.
6. Define query-composition APIs that interoperate with the typed macro system.
7. Design diagnostics, fallback rules, and explicit unsupported-feature errors.

## Notes

- The first implementation should optimize for a delightful narrow path, not
  broad SQL completeness.
- Because this is intended as the future query direction, optimize for
  architectural cleanliness and long-term ergonomics rather than minimizing
  churn against today's query APIs.
- If `pg_parse` turns out to be too limiting later, the architecture should make
  parser replacement possible, but that is not a first-order design goal in this
  workflow.
