# Struct Support in Macros Implementation Plan

## Overview

Implement strict struct-shaped inputs and query rows for the schema-aware typed
SQL feature so users can keep using named structs across both reusable-schema
and inline-schema entry surfaces. The plan extends the existing typed-SQL macro
pipeline rather than introducing a parallel statement-building path, reusing the
current struct codec model while bringing the macro-generated contract into line
with the public docs and expected application experience.

## Current State Analysis

The current schema-aware typed SQL path is routed through
`crates/macros/src/typed_sql/public_schema.rs`, which parses the schema-aware
front door, resolves the statement contract, and lowers it into generated query
or command tokens. Lowering in `crates/macros/src/typed_sql/lower.rs` currently
emits tuple-shaped parameter and row codecs, so the generated statement contract
is positional even when the surrounding application code is naturally
struct-shaped.

The lower-level statement builders already support struct-shaped encoding and
decoding through the existing codec model, so the missing behavior is not a
runtime capability gap. The work is primarily a contract-selection, validation,
and lowering problem inside the typed-SQL macro pipeline, plus test coverage and
docs alignment. Current UI coverage in `crates/core/tests/typed_macro_ui.rs` and
`crates/core/tests/typed_query_ui.rs` exercises the macro surfaces heavily, and
the current getting-started docs already present a struct-centric story that the
implementation does not yet consistently uphold.

## Desired End State

Users can define named structs for schema-aware command inputs, query inputs,
and query rows on both reusable-schema and inline-schema typed SQL surfaces.
The generated statement contract accepts those named structs when the statement
contract and the struct field set align, and rejects them when required fields
are missing, extra fields are present, or field type/nullability is
incompatible.

The feature must support both explicit struct selection and surrounding-type
inference where the intended shape is unambiguous, with explicit selection
taking precedence when both are present. Output row matching is based on final
output field names, including aliases, rather than projection order alone. The
final verification approach should prove the behavior through focused UI/runtime
tests and updated user-facing documentation, then close with docs integrity and
workspace docs verification.

## What We're NOT Doing

- Relaxing field matching to ignore extra struct fields
- Falling back to runtime-only shape checking for primary validation
- Replacing the typed-SQL parser backend as part of this workflow
- Redesigning raw statement builders that already support struct-shaped values
- Expanding unrelated typed-SQL subset features beyond what this struct support
  work requires

## Phase Status
- [x] **Phase 1: Struct Shape Selection Surface** - Extend the schema-aware typed SQL front door so both entry surfaces can select struct-shaped input and row contracts, including explicit-selection precedence and surrounding-type inference handoff.
- [ ] **Phase 2: Strict Struct Contract Validation and Lowering** - Validate input/output structs against the resolved statement contract, including ambiguous row-name rejection, and emit struct-aware generated statements instead of tuple-only contracts.
- [ ] **Phase 3: Documentation** - Align public docs/examples with the shipped struct-centric macro behavior, record the as-built design in `Docs.md`, and run final verification.

## Phase Candidates
- [ ] Evaluate whether the long-term typed-SQL parsing backend should move to an external SQL parser library in a separate workflow if this work exposes parser-maintenance pressure

---

## Phase 1: Struct Shape Selection Surface

