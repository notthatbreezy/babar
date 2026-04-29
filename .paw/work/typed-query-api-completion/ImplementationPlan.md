# Typed Query API Completion Implementation Plan

## Overview

Complete `babar`'s typed-query API as the primary schema-aware read-query surface by extending the current `typed_query!` pipeline to support suffix-style optional query behavior (`$value?`, `(...)?`) while preserving the current SQL-first feel. The implementation will stay query-only, keep inline schema plus token-style SQL as the current public entrypoint, and make omission behavior explicit, predictable, and well-covered by tests and examples.

## Current State Analysis

The current public surface already exposes `babar::typed_query!`, backed by a linear compile-time pipeline in `crates/macros/src/typed_sql/`: public proc-macro parsing in `public_schema.rs`, token-style SQL ingestion and span mapping in `public_input.rs`, placeholder canonicalization in `source.rs`, `sqlparser` backend parsing in `parse_backend.rs`, normalization into `ParsedSelect` / `ParsedExpr` in `normalize.rs` and `ir.rs`, schema resolution and placeholder inference in `resolver.rs`, and lowering into `Query` tokens in `lower.rs` (`.paw/work/typed-query-api-completion/CodeResearch.md`). Public docs, UI tests, integration tests, and the axum example all describe this macro as an early query-only proof of concept.

The main gap is that the pipeline currently models only ordinary named placeholders (`$id`) and ordinary parenthesized SQL groups; it has no representation for suffix-style optional inputs or optional owned groups. The public token frontend currently composes `$` plus identifier into placeholder tokens, the canonicalizer only rewrites named placeholders, and the normalized IR has no notion of omission boundaries. Lowering also emits a single canonical SQL string directly from `parsed.source.canonical_sql`, so optional behavior needs a planned path from frontend representation through normalization, resolution, and final SQL emission. Documentation already teaches the current proof-of-concept shape, so the final phase must update docs/examples to reflect the completed query API and its omission rules.

## Desired End State

`typed_query!` supports a defined query-only subset in which users can declare optional single inputs with `$value?` and optional owned groups/clauses with `(...)?`, including supported `WHERE` predicate ownership and optional `ORDER BY`, `LIMIT`, and `OFFSET` behavior. The compile-time pipeline validates supported placements, rejects ambiguous or unsupported suffix syntax with location-aware diagnostics, and emits valid SQL for every supported combination of active and inactive inputs. Public examples and docs show realistic listing-style query usage, and the verification suite covers parser/frontend behavior, normalization/resolution semantics, SQL emission, UI diagnostics, and at least one service-style example.

## What We're NOT Doing

- Adding typed write support (`INSERT`, `UPDATE`, `DELETE`) in this initiative
- Building a general-purpose SQL templating or arbitrary rewrite engine
- Expanding the typed-query system to full SQL grammar coverage
- Requiring generated schema modules or a new mandatory schema authoring workflow
- Replacing SQL-first authoring with a fluent query-builder API

## Phase Status
- [ ] **Phase 1: Optional Syntax Frontend** - Extend the public macro frontend and normalized IR to represent `$value?` and `(...)?` as explicit optional syntax.
- [ ] **Phase 2: Omission Semantics and SQL Emission** - Define and implement structure-aware omission rules through resolution and final SQL generation.
- [ ] **Phase 3: Diagnostics and Coverage** - Add end-to-end diagnostics, UI coverage, execution-focused tests, and example validation for supported and unsupported optional patterns.
- [ ] **Phase 4: Documentation** - Document the completed read-query API and optional suffix semantics across PAW docs and project docs.

## Phase Candidates
- [ ] Reusable schema sources beyond inline `schema = { ... }`
- [ ] Broader typed-query SQL subset beyond the current `SELECT`-focused scope
- [ ] Query composition APIs that integrate with optional suffix semantics
- [ ] Query-plan caching / multi-shape reuse strategy for omitted-clause variants

---

## Phase 1: Optional Syntax Frontend

