# Minor API Improvements Implementation Plan

## Overview

Implement the accepted minor-improvements bundle as one coherent cleanup pass
across the API surface, generated macro wrappers, and reader-facing
documentation. The implementation should simplify raw-statement construction,
remove transitional typed-SQL naming from the public story, and then align the
main docs path around one primary `query!` / `command!` narrative with
struct-first examples after the earliest introductory material.

The plan intentionally keeps the architectural center of gravity where it is
today. `Query`, `Command`, the schema-aware macro pipeline, and authored schema
wrappers already exist; this work should tighten and rename those surfaces
rather than add another layer. The documentation work is therefore the final
integration phase: once the primary API names and generated wrappers are
settled, the docs can teach one stable story instead of describing a
transition.

## Current State Analysis

The raw fallback builders in `crates/core/src/query/mod.rs` currently expose
only explicit-codec constructors: `Command::raw(sql, encoder)` and
`Query::raw(sql, encoder, decoder)`. That forces no-parameter statements to
spell an empty codec placeholder even when the caller only wants a simple typed
fallback for DDL or a no-bind `SELECT`.

Transitional typed-SQL naming is still present across the public and
product-facing surface. `crates/core/src/lib.rs` still re-exports
`typed_query!`; `crates/macros/src/lib.rs` and
`crates/macros/src/schema_decl.rs` still describe or emit `typed_query` /
`typed_command` compatibility entrypoints; and user-visible diagnostics in
`crates/macros/src/typed_sql/public_schema.rs` and `crates/core/src/session/mod.rs`
still mention `typed_query!` directly.

The docs also still tell a mixed story. `docs/index.md`,
`docs/getting-started/first-query.md`, `docs/book/03-parameterized-commands.md`,
`docs/explanation/design-principles.md`, `docs/explanation/what-makes-babar-babar.md`,
`docs/reference/errors.md`, and crate-level rustdoc in `crates/core/src/lib.rs`
still mix transitional naming, tuple-heavy examples, and explanation language
that does not yet draw sharp boundaries between onboarding, book, explanation,
and reference roles. `docs/SUMMARY.md` also does not yet include a dedicated
macro-architecture explanation page.

## Desired End State

`Command` and `Query` expose dedicated raw constructors that separate the
zero-parameter case from the explicit-codec case without changing the underlying
statement model. Public typed SQL presents one naming scheme centered on
`query!`, `command!`, and schema-scoped `query!` / `command!` wrappers, while
generated aliases and user-facing diagnostics stop teaching compatibility names.

The documentation path then reflects that same single-surface story. Introductory
material can keep tuples only where brevity is doing real work, but the normal
examples past the first steps should use named structs for multi-field values.
The final docs set should give the landing page, getting-started flow, book,
reference pages, and explanation pages distinct roles, and it should include one
dedicated explanation page that describes the macro pipeline at an
architecture-level for advanced Rust readers.

## What We're NOT Doing

- Expanding the typed SQL subset or broadening supported SQL constructs as part
  of this cleanup
- Adding a new transition layer, fallback alias, or compatibility shim to keep
  `typed_query` / `typed_command` user-facing
- Rewriting every tuple example in the docs, including the very first
  introductory snippets where tuples are intentionally shortest
- Reorganizing the entire documentation site or tutorial set beyond the targeted
  surfaces called out in the spec
- Turning the macro explanation into line-by-line source commentary or a full
  internal reference dump

## Phase Status
- [ ] **Phase 1: Raw Builder Ergonomics** - Split raw statement construction into dedicated zero-parameter and explicit-codec entrypoints.
- [ ] **Phase 2: Primary Naming Cleanup** - Remove public transitional typed-SQL names and align generated wrappers, diagnostics, and regression coverage with the single primary surface.
- [ ] **Phase 3: Documentation** - Update project docs and the PAW as-built docs so the published story matches the cleaned-up API and Diataxis boundaries.

## Phase Candidates

---

## Phase 1: Raw Builder Ergonomics

