# Minor API Improvements

## Overview

This work tightens the public typed-SQL story for `babar` without changing the
underlying architecture of typed statements, schema-aware macros, or runtime
execution. The implementation focuses on three outcomes:

1. raw statements with no bind parameters read naturally through dedicated
   `raw(...)` constructors
2. the public typed-SQL surface teaches one naming scheme centered on
   `query!`, `command!`, and schema-scoped wrappers
3. the main documentation path introduces struct-shaped examples earlier and
   separates onboarding, explanation, and reference content more clearly

The result is a smaller public surface with less transition-era language and a
clearer path for both everyday users and advanced readers.

## Architecture and Design

### High-Level Architecture

The implementation leaves the existing runtime and macro pipeline intact:

- `Query<A, B>` and `Command<A>` remain the core typed statement values
- schema-aware `query!` / `command!` still compile through the same typed-SQL
  parser, resolver, verification, and lowering path
- `schema!` still generates authored schema modules and schema-scoped wrappers
- mdBook and crate rustdoc continue to describe the same public API from
  different presentation surfaces

The changes are therefore surface-level integrations rather than a rework of
statement execution or macro internals:

- `raw(...)` / `raw_with(...)` split builder ergonomics in `crates/core`
- transitional `typed_query` / `typed_command` names are removed from public
  exports, generated wrappers, and user-facing diagnostics
- docs surfaces are revised to present one stable product story

### Design Decisions

#### Separate zero-parameter and explicit-codec raw builders

The accepted API direction was:

- `Command::raw(sql)` and `Query::raw(sql, decoder)` for no-parameter raw
  statements
- `Command::raw_with(sql, encoder)` and
  `Query::raw_with(sql, encoder, decoder)` for explicit-codec raw statements

This keeps the advanced raw path available while removing placeholder empty
codec tuples for common bootstrap and setup cases.

#### Remove transitional typed-SQL names instead of de-emphasizing them

Because the project is still greenfield, the implementation removes
`typed_query!` and schema-scoped `typed_query!` / `typed_command!` aliases from
the public and product-facing surface instead of preserving them as a migration
story. Diagnostics and rustdoc were updated to match the current API rather
than explaining compatibility layers.

#### Use docs role separation, not just copy edits

The documentation changes are organized so the main surfaces do different jobs:

- landing page and getting-started content orient new readers
- book chapters teach task-oriented usage patterns
- explanation pages answer design and architecture questions
- reference pages stay lookup-oriented

This keeps the docs set from repeating the same narrative in multiple places.

#### Prefer named structs after the first introductory steps

Tuple examples remain in the very earliest material where they are the shortest
way to explain statement shape, but representative multi-field examples later in
the docs use named structs for params or rows. This matches how users are more
likely to write application code once they move past the first introduction.

### Integration Points

- **`crates/core/src/query/mod.rs`**: raw constructor split for `Query` and
  `Command`
- **`crates/macros/src/lib.rs`** and **`crates/macros/src/schema_decl.rs`**:
  public typed-SQL entrypoint cleanup and generated wrapper cleanup
- **`crates/macros/src/typed_sql/public_schema.rs`** and
  **`crates/core/src/session/mod.rs`**: user-visible diagnostics aligned with
  the cleaned public naming
- **`crates/core/tests/**`**: targeted UI/runtime coverage updated for removed
  aliases and new raw-builder behavior
- **`docs/**`** and **`crates/core/src/lib.rs`**: published docs surfaces aligned
  with the same API story

## User Guide

### Prerequisites

- Use the current `babar` crate version from this branch/worktree
- Prefer authored `schema!` modules or inline `schema = { ... }` blocks with
  `query!` / `command!` for typed SQL
- Use raw statement builders only when the schema-aware typed-SQL subset is not
  the right fit

### Basic Usage

For normal typed SQL, the primary surface is:

- `babar::query!(...)`
- `babar::command!(...)`
- `app_schema::query!(...)`
- `app_schema::command!(...)`

For raw fallbacks:

- use `Command::raw("...")` for commands with no bind parameters
- use `Query::raw("...", row_decoder)` for queries with no bind parameters
- use `Command::raw_with("...", encoder)` when parameters still need explicit
  codecs
- use `Query::raw_with("...", encoder, decoder)` when both parameter and row
  codecs are explicit

The docs and rustdoc now teach that split consistently.

### Advanced Usage

Advanced readers who want the macro architecture should use the dedicated
explanation page in `docs/explanation/typed-sql-macro-pipeline.md`. That page
describes:

- public macro entrypoints
- authored schema wrappers
- parsing and normalization
- statement resolution and validation
- live verification boundaries
- lowering into runtime `Query` / `Command` shapes
- where diagnostics are produced

This page is intended to explain the pipeline at an architectural level without
requiring readers to begin by tracing source files.

## API Reference

### Key Components

- **`Query::raw(sql, decoder)`**: no-parameter raw query builder
- **`Query::raw_with(sql, encoder, decoder)`**: explicit-codec raw query builder
- **`Command::raw(sql)`**: no-parameter raw command builder
- **`Command::raw_with(sql, encoder)`**: explicit-codec raw command builder
- **`query!` / `command!`**: primary schema-aware typed-SQL entrypoints
- **`schema!`**: authored schema module generator with local typed-SQL wrappers

### Configuration Options

- Compile-time verification still uses `BABAR_DATABASE_URL` first and
  `DATABASE_URL` second
- The typed-SQL subset remains intentionally narrow; unsupported statements
  still use raw builders or lower-level `sql!` composition where appropriate
- Prepared statements still reject runtime-dependent optional typed SQL, but
  the user-facing errors now describe the current typed-SQL surface instead of
  removed compatibility names

## Testing

### How to Test

Targeted validation for this work includes:

- raw-builder and typed-SQL UI/runtime test suites in `crates/core/tests`
- crate rustdoc tests for `babar`
- full workspace docs build through `mdbook build`
- full feature validation through the project’s Cargo test/clippy/doc commands

Representative commands:

- `cargo +stable test --all-features`
- `cargo +stable clippy --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- `mdbook build`

### Edge Cases

- A raw query can bind no parameters while still requiring an explicit row
  decoder; `Query::raw(sql, decoder)` keeps that decoder visible.
- The earliest intro snippets may still use tuples where that reduces ceremony,
  but later representative examples should use named structs for multi-field
  values.
- Runtime-dependent optional typed SQL still cannot be prepared as a named
  server-side statement; those errors now reference the current API surface.
- Removing compatibility aliases required updating compile-fail fixtures and
  stderr output, because some previous tests asserted the old alias behavior.

## Limitations and Future Work

- This work does not expand the typed-SQL subset or add new SQL support.
- It intentionally does not preserve compatibility aliases for `typed_query` or
  `typed_command`.
- The docs reorganization is targeted, not a full site redesign.
- The macro explanation is architectural rather than exhaustive; source-level
  implementation details still live in the codebase.
