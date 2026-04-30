# Typed SQL Unification Implementation Plan

## Overview

This work turns babar's schema-aware typed SQL direction into the primary typed
SQL API by moving public `query!` and `command!` onto the typed SQL compiler
path, expanding that compiler beyond its current `SELECT`-only boundary,
keeping non-`RETURNING` `command!` statements in the existing command-style
no-row lane, lowering explicit-`RETURNING` `command!` statements through the
same query-shaped row machinery used for row-returning typed SQL in this round,
and building live verification and broader runtime type support into the
unified surface.

The plan is intentionally architecture-first. Rather than bolting more behavior
onto the old explicit-codec macros, it widens the existing typed SQL pipeline
and keeps `Fragment`, `Query`, and `Command` as the shared runtime substrate.
Raw `Query::raw` / `Command::raw` remain advanced extended-protocol fallbacks,
while `simple_query_raw` remains the migration and control-plane lane.

## Current State Analysis

- Public `query!` / `command!` still parse explicit codec DSL and verify through
  `verify.rs`, while `typed_query!` uses a separate schema-aware compiler path
  (`crates/macros/src/lib.rs:127-229`,
  `crates/macros/src/lib.rs:232-398`,
  `crates/macros/src/typed_sql/public_schema.rs:20-40`).
- The typed SQL compiler is still shaped around `ParsedSelect`,
  `CheckedSelect`, and `LoweredQuery`, with a backend parser that accepts only a
  single top-level `SELECT`
  (`crates/macros/src/typed_sql/mod.rs:19-33`,
  `crates/macros/src/typed_sql/parse_backend.rs:9-37`,
  `crates/macros/src/typed_sql/lower.rs:418-478`).
- Authored schema modules already integrate cleanly through `schema!` and a
  schema-scoped `typed_query!` bridge, which is the strongest existing reuse
  seam for repointing authored-schema call sites onto local `query!` /
  `command!` wrappers backed by the shared typed compiler contract
  (`crates/macros/src/schema_decl.rs:22-123`,
  `crates/macros/src/schema_decl.rs:233-336`).
- Live verification exists only in the explicit-codec macro path today, while
  typed SQL does not yet call the probe machinery
  (`crates/macros/src/verify.rs:21-52`,
  `crates/macros/src/verify.rs:124-152`,
  `crates/macros/src/verify.rs:245-295`).
- Authored typed SQL accepts `uuid`, temporal types, `json` / `jsonb`, and
  `numeric`, but runtime lowering still only supports primitive scalar families;
  the wider core runtime already has broader codec/type support, so the gap is
  in typed SQL lowering rather than the transport/runtime itself
  (`crates/macros/src/typed_sql/public_schema.rs:331-357`,
  `crates/macros/src/typed_sql/lower.rs:1042-1081`,
  `crates/core/src/types.rs:96-111,248-255`,
  `crates/core/src/codec/mod.rs:38-72,79-130`).
- `simple_query_raw` is a hard dependency for migrations and some control-plane
  behavior, so the raw/simple-query story must be preserved deliberately rather
  than treated as dead legacy
  (`crates/core/src/migration/runner.rs:32-75,149-179,216-315`,
  `crates/core/src/pool.rs:456-465,490-496`,
  `crates/core/src/session/mod.rs:60-80,443-510`).
- Runtime-dynamic optional typed SQL already exists and is intentionally
  non-preparable today; the plan must preserve a clear supported/non-supported
  policy rather than accidentally implying that all statement shapes can be
  prepared
  (`crates/macros/src/typed_sql/lower.rs:46-94`,
  `crates/core/src/session/mod.rs:267-280,341-353`).

## Desired End State

After implementation:

1. `query!` is the default schema-aware typed surface for supported
   row-returning statements in the first-round read subset: single top-level
   `SELECT` statements with explicit projections plus the current `FROM`,
   `JOIN`, `WHERE`, `ORDER BY`, `LIMIT`, and `OFFSET` coverage.
2. `command!` is the default schema-aware typed surface for supported write
   statements: statements without `RETURNING` stay command-style/no-row, while
   explicit `RETURNING` forms lower through the same query-shaped row machinery
   as `SELECT` in this round instead of introducing a separate row-returning
   command abstraction.
3. The unified typed SQL compiler supports the agreed first DML subset and the
   prioritized missing SQL types (`uuid`, `date`, `time`, `timestamp`,
   `timestamptz`, `json`, `jsonb`, `numeric`) for both parameters and returned
   columns.
4. Optional live verification checks the referenced authored schema facts and
   inferred statement metadata for one typed statement against a live database
   through the typed SQL path.
5. `Query::raw` / `Command::raw` remain documented advanced extended-protocol
   fallbacks, and `simple_query_raw` remains the migrations/control-plane path.