### Changes Required:
- **`crates/macros/src/typed_sql/public_schema.rs`**: Extend the schema-aware front door so reusable-schema and inline-schema typed SQL entry surfaces can carry struct-shape intent for inputs and query rows, while preserving current schema-source routing and verification hooks.
- **`crates/macros/src/lib.rs`** and any directly related typed-SQL entry plumbing: Keep top-level entry behavior aligned with the same struct-selection rules used by schema-scoped expansion so both surfaces expose the same user contract.
- **Typed-SQL statement metadata flow** in the macros crate: Thread explicit struct-selection metadata and surrounding-type-inference context far enough through the pipeline that later phases can validate and lower against named shapes without inventing a second front door.
- **Tests**:
  - `crates/core/tests/typed_macro_ui.rs`
  - `crates/core/tests/typed_query_ui.rs`
  - new or updated UI fixtures under `crates/core/tests/ui/typed_macro/` and `crates/core/tests/ui/typed_query/`
  covering explicit selection, inference, and precedence/conflict cases on both macro surfaces.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test typed_macro_ui --test typed_query_ui --all-features`
- [ ] Tests pass: `cargo test -p babar --test sql_macro --all-features`

#### Manual Verification:
- [ ] The plan-visible contract supports struct-shape selection on both reusable-schema and inline-schema entry surfaces rather than only one of them.
- [ ] When explicit struct selection and surrounding-type inference are both available for the same statement, the explicit selection path is the one that governs the statement contract.

---

## Phase 2: Strict Struct Contract Validation and Lowering

### Changes Required:
- **`crates/macros/src/typed_sql/lower.rs`**: Replace tuple-only emission for eligible schema-aware statements with struct-aware generated statement contracts while preserving the existing tuple fallback for contracts that are not represented as strict named structs.
- **`crates/macros/src/typed_sql/public_schema.rs`** plus any related resolver-facing typed-SQL metadata surfaces: Validate named input fields and named output fields against the resolved statement contract, including final output-name matching for query rows, alias-aware output handling, and rejection of duplicate or otherwise ambiguous final output names before struct row matching proceeds.
- **Existing struct codec integration points** in the current statement/codec pipeline: Reuse the established struct codec model rather than adding a separate runtime representation for struct-shaped generated statements.
- **Compatibility-focused tests**:
  - `crates/core/tests/typed_query_ui.rs`
  - `crates/core/tests/typed_macro_ui.rs`
  - `crates/core/tests/sql_macro.rs`
  - any targeted runtime tests under `crates/core/tests/`
  covering:
  - missing required input fields
  - extra input fields
  - missing required output fields
  - extra output fields
  - incompatible field types and nullability
  - alias-based output matching
  - duplicate or ambiguous final output names for row structs
  - optional field/nullability interactions
  - preservation of tuple-shaped behavior where a strict named struct is not the contract

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test typed_macro_ui --test typed_query_ui --all-features`
- [ ] Tests pass: `cargo test -p babar --test sql_macro --all-features`
- [ ] Tests pass: `cargo test --all-features`

#### Manual Verification:
- [ ] A schema-aware command/query input contract accepts a named struct only when all required fields are present, no extra fields are present, and field types/nullability align with the statement contract.
- [ ] A schema-aware query row contract matches named output fields by final output field names, including aliases, rather than relying only on projection order.
- [ ] A schema-aware query row contract is rejected when the final output names are duplicated or otherwise ambiguous for strict struct matching.
- [ ] Existing tuple-shaped workflows remain available for statement contracts that are not represented as strict named structs.

---

## Phase 3: Documentation

### Changes Required:
- **User-facing docs and examples**:
  - `docs/getting-started/first-query.md`
  - `docs/book/02-selecting.md`
  - any other directly affected docs/examples discovered during implementation
  must be updated so schema-aware typed SQL teaches named structs as the default path when the statement contract can be represented as a named field set.
- **Tutorial/example simplification pass** on touched documentation examples: Use one shared struct when the statement input and query output field sets are identical, and split them only when those field sets differ.
- **`.paw/work/struct-support-in-macros/Docs.md`**: Record the as-built macro contract, validation rules, docs impact, and verification approach (load `paw-docs-guidance` during implementation).

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Lint/typecheck: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Tests pass: `cargo test --all-features`

#### Manual Verification:
- [ ] The getting-started flow and at least one book-style example present named structs, not positional shapes, as the default schema-aware typed SQL path when the statement contract can be expressed as a named field set.
- [ ] Updated examples use one shared struct when the input and output field sets are identical and use separate structs only when those field sets differ.
- [ ] `Docs.md` explains the final struct-selection rules, strict matching behavior, row-name matching rule, and the boundaries of the feature.

---

## References
- Issue: none
- Spec: `.paw/work/struct-support-in-macros/Spec.md`
- Work Shaping: `.paw/work/struct-support-in-macros/WorkShaping.md`
- Current implementation context:
  - `crates/macros/src/typed_sql/public_schema.rs`
  - `crates/macros/src/typed_sql/lower.rs`
  - `crates/core/tests/typed_macro_ui.rs`
  - `crates/core/tests/typed_query_ui.rs`
