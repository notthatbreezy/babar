# External Schemas Implementation Plan

## Overview

Implement reusable external schema support for `typed_query!` by introducing an authored Rust-visible schema declaration surface that can define multiple tables in one schema module and expose a schema-scoped query wrapper. The plan keeps the existing typed-query pipeline largely intact by compiling schema-module declarations down into the schema/catalog shape the current macro path already understands, rather than replacing resolution and lowering wholesale.

## Current State Analysis

Today, `typed_query!` only accepts `schema = { ... }, <SQL>` and builds its `SchemaCatalog` directly from the inline DSL in `crates/macros/src/typed_sql/public_schema.rs`. The typed-query pipeline after that point is already established: public SQL input parsing, placeholder canonicalization, backend parse, normalization, resolution, and lowering (`.paw/work/external-schemas/CodeResearch.md`). The repository also already has a separate Rust-visible schema layer in `crates/core/src/schema.rs` built from const-friendly table/column/type symbols, but that layer is not currently consumed by the public typed-query entrypoint.

The chosen user-facing direction is a schema-scoped wrapper rather than trying to teach the global `babar::typed_query!` proc macro to inspect arbitrary foreign Rust items directly. The planning assumption is that an authored schema declaration macro can generate reusable Rust-visible schema symbols and a local `typed_query!` wrapper that feeds the existing typed-query compilation path with synthesized schema metadata. This keeps the user API Rust-native, supports schemas that contain multiple tables, and minimizes the amount of churn required in the existing resolver/lowering internals.

## Desired End State

Users can declare a reusable authored schema module containing multiple table declarations through a Rust-visible surface, including field types, nullability, and a narrow set of important semantic markers such as primary-key identity. That schema module exposes a local `typed_query!` wrapper so users can write schema-aware queries without repeating inline schema blocks.

Implementation success means the schema declaration surface produces the schema facts required by the current typed-query pipeline, the schema-scoped wrapper compiles queries through the existing SQL-first flow, diagnostics clearly cover authored-schema misuse and current type/lowering boundaries, and docs/examples teach the recommended hybrid pattern. Verification includes targeted UI/runtime/schema tests during each phase and repo-wide lint/test/docs validation before completion.

## What We're NOT Doing

- Preserving a global-only `babar::typed_query!` entrypoint as the sole v1 external-schema interface
- Adding file-path-based, snapshot-based, or live-database schema sources in this initiative
- Supporting schema code generation or generated schema artifacts in this initiative
- Expanding the feature into write-query support or full ORM/schema-management behavior
- Reworking the core typed-query resolver/lowering architecture beyond what is needed to bridge in external schemas
- Broadening the typed-query SQL subset beyond the existing read-query scope as part of this work

## Phase Status
- [ ] **Phase 1: Schema Declaration Surface** - Add a Rust-visible schema module declaration system that can describe multiple tables and emit reusable schema symbols.
- [ ] **Phase 2: Typed Query Wrapper Bridge** - Connect schema modules to the existing typed-query pipeline through a schema-scoped wrapper and catalog bridge.
- [ ] **Phase 3: Diagnostics and Coverage** - Expand UI, runtime, and schema-level coverage for authored external schema usage and failure modes.
- [x] **Phase 4: Documentation** - Document the external schema model, recommended wrapper pattern, and verification approach.

## Phase Candidates
- [ ] Preserve or reintroduce a global `babar::typed_query!` path that can consume external schema modules directly
- [ ] Expand external schema semantics beyond the initial type/nullability/primary-key marker set
- [ ] Add schema code generation or database-introspection workflows
- [ ] Broaden runtime codec coverage beyond the current typed-query lowering limits

---

## Phase 1: Schema Declaration Surface