6. Public `query!` / `command!` expose the unified typed SQL path with an
   explicit migration contract: old explicit-codec forms fail with targeted
   migration diagnostics; schema-generated modules expose local `query!` /
   `command!` wrappers as the primary authored-schema call-site contract backed
   by the shared internal schema-driven typed compiler bridge; and
   `typed_query!` remains available only as a compatibility alias rather than
   the primary recommended surface.

Verification approach:

- Preserve green behavior for existing typed-query features while widening the
  internal compiler.
- Require targeted UI/integration tests for new statement kinds, new type
  lowering, live verification, and unsupported-subset diagnostics.
- Keep the current rule that runtime-dynamic typed SQL shapes are executable but
  not preparable; document that policy explicitly rather than silently changing
  it.

## What We're NOT Doing

- Typed DDL in this round
- `ON CONFLICT`
- `INSERT ... SELECT`
- `WITH` / CTEs
- Subqueries
- Set operations
- `UPDATE ... FROM`
- `DELETE ... USING`
- Predicate-free `UPDATE`
- Predicate-free `DELETE`
- Wildcard `SELECT *` projection support in the first-round read subset
- Wildcard `RETURNING *`
- Multi-statement batches
- Removal of `Query::raw`, `Command::raw`, or `simple_query_raw`
- Full codec/type parity beyond the prioritized missing SQL types in this round

## Phase Status

- [ ] **Phase 1: Statement Compiler Foundation** - Generalize the typed SQL compiler from a `SELECT`-only pipeline into a statement-kind-aware internal architecture without changing user-facing behavior yet.
- [ ] **Phase 2: Typed DML Support** - Add the agreed `INSERT` / `UPDATE` / `DELETE` subset, including explicit `RETURNING`, while keeping non-`RETURNING` commands no-row and rejecting predicate-free `UPDATE` / `DELETE`, then connect it to authored schema modules.
- [ ] **Phase 3: Verification and Type Expansion** - Integrate live verification into the typed SQL path and extend runtime lowering for the prioritized missing SQL types.
- [ ] **Phase 4: Public API Repointing** - Move public `query!` / `command!` onto the unified typed SQL compiler, keep raw fallbacks explicit, and add migration-facing diagnostics/tests.
- [ ] **Phase 5: Documentation** - Update the as-built reference plus public docs/examples so the new typed SQL default and fallback policy are clear.

## Phase Candidates

<!-- None yet -->

---

## Phase 1: Statement Compiler Foundation