### Changes Required:
- **`crates/core/src/query/mod.rs`**: Rename the explicit-codec raw constructors to `raw_with(...)` and add dedicated zero-parameter `raw(...)` constructors for `Command` and `Query`, while keeping fragment-based constructors unchanged.
- **`crates/core/src/lib.rs`**: Refresh crate-level examples and API descriptions that currently teach `Command::raw(..., ())` or only the explicit-codec form so rustdoc matches the new builder split.
- **`crates/core/tests/sql_macro.rs`**: Update raw-builder parity assertions so they cover the renamed explicit-codec path and the new no-parameter raw path.
- **`crates/core/src/query/mod.rs` tests**: Add or expand focused unit coverage around `sql()`, declared OIDs, and zero-parameter behavior for the new constructor split.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --lib query::tests`
- [ ] Tests pass: `cargo test -p babar --test sql_macro`
- [ ] Lint/typecheck: `cargo check -p babar --all-features`

#### Manual Verification:
- [ ] A no-parameter raw command reads as `Command::raw("...")` instead of requiring an empty codec placeholder.
- [ ] A no-parameter raw query keeps its explicit row decoder while avoiding a placeholder parameter codec, and parameterized raw statements still have a distinct `raw_with(...)` path.

---

## Phase 2: Primary Naming Cleanup

### Changes Required:
- **`crates/macros/src/lib.rs`**: Remove the public `typed_query!` compatibility entrypoint, update proc-macro docs/comments, and rewrite public guidance/error messages so they point only to `query!`, `command!`, schema-scoped wrappers, and raw fallbacks.
- **`crates/macros/src/schema_decl.rs`**: Stop generating `typed_query` / `typed_command` aliases inside authored schema modules so local wrappers expose only the primary names.
- **`crates/macros/src/typed_sql/public_schema.rs`**: Rewrite schema-front-door diagnostics that still say `typed_query!` so compile-time errors describe the current public entrypoints and authored-wrapper behavior accurately.
- **`crates/core/src/lib.rs`**: Remove the public `typed_query!` re-export and align crate-level API docs with the primary naming scheme.
- **`crates/core/src/session/mod.rs`**: Update runtime-facing preparation errors for dynamic typed SQL so they no longer mention removed compatibility names.
- **`crates/core/tests/typed_macro_ui.rs`**, **`crates/core/tests/typed_query_ui.rs`**, **`crates/core/tests/sql_macro.rs`**, and **`crates/core/tests/ui/**`**: Replace alias-preservation assertions with coverage that proves the primary macros and schema-scoped wrappers remain the supported surface and that removed names fail in clear ways where appropriate.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo test -p babar --test typed_macro_ui --test typed_query_ui --test sql_macro --all-features`
- [ ] Tests pass: `cargo test -p babar-macros --lib`
- [ ] Lint/typecheck: `cargo fmt --check`

#### Manual Verification:
- [ ] Public rustdoc and generated schema modules teach only `query!` / `command!` and schema-scoped wrappers as the typed SQL surface.
- [ ] User-visible compile-time and runtime errors no longer tell readers to use or expect `typed_query!` / `typed_command!`.

---

## Phase 3: Documentation

### Changes Required:
- **`.paw/work/minor-api-improvements/Docs.md`**: Record the as-built API renames, public-surface cleanup, affected docs inventory, and verification approach for the completed wave.
- **`docs/index.md`** and **`docs/getting-started/first-query.md`**: Reframe the onboarding path so it introduces the primary typed SQL surface directly, uses less self-promotional language, and moves toward named structs as soon as the reader is past the shortest first-step examples.
- **`docs/book/02-selecting.md`** and **`docs/book/03-parameterized-commands.md`**: Keep the book aligned with the new raw/raw_with split, remove transition framing around typed SQL naming, and make representative multi-field examples struct-shaped instead of tuple-first.
- **`docs/explanation/what-makes-babar-babar.md`** and **`docs/explanation/design-principles.md`**: Tighten explanation pages around concrete design questions and tradeoffs instead of self-congratulatory framing, and make their explanatory role distinct from the book and reference sections.
- **`docs/reference/errors.md`** and any closely related reference surfaces: Remove stale `typed_query!` references and keep lookup-style pages focused on facts rather than narrative migration notes.
- **`docs/explanation/` (new macro explanation page)** and **`docs/SUMMARY.md`**: Add one dedicated macro-pipeline explanation page covering entrypoints, authored schema wrappers, parsing, resolution, verification, lowering, diagnostics, and runtime statement shapes; wire it into the published docs navigation.
- **`crates/core/src/lib.rs`**: Keep crate-level rustdoc examples and section boundaries consistent with the revised docs story so docs.rs and mdBook do not diverge.

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `cargo +stable test --all-features`
- [ ] Lint/typecheck: `cargo +stable clippy --all-targets --all-features -- -D warnings`
- [ ] Lint/typecheck: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Docs build: `mdbook build`

#### Manual Verification:
- [ ] After the earliest introductory snippets, representative multi-field examples in the targeted onboarding and book path use named structs for params or rows.
- [ ] The targeted onboarding, book, explanation, and reference surfaces each read as distinct Diataxis roles instead of repeating the same story with different headings.
- [ ] The targeted explanation pages avoid milestone language and product self-praise while still preserving concrete tradeoffs and technical context.
- [ ] Advanced readers can find one dedicated macro explanation page from the docs navigation and understand the macro pipeline without starting from source code.

---

## References
- Issue: none
- Spec: `.paw/work/minor-api-improvements/Spec.md`
- Research: none