### Changes Required:
- **`crates/macros/src/lib.rs`**: Add the public schema declaration macro entrypoint(s) that users invoke to define external schema modules.
- **`crates/macros/src/` (new schema declaration module)**: Implement parsing and expansion for the schema-module syntax, including support for multiple table declarations in one schema and narrow field-level semantic markers.
- **`crates/core/src/schema.rs`**: Extend or align the existing Rust-visible schema primitives so authored schema modules can expose reusable table/column/type symbols through the core schema layer.
- **`crates/core/src/lib.rs`**: Re-export any new public schema macros or supporting public items and align crate-level documentation with the new surface.
- **Tests**: Add schema-surface unit coverage around authored table/column/type symbols and multi-table schema declarations in the most direct macro/schema test locations.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar schema::tests`
- [ ] Lint/typecheck: `cargo clippy -p babar-macros --all-features -- -D warnings`

#### Manual Verification:
- [ ] One schema module can declare more than one table without requiring repeated top-level setup per table.
- [ ] The authored schema surface reads as Rust-native and keeps the common field case visibly type-first.

---

## Phase 2: Typed Query Wrapper Bridge

### Changes Required:
- **`crates/macros/src/typed_sql/public_schema.rs`**: Refactor the typed-query input path so schema metadata can come from authored schema-module expansion as well as the current inline path.
- **`crates/macros/src/lib.rs`** and **schema declaration expansion code**: Generate a schema-scoped `typed_query!` wrapper that forwards authored schema metadata into the existing typed-query entry pipeline.
- **`crates/macros/src/typed_sql/resolver.rs`**: Keep the catalog boundary stable while accepting schema facts bridged from authored schema-module output.
- **`crates/macros/src/typed_sql/mod.rs`** and related boundary types: Align any error/reporting or shared typed-query interfaces needed for the wrapper path.
- **`crates/core/src/schema.rs`**: Ensure the schema module expansion exposes the symbols/metadata the wrapper bridge needs without introducing a second schema representation model for users.
- **Tests**: Add parity coverage showing schema-scoped typed queries compile through the same typed-query pipeline as the inline schema path.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test sql_macro typed_query_macro_matches_raw_builder`
- [ ] Lint/typecheck: `cargo fmt --check`

#### Manual Verification:
- [ ] A schema-scoped `typed_query!` call can resolve tables from a schema module containing multiple declared tables.
- [ ] The external-schema wrapper preserves the current SQL-first query authoring experience rather than introducing a second query-building style.

---

## Phase 3: Diagnostics and Coverage

### Changes Required:
- **`crates/core/tests/typed_query_ui.rs`** and **`crates/core/tests/ui/typed_query/**`**: Add pass/fail coverage for authored external schema references, missing tables or columns, unsupported metadata markers, and any rejected mixed inline/external usage combinations.
- **`crates/core/tests/sql_macro.rs`**: Add integration coverage for schema-scoped typed-query usage and runtime parity with the current query execution path.
- **`crates/core/src/schema.rs`** test fixtures: Expand fixture-style authored schema modules to cover reusable symbols, marker semantics, multi-table declarations, and reuse across multiple queries.
- **`crates/macros/src/typed_sql/public_schema.rs`** and **`resolver.rs`**: Keep diagnostics specific for authored external-schema misuse, catalog-resolution failures, schema-qualified identity, and schema-declared types that are accepted by the declaration surface but not yet supported by typed-query lowering.
- **Examples**: Update `crates/core/examples/axum_service.rs` or another directly user-facing example to demonstrate the recommended external schema pattern in realistic service code.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test --all-features`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`

#### Manual Verification:
- [ ] Compile-fail cases clearly distinguish missing external schema items from ordinary SQL/query mistakes.
- [ ] Unsupported declared types fail with diagnostics that explain the current external-schema/type-lowering boundary.
- [ ] One authored schema module can be shared across multiple typed queries without repeating schema declarations at each callsite.
- [ ] At least one runnable example shows the recommended schema-scoped wrapper pattern using reusable schema declarations.

---

## Phase 4: Documentation

### Changes Required:
- **`.paw/work/external-schemas/Docs.md`**: Record the as-built technical reference, usage model, marker semantics, and verification approach.
- **`README.md`**: Add the external schema model, recommended schema-scoped wrapper usage, and v1 scope boundaries.
- **`docs/getting-started/first-query.md`**: Introduce the reusable external schema story in the primary getting-started query flow if warranted by the final UX.
- **`docs/book/02-selecting.md`** and **`docs/book/03-parameterized-commands.md`**: Teach the recommended hybrid pattern and clarify how it relates to the existing inline schema path.
- **`crates/core/src/lib.rs`** crate docs and any example-adjacent docs: Keep public API teaching aligned with the new schema-scoped wrapper model.
- **API direction documentation**: Explicitly document the supported authored-schema API options, the chosen schema-scoped wrapper pattern, and the ergonomics tradeoffs that led to that recommendation.
- **Project docs content**: Explicitly document the chosen authored-schema-only v1 scope, the current supported external-schema type surface, and the current unsupported-type boundary when schema declarations can express more than lowering/runtime support.
- **Docs build verification**: Validate project docs with the discovered mdBook command.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Verification suite passes: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`

#### Manual Verification:
- [ ] Public docs show that one schema module can contain multiple tables and serve as the query context for schema-scoped typed queries.
- [ ] Docs consistently explain the v1 limits: authored Rust-visible schemas only, SQL-first queries, no file-based schema inputs, and no schema code generation.
- [ ] Docs describe the supported authored-schema API options and explain why the schema-scoped wrapper hybrid pattern is the recommended direction.
- [ ] Docs explain the currently supported external-schema type surface and how unsupported declared types are surfaced to users.

---

## References
- Issue: none
- Spec: `.paw/work/external-schemas/Spec.md`
- Research: `.paw/work/external-schemas/CodeResearch.md`
