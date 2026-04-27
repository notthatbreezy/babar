# Compile Time SQL Verification Plan

## Summary
Add an **optional live-database verification mode** for babar's SQL macros. Normal builds should keep today's behavior and require no database. When a database is configured for macro expansion, babar should verify SQL earlier and fail the build on schema mismatches.

The requested scope includes both:
- `sql!`
- raw/prepared query workflows

The key design constraint is that `Query::raw` and `Command::raw` are runtime constructors, so compile-time verification for those use cases requires a **macro-backed typed constructor surface** that emits ordinary `Query` / `Command` values.

## Current State Analysis
- `sql!` in `crates/macros/src/lib.rs` currently rewrites named placeholders into positional `$1`, `$2`, ... slots and captures callsite origin metadata. It does **not** parse SQL or contact a database.
- `Query::raw` / `Command::raw` in `crates/core/src/query/mod.rs` are plain runtime functions. They cannot participate in compile-time verification directly.
- `Session::prepare_query` / `prepare_command` already do runtime schema validation against server metadata, so verified macros should ideally reuse the same encoder/decoder metadata model and produce the same `Query` / `Command` values those APIs accept.
- Project docs currently state that compile-time schema verification is out of scope, so this feature must intentionally revise that stance while preserving the library's explicit/optional philosophy.

## Key Decisions
- **Online only for v1**: verification is available only when a database/config is provided at build time. No offline cache or generated schema snapshot in the first cut.
- **Optional by configuration**: no DB config means macros fall back to today's normal behavior.
- **Verification runs inside `babar-macros`** using a live PostgreSQL metadata probe rather than a separate companion crate in the first version.
- **Macro-backed typed verification**: compile-time verification for raw/prepared workflows will come from new `query!` / `command!` macros that emit ordinary `Query` / `Command` values.
- **Verifiable codec DSL in v1**: the verified macro path will initially support a parseable codec subset the proc macro can map to PostgreSQL OIDs (`int2`, `int4`, `int8`, `bool`, `text`, `varchar`, `bytea`, and `nullable(...)`, plus tuples of these). Existing arbitrary codec expressions and derived struct codecs remain usable through runtime APIs, but are not part of the first compile-time verification surface.
- **`sql!` stays narrower**: by itself it can verify SQL parsing and parameter metadata only when its bindings are in the verifiable subset; full row-shape checking belongs on typed query/command macros that also see decoder metadata.

## Work Items

### 1. `sql-verify-architecture`
- Define build-time configuration discovery (`BABAR_DATABASE_URL`, `DATABASE_URL`, or babar-specific override).
- Decide whether verification support lives directly in `babar-macros` or via a helper/companion crate.
- Define failure policy:
  - no config => skip verification
  - config present but unreachable/invalid => compile error
- Lock the verified macro API shape for typed queries and commands.

### 2. `sql-verify-engine`
- Build the live metadata probe used during macro expansion.
- Verify parameter metadata against the declared encoder OIDs/count.
- Verify row metadata against the declared decoder OIDs/count for query-producing macros.
- Produce callsite-focused diagnostics for parse, connectivity, and schema mismatch failures.

### 3. `sql-verify-macro-surface`
- Add verified typed statement macros that emit normal `Query` / `Command` values and therefore work with:
  - `Session::query`
  - `Session::execute`
  - `Session::prepare_query`
  - `Session::prepare_command`
  - transaction/pool equivalents
- Integrate optional verification into `sql!` for the subset of checks it can support cleanly.
- Preserve origin metadata and existing codec ergonomics.

### 3a. `sql-verify-sql-macro`
- Extend `sql!` to invoke the same optional verifier when:
  - a verification database URL is configured
  - every bound codec expression is in the verifiable subset
- Keep current expansion behavior unchanged when verification is unavailable or intentionally skipped.

### 4. `sql-verify-validation-docs`
- Add trybuild and integration coverage for verified and unverified builds.
- Add live-schema compile-fail tests for mismatched params and result shapes.
- Update examples and docs to explain:
  - optional verification behavior
  - recommended typed verified macros
  - no-offline-cache limitation for v1

## Suggested Phase Breakdown

### Phase 1: Architecture and public API
- Decide config source, helper crate strategy, and verified macro names.
- Write the compatibility story for existing runtime constructors.

### Phase 2: Live verification engine
- Add compile-time DB probing and metadata normalization.
- Implement comparison logic and diagnostics.

### Phase 3: Typed verified macros
- Ship macro-backed `Query` / `Command` construction.

### Phase 4: `sql!` verification integration
- Add best-effort verification to `sql!` for supported binding codecs.

### Phase 5: Tests, examples, and docs
- Cover no-config fallback and verified mode.
- Update repo docs to describe the new optional capability.

## Notes
- The most important non-obvious constraint is that full compile-time verification cannot be bolted into `Query::raw` / `Command::raw` themselves. Those remain runtime APIs; the verified path must be macro-driven.
- Prepared statements do not need their own separate compile-time surface if verified macros already produce ordinary `Query` / `Command` values.
- The first shipping version deliberately optimizes for a coherent, verifiable typed macro DSL rather than universal support for arbitrary codec expressions at compile time.

## Status Notes
- 2026-04-27: Added typed-macro trybuild coverage for invalid configuration plus live parameter/row mismatches, and updated rustdoc / README / PLAN / MILESTONES / CLAUDE to describe optional online verification, the `query!` / `command!` surface, the v1 verifiable codec subset, and no-config fallback behavior.