### Changes Required:
- **`crates/macros/src/typed_sql/mod.rs`**: Replace `ParsedSelect`-only / query-only orchestration with statement-kind-aware typed SQL entrypoints and shared typed statement abstractions.
- **`crates/macros/src/typed_sql/ir.rs`**: Introduce a new statement-shape-aware IR module and migrate or re-export the current `ParsedSelect` / `CheckedSelect` / `LoweredQuery` families out of `mod.rs` so row-returning and command-style statements share one compiler vocabulary.
- **`crates/macros/src/typed_sql/public_schema.rs`**: Keep the current schema ingestion/orchestration seam, but lift it so later phases can compile both query and command forms through one typed compiler front door.
- **`crates/macros/src/typed_sql/public_input.rs`** and **`crates/macros/src/typed_sql/source.rs`**: Preserve token/literal input, placeholder canonicalization, and optional-group handling while making them reusable across future statement kinds.
- **Tests**:
  - `crates/macros/src/typed_sql/*` unit coverage for new statement-kind abstractions
  - `crates/core/tests/typed_query_ui.rs`
  - existing `crates/core/tests/ui/typed_query/**` pass/fail suites as regression coverage

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar-macros --lib`
- [ ] Tests pass: `cargo test -p babar --test typed_query_ui`
- [ ] Lint/typecheck: `cargo clippy -p babar-macros --lib -- -D warnings`

#### Manual Verification:
- [ ] Existing authored-schema `typed_query!` behavior remains unchanged from a user perspective.
- [ ] The internal compiler shape is clearly statement-aware rather than still hard-coded to `SELECT`.

---

## Phase 2: Typed DML Support

### Changes Required:
- **`crates/macros/src/typed_sql/parse_backend.rs`**: Expand parsing beyond the current single-`SELECT` gate to cover the agreed `INSERT ... VALUES`, `UPDATE ... SET ... WHERE`, and `DELETE ... WHERE` subset, with explicit support for explicit-column `RETURNING`, while preserving single-statement enforcement and rejecting `INSERT ... SELECT` plus predicate-free `UPDATE` / `DELETE`.
- **`crates/macros/src/typed_sql/normalize.rs`**: Normalize the agreed DML subset into typed SQL IR while continuing to reject out-of-scope constructs such as `ON CONFLICT`, `INSERT ... SELECT`, subqueries, `WITH`, `UPDATE ... FROM`, `DELETE ... USING`, predicate-free `UPDATE` / `DELETE`, and wildcard `RETURNING *`.
- **`crates/macros/src/typed_sql/resolver.rs`**: Add statement-kind-aware schema resolution for target tables, assignments, predicates, and `RETURNING` projections; keep non-`RETURNING` `command!` statements on the command/no-row lane and reuse the existing row/projection machinery only for explicit `RETURNING`.
- **`crates/macros/src/schema_decl.rs`**: Add schema-scoped command-side bridge support so authored schema modules can ultimately expose local `query!` / `command!` wrappers as the primary authored-schema ergonomics while continuing to target the same internal `__babar_schema`-driven typed compiler contract rather than baking Phase 4 public macro names into the bridge.
- **Tests**:
  - new UI fixtures under `crates/core/tests/ui/typed_query/**` or parallel typed-command coverage for supported/unsupported DML, including predicate-free `UPDATE` / `DELETE` rejection
  - integration coverage in `crates/core/tests/sql_macro.rs`
  - cross-crate authored schema coverage patterned after `crates/core/tests/external_schema_export.rs`
  - negative coverage for preserved single-statement enforcement and `INSERT ... SELECT` rejection
  - prepare coverage for at least one static DML statement shape so the execution-model contract is checked as soon as write support lands

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test sql_macro`
- [ ] Tests pass: `cargo test -p babar --test typed_query_ui`
- [ ] Tests pass: `cargo test -p babar --test prepared`
- [ ] Lint/typecheck: `cargo clippy -p babar --tests -- -D warnings -A clippy::unused_async`

#### Manual Verification:
- [ ] Supported `INSERT`, `UPDATE`, and `DELETE` statements feel like the same schema-aware typed SQL family as read queries, with a clear split between non-`RETURNING` command/no-row behavior and explicit-`RETURNING` query-shaped row behavior.
- [ ] Unsupported DML constructs, including predicate-free `UPDATE` / `DELETE`, fail with subset diagnostics instead of falling through into confusing parser/runtime errors.
- [ ] At least one static supported DML statement is confirmed preparable, while runtime-dynamic behavior remains governed by the existing shape-dependent policy.

---

## Phase 3: Verification and Type Expansion

### Changes Required:
- **`crates/macros/src/verify.rs`**: Reuse probe/config discovery and metadata comparison as the transport layer for typed SQL verification, driven by inferred statement metadata instead of explicit codec DSL.
- **`crates/macros/src/typed_sql/public_schema.rs`**: Add typed-SQL verification orchestration so the referenced authored schema facts, referenced tables/columns, parameters, and returned/`RETURNING` columns for one typed statement can be checked against a live database.
- **`crates/macros/src/typed_sql/lower.rs`**: Extend runtime codec lowering for `uuid`, `date`, `time`, `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric`, reusing the broader core runtime codec/type layer already present in `crates/core/src/codec/` and `crates/core/src/types.rs`.
- **`crates/core/src/session/mod.rs`** and **`crates/core/src/query/fragment.rs`**: Preserve and clarify the current execution contract for runtime-dynamic typed SQL shapes; do not silently make dynamic statements appear preparable if they remain non-preparable.
- **Tests**:
  - live verification UI cases alongside `crates/core/tests/sql_macro_ui.rs` and `crates/core/tests/typed_macro_ui.rs`
  - typed SQL unsupported-type regression cases in `crates/core/tests/ui/typed_query/**`
  - integration/runtime coverage in `crates/core/tests/sql_macro.rs`
  - prepared/streaming policy coverage in `crates/core/tests/prepared.rs` and `crates/core/tests/streaming.rs`

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test typed_macro_ui --test sql_macro_ui --test typed_query_ui --test sql_macro`
- [ ] Tests pass: `cargo test -p babar-macros --lib`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`

#### Manual Verification:
- [ ] Live verification errors distinguish schema drift/config mismatch from unsupported typed SQL subset failures.
- [ ] The prioritized missing SQL types are usable in supported read and write statements for both parameters and returned columns.
- [ ] Dynamic optional typed SQL remains documented and enforced as non-preparable unless the implementation explicitly proves otherwise.

---

## Phase 4: Public API Repointing

### Changes Required:
- **`crates/macros/src/lib.rs`**: Repoint public `query!` / `command!` macro entrypoints onto the unified schema-aware typed SQL compiler, while staging compatibility behavior so the surface transition is deliberate rather than ad hoc and preserving the command/no-row vs explicit-`RETURNING` query-shaped split.
- **`crates/macros/src/lib.rs`**: Define the migration contract for public macro repointing: old explicit-codec `query!` / `command!` forms fail with targeted migration diagnostics; schema-generated modules expose local `query!` / `command!` wrappers as the primary authored-schema call-site contract on top of the shared internal schema-driven typed compiler bridge; and `typed_query!` remains available only as a compatibility alias during this round rather than the primary documented surface.
- **`crates/core/src/lib.rs`**: Update macro exports and crate-level docs to present schema-aware `query!` / `command!` as the primary typed SQL story and demote `sql!` from first-class guidance.
- **`crates/core/src/query/mod.rs`**: Preserve `Query::raw` / `Command::raw` as advanced extended-protocol escape hatches with clear documentation boundaries.
- **`crates/core/tests/typed_macro_ui.rs`**, **`crates/core/tests/sql_macro_ui.rs`**, **`crates/core/tests/sql_macro.rs`**: Add migration-facing coverage showing the new `query!` / `command!` behavior, unsupported-subset diagnostics, and raw fallback expectations.
- **Inline-schema regression coverage**: Preserve inline schema reads and supported writes as examples/tests-only flows so FR-004 remains true after public macro repointing.
- **Examples / migration-facing touchpoints**:
  - `crates/core/examples/axum_service.rs`
  - `crates/core/examples/quickstart.rs`
  - other small examples that currently present raw or explicit-codec macros as the primary path

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test --all-features`
- [ ] Lint/typecheck: `cargo fmt --check`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`

#### Manual Verification:
- [ ] A user reading the public macro API sees schema-aware `query!` / `command!` as the default path rather than `typed_query!`, `sql!`, or explicit codec tuples, and authored-schema modules present local `query!` / `command!` wrappers as the primary call-site contract.
- [ ] A user with an older explicit-codec macro call site gets a clear migration diagnostic, and inline schema examples/tests still compile on the preserved examples/tests path.
- [ ] Raw `Query::raw` / `Command::raw` remain available and clearly positioned as advanced fallbacks, not accidental leftovers.
- [ ] `simple_query_raw` remains clearly reserved for migrations/control-plane SQL rather than being reintroduced as a default app-level path.
- [ ] Migration-facing examples/tests provide a concrete sign-off hook that `sql!` has been phased out as the default application guidance in practice rather than only in wording.

---

## Phase 5: Documentation

### Changes Required:
- **`.paw/work/typed-sql-unification/Docs.md`**: Technical reference covering the unified typed SQL architecture, supported SQL subset, verification behavior, prioritized type expansion, execution constraints, and raw/simple-query fallback policy.
- **Project docs**:
  - `README.md`
  - `docs/getting-started/first-query.md`
  - `docs/book/02-selecting.md`
  - `docs/book/03-parameterized-commands.md`
  - `docs/book/04-prepared-and-streaming.md`
  - `docs/book/11-web-service.md`
  - any other touched examples or guidance pages identified during implementation
- **Documentation content goals**:
  - present schema-aware `query!` / `command!` as the default
  - explain the supported typed SQL subset and explicit non-goals
  - explain the first-round read subset boundary in the same explicit way as the write subset
  - explain that non-`RETURNING` `command!` stays command/no-row while explicit `RETURNING` currently lowers through query-shaped row behavior
  - explain that predicate-free `UPDATE` / `DELETE` are out of scope and produce unsupported-subset diagnostics
  - explain live verification inputs and failure modes
  - explain the migration contract for old explicit-codec macro forms, the authored-schema local `query!` / `command!` wrapper story, and the compatibility-only positioning of `typed_query!`
  - explain when to use `Query::raw` / `Command::raw` vs `simple_query_raw`
  - explain prepare/streaming limitations for runtime-dynamic typed SQL

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Docs build: `mdbook build`

#### Manual Verification:
- [ ] Public docs no longer present `sql!` or the old explicit-codec `query!` / `command!` model as the primary path.
- [ ] The raw/simple-query fallback story is easy to understand and consistent across README, book docs, and examples.
- [ ] Prepare/streaming and verification caveats are explicit enough that users do not infer broader support than the implementation actually provides.
- [ ] Documentation/examples sign-off makes the `sql!` phase-out concrete: primary guides and migration touchpoints show `query!` / `command!`, while any retained `sql!` usage is clearly compatibility-only or tied to a non-primary fallback story.

---

## References

- Issue: none
- Spec: `.paw/work/typed-sql-unification/Spec.md`
- Research: `.paw/work/typed-sql-unification/CodeResearch.md`
- Work shaping: `.paw/work/typed-sql-unification/WorkShaping.md`