### Changes Required:
- **`crates/macros/src/typed_sql/public_input.rs`**: Extend token-style SQL ingestion so suffix markers on placeholders and parenthesized groups are recognized, span-mapped, and surfaced as explicit frontend constructs rather than ordinary SQL text.
- **`crates/macros/src/typed_sql/source.rs`**: Extend canonical source handling to distinguish ordinary named placeholders from optional single-input markers and to preserve enough source metadata for later omission-aware diagnostics.
- **`crates/macros/src/typed_sql/ir.rs`**: Add normalized IR entities for optional single inputs and optional owned groups/clauses, keeping ownership boundaries explicit in the IR.
- **`crates/macros/src/typed_sql/normalize.rs`**: Normalize supported suffix-style optional syntax into the new IR forms while rejecting unsupported placements before later stages assume ordinary SQL structure.
- **`crates/macros/src/typed_sql/mod.rs`**: Keep staged error rendering aligned with the new frontend/IR error cases.
- **Tests**: Extend module/unit coverage in `crates/macros/src/typed_sql/public_input.rs` and `crates/macros/src/typed_sql/mod.rs`; add or expand corpus fixtures under `crates/macros/tests/typed_query/corpus/` for supported and unsupported suffix-style input.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar-macros typed_sql --all-features`
- [ ] Lint/typecheck: `cargo clippy -p babar-macros --lib -- -D warnings`

#### Manual Verification:
- [ ] A supported token-style query using `$value?` and `(...)?` parses as typed-query input without degrading source-span diagnostics.
- [ ] Unsupported suffix placements fail in the frontend/normalization path rather than leaking as opaque downstream parse failures.

---

## Phase 2: Omission Semantics and SQL Emission

### Changes Required:
- **`crates/macros/src/typed_sql/resolver.rs`**: Extend checked-query analysis to track active/inactive optional inputs and owned omission boundaries, including supported predicate ownership and optional tail-clause semantics.
- **`crates/macros/src/typed_sql/lower.rs`**: Replace the current direct canonical-SQL emission path with omission-aware SQL generation that can produce valid SQL for supported active-input combinations while preserving inferred codec tuples and origin metadata.
- **`crates/macros/src/typed_sql/public_schema.rs`**: Keep end-to-end `typed_query!` compilation wired through the omission-aware checked/lowered path.
- **`crates/macros/src/typed_sql/parse_backend.rs` / `normalize.rs` / `ir.rs`**: Adjust any shared boundary types needed so optional groups that survive frontend parsing can be validated and lowered coherently.
- **Tests**: Add resolver/lowering tests covering optional single predicates, grouped range predicates, optional `ORDER BY`, optional `LIMIT`, optional `OFFSET`, repeated optional placeholders, and no-active-filter cases.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar-macros --all-features`
- [ ] Lint/typecheck: `cargo clippy -p babar-macros --all-features -- -D warnings`

#### Manual Verification:
- [ ] A listing-style query with several optional filters emits valid SQL when only a subset of inputs is active.
- [ ] A grouped optional predicate is omitted as a whole when its required grouped inputs are incomplete, with no partial-condition emission.

---

## Phase 3: Diagnostics and Coverage

### Changes Required:
- **`crates/core/tests/typed_query_ui.rs`** and **`crates/core/tests/ui/typed_query/**`**: Add compile-fail coverage for unsupported suffix syntax placements, ambiguous ownership boundaries, and invalid optional tail-clause usage.
- **`crates/core/tests/sql_macro.rs`**: Add integration coverage showing `typed_query!` parity/execution behavior for supported optional suffix queries against Postgres.
- **`crates/core/examples/axum_service.rs`**: Expand the example, if needed, to show realistic optional listing filters/order/pagination behavior using the completed API.
- **`crates/macros/src/typed_sql/mod.rs`** / **`public_input.rs`** / **`resolver.rs`**: Ensure staged diagnostics and source excerpts remain specific for optional-syntax failures.
- **Tests**: Keep corpus fixtures, unit tests, UI tests, and integration tests aligned so parser, resolver, lowering, and user-facing diagnostics are all covered.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test --all-features`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`

#### Manual Verification:
- [ ] At least one runnable service-style example demonstrates optional filtering/listing behavior with the completed query API.
- [ ] Error messages for unsupported optional syntax point at the relevant suffix/group region and explain the ownership rule being violated.

---

## Phase 4: Documentation

### Changes Required:
- **`.paw/work/typed-query-api-completion/Docs.md`**: Technical reference describing the completed optional-suffix query behavior, omitted-clause semantics, and verification approach.
- **`README.md`**: Update the typed-query overview to describe the completed query-only API and its optional suffix semantics.
- **`docs/book/02-selecting.md`** and **`docs/book/03-parameterized-commands.md`**: Teach the supported `$value?` / `(...)?` read-query behavior and reinforce the query-only boundary.
- **`docs/getting-started/first-query.md`**, **`docs/explanation/what-makes-babar-babar.md`**, and related typed-query docs surfaced by implementation changes: keep public guidance aligned with the final read-query behavior.
- **Project docs build**: Use the discovered mdBook build command from CodeResearch.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Verification suite passes: `cargo test --all-features`

#### Manual Verification:
- [ ] Documentation clearly explains when `$value?` removes an owned predicate and when `(...)?` removes an owned group/clause.
- [ ] Public docs and examples consistently present the API as query-only, SQL-first, and intentionally narrower than a general SQL rewrite system.

---

## References
- Issue: none
- Spec: `.paw/work/typed-query-api-completion/Spec.md`
- Research: `.paw/work/typed-query-api-completion/CodeResearch.md`
