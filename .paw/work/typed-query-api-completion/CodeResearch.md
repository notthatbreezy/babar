---
date: 2026-04-28T23:56:33-04:00
git_commit: 8be0031c9f79b15e387c3e965009ea0e560c7799
branch: feature/typed-query-macros
repository: notthatbreezy/babar
topic: "Typed query API completion"
tags: [research, typed-query, macros, parser, docs]
status: complete
last_updated: 2026-04-28
---

# Research: Typed Query API Completion

## Research Question

Document the current typed query implementation and public surface in `babar`, including where token-style SQL, inline schema DSL handling, normalization, resolution/typechecking, lowering, tests, examples, and documentation infrastructure exist today for the `typed_query!` path and where optional suffix syntax would connect to the current pipeline.

## Summary

`typed_query!` is a proc macro re-exported from the core crate that reads `schema = { ... }` plus token-style SQL, resolves that SQL against an inline schema catalog, lowers named placeholders to positional SQL, and emits an ordinary `::babar::query::Query` with inferred parameter and row codec tuples (`crates/macros/src/lib.rs:188-206`, `crates/macros/src/typed_sql/public_schema.rs:18-33`, `crates/macros/src/typed_sql/mod.rs:18-33`, `crates/macros/src/typed_sql/lower.rs:29-48`).

The current typed SQL pipeline is split into distinct stages: public token ingestion and span mapping, placeholder canonicalization, backend SQL parsing through `sqlparser`, normalization into `ParsedSelect` / `ParsedExpr`, schema resolution plus placeholder inference, and runtime lowering (`crates/macros/src/typed_sql/public_input.rs:11-53`, `crates/macros/src/typed_sql/source.rs:270-439`, `crates/macros/src/typed_sql/parse_backend.rs:9-38`, `crates/macros/src/typed_sql/normalize.rs:22-107`, `crates/macros/src/typed_sql/ir.rs:45-210`, `crates/macros/src/typed_sql/resolver.rs:340-505`, `crates/macros/src/typed_sql/lower.rs:131-148`).

The current public documentation and example material describe `typed_query!` as an early, query-only, inline-schema proof of concept for a supported `SELECT` subset; the axum example uses it for reads while writes remain `Command::raw` (`README.md:301-314`, `docs/book/02-selecting.md:53-77`, `docs/book/03-parameterized-commands.md:109-139`, `crates/core/examples/axum_service.rs:1-9`, `crates/core/examples/axum_service.rs:92-107`, `crates/core/examples/axum_service.rs:115-126`, `crates/core/examples/axum_service.rs:136-152`).

## Documentation System

- **Framework**: mdBook, configured through `book.toml` with `docs/` as the source directory and HTML output enabled (`book.toml:1-10`).
- **Docs Directory**: `docs/`, with navigation defined in `docs/SUMMARY.md` (`book.toml:1-5`, `docs/SUMMARY.md:1-39`).
- **Navigation Config**: `docs/SUMMARY.md` organizes getting started, book, reference, explanation, and tutorial sections (`docs/SUMMARY.md:1-39`).
- **Style Conventions**: the docs use tutorial/book-style Markdown with short prose sections, fenced Rust or shell code blocks, and chapter-style headings across `docs/book/*` and the README (`docs/book/02-selecting.md:1-77`, `docs/book/03-parameterized-commands.md:1-139`, `README.md:117-198`).
- **Build Command**: `mdbook build`, documented in the README and used by the Pages workflow (`README.md:196-198`, `.github/workflows/pages.yml:29-39`).
- **Standard Files**: `README.md` and `CHANGELOG.md` are at the repository root; `docs/SUMMARY.md` is the mdBook navigation file (`README.md:1-5`, `CHANGELOG.md:1`, `docs/SUMMARY.md:1-39`).

## Verification Commands

- **Test Command**: `cargo test --all-features` in CI, with the README also documenting MSRV and stable variants (`.github/workflows/ci.yml:11-23`, `README.md:184-187`).
- **Lint Command**: `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings` (`.github/workflows/ci.yml:24-35`, `README.md:174-179`).
- **Build Command**: `cargo build --all-features` works locally, while CI uses `cargo doc --workspace --no-deps` and `mdbook build` for docs validation (`README.md:181-198`, `.github/workflows/ci.yml:33-35`, `.github/workflows/pages.yml:36-44`).
- **Type Check**: the repo uses Cargo/Rust compilation; the README’s faster-iteration guidance calls out `cargo check --all-features`, and the hygiene job runs `cargo msrv verify ... cargo check` for both crates (`README.md:205-214`, `.github/workflows/ci.yml:47-53`).

## Detailed Findings

### Public surface and entrypoints

- `babar-macros` defines `typed_query` as a proc macro whose public contract is “inline schema DSL + typed_sql v1 subset + inferred parameter and row codecs,” then forwards expansion into `typed_sql::expand_typed_query` (`crates/macros/src/lib.rs:188-206`).
- The core crate re-exports `typed_query` beside `sql!`, `query!`, `command!`, and `Codec`, so the public call site is `babar::typed_query!(...)` (`crates/core/src/lib.rs:262-284`).
- Core crate docs show the macro producing a `Query<(i32,), (i32, String)>` from inline schema plus token-style SQL and state that it avoids evaluating user-authored Rust schema constants during expansion (`crates/core/src/lib.rs:143-165`).
- The runtime surface after expansion is the ordinary typed query API: `PoolConnection::query` accepts any `Query<A, B>`, and `Transaction` / `Savepoint` scopes expose the same `query` method shape (`crates/core/src/pool.rs:505-510`, `crates/core/src/transaction.rs:74-82`).

### Typed query architecture and pipeline

- `typed_sql::parse_select` canonicalizes the SQL into a `SqlSource`, then `parse_select_source` runs backend parsing followed by normalization, returning `ParsedSql { source, select }` (`crates/macros/src/typed_sql/mod.rs:18-33`).
- `expand_typed_query` parses a `TypedQueryInput`, builds a `SchemaCatalog` from the inline schema, parses the SQL as a `SELECT`, resolves it against the catalog, lowers it, and emits tokens for a `Query` (`crates/macros/src/typed_sql/public_schema.rs:18-33`).
- Lowering emits a `Fragment::__from_parts(...)` carrying the canonical SQL string, inferred parameter codec tuple, parameter count, and macro callsite origin, then wraps it with `Query::from_fragment(..., row_codec_tuple)` (`crates/macros/src/typed_sql/lower.rs:29-48`).
- The typed SQL module also carries staged error kinds (`Parse`, `Unsupported`, `Resolve`, `Type`, `Internal`) and user-facing rendering with source excerpts/help text (`crates/macros/src/typed_sql/mod.rs:37-208`).

### Schema DSL handling

- `TypedQueryInput` requires `schema = { ... }, <sql tokens>` and trims an optional trailing comma after the SQL token stream (`crates/macros/src/typed_sql/public_schema.rs:35-62`).
- The schema parser reads repeated `table ... { ... }` blocks, supports optional schema qualification (`table public.users`) and per-column `name: type` entries, and rejects duplicate table or column names while building a `SchemaCatalog` (`crates/macros/src/typed_sql/public_schema.rs:64-137`, `crates/macros/src/typed_sql/public_schema.rs:140-207`).
- Column types in the inline DSL are either a base SQL type or `nullable(inner)`, and the accepted type names are `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`, `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric` (`crates/macros/src/typed_sql/public_schema.rs:210-269`).
- Separately from the inline macro DSL, the core crate exposes const-friendly schema primitives such as `SqlType`, `Nullability`, `TableRef`, `Column`, and `Binding` in `babar::schema` (`crates/core/src/schema.rs:13-94`, `crates/core/src/schema.rs:108-189`, `crates/core/src/schema.rs:225-285`, `crates/core/src/schema.rs:325-385`).

### Token-style SQL input, canonicalization, and normalization

- `PublicSqlInput::parse` accepts either a single string literal or token-style SQL, builds a `SqlSource`, and records a span map so later typed SQL errors can be translated back to proc-macro token spans (`crates/macros/src/typed_sql/public_input.rs:11-53`).
- Token-style SQL assembly accepts identifiers, literals, punctuation, and parenthesis groups; braces and brackets are rejected, string/char literals are rendered as SQL string literals, and numeric literals must be unsuffixed and may not use `_` separators (`crates/macros/src/typed_sql/public_input.rs:63-127`, `crates/macros/src/typed_sql/public_input.rs:273-287`, `crates/macros/src/typed_sql/public_input.rs:392-415`).
- Named placeholders in token input are recognized only as `$` followed by an identifier token; the builder joins them into one SQL piece such as `$id` before canonicalization (`crates/macros/src/typed_sql/public_input.rs:129-160`).
- Canonicalization scans the SQL text, preserves strings/comments, rejects positional placeholders like `$1`, rewrites each named placeholder to a positional slot, and records every occurrence in `PlaceholderTable` / `PlaceholderOccurrence` (`crates/macros/src/typed_sql/source.rs:196-255`, `crates/macros/src/typed_sql/source.rs:270-439`).
- The backend parser accepts exactly one top-level `SELECT` statement and rejects non-`SELECT` statements, set operations, and derived top-level queries (`crates/macros/src/typed_sql/parse_backend.rs:9-38`).
- Normalization turns the backend AST into `ParsedSelect`, `ParsedProjection`, `ParsedOrderBy`, `ParsedLimit`, `ParsedOffset`, and `ParsedExpr`, with `ParsedExpr` variants limited to column refs, placeholders, literals, unary ops, binary ops, `IS NULL`, and flattened boolean chains (`crates/macros/src/typed_sql/ir.rs:45-210`, `crates/macros/src/typed_sql/normalize.rs:373-563`).

### Resolver, typechecker, and lowering

- `resolve_select` resolves the `FROM` binding, each joined table, projections, filters, order-by items, and limit/offset expressions against the `SchemaCatalog`, then performs placeholder inference and finalization before producing `CheckedSelect` (`crates/macros/src/typed_sql/resolver.rs:340-505`).
- Table resolution supports qualified and unqualified table names, with unqualified-name ambiguity reported as a resolve error; scope tracking also reports duplicate bindings or unknown aliases with the visible binding set (`crates/macros/src/typed_sql/resolver.rs:112-183`, `crates/macros/src/typed_sql/resolver.rs:507-550`).
- Expression analysis distinguishes value expressions from predicate expressions, widens nullability through outer joins, constrains placeholder types from comparisons and `LIMIT`/`OFFSET`, and rejects unconstrained placeholders during inference solving (`crates/macros/src/typed_sql/resolver.rs:821-832`, `crates/macros/src/typed_sql/resolver.rs:856-1085`, `crates/macros/src/typed_sql/resolver.rs:1360-1392`).
- Finalization produces checked expressions with concrete SQL types and nullability; bare `NULL` literals do not finalize to a concrete type (`crates/macros/src/typed_sql/resolver.rs:1087-1219`, `crates/macros/src/typed_sql/resolver.rs:1427-1435`).
- Lowering currently copies `parsed.source.canonical_sql` as the emitted SQL, lowers parameter and projection metadata into runtime codec selections, and emits tuple codec tokens for the final `Query` (`crates/macros/src/typed_sql/lower.rs:131-230`).

### Attachment points for optional suffix syntax

- Placeholder-shaped optional syntax would attach first in the public token/frontend path: token assembly currently recognizes placeholder tokens only through `$` + identifier composition, and canonicalization only rewrites named placeholders discovered by scanning `$` followed by identifier characters (`crates/macros/src/typed_sql/public_input.rs:129-160`, `crates/macros/src/typed_sql/source.rs:309-365`).
- Group-shaped optional syntax would attach in the token/AST pipeline around parenthesized groups and normalized expressions: token input preserves only parenthesis groups, `Expr::Nested(inner)` is normalized by recursing into the inner expression, and the current IR does not define any optional-group node (`crates/macros/src/typed_sql/public_input.rs:79-93`, `crates/macros/src/typed_sql/normalize.rs:410-427`, `crates/macros/src/typed_sql/ir.rs:125-210`).
- Because the backend stage currently expects ordinary PostgreSQL `SELECT` syntax and the normalizer only models the existing subset (`SELECT/FROM/JOIN/WHERE/ORDER BY/LIMIT/OFFSET` plus the current expression forms), parser/frontend work for suffix syntax would need to connect before or during normalization so it can be represented in `ParsedSelect` / `ParsedExpr` and then flow into resolution and lowering (`crates/macros/src/typed_sql/parse_backend.rs:9-38`, `crates/macros/src/typed_sql/normalize.rs:37-107`, `crates/macros/src/typed_sql/ir.rs:45-210`).

### Current tests and examples relevant to `typed_query!`

- The macro module has direct tests for placeholder canonicalization, supported-subset normalization, unsupported constructs, rendered diagnostics, and a typed query corpus rooted at `crates/macros/tests/typed_query/corpus` with parse-ok-supported, parse-ok-unsupported, and syntax-error fixture classes (`crates/macros/src/typed_sql/mod.rs:219-237`, `crates/macros/src/typed_sql/mod.rs:257-420`, `crates/macros/tests/typed_query/corpus/pg-query/parse-ok-supported/repeated_placeholder.sql:1-3`, `crates/macros/tests/typed_query/corpus/postgres-regress/parse-ok-unsupported/bare_column.sql:1`, `crates/macros/tests/typed_query/corpus/pg-parse/syntax-error/missing_from_target.sql:1`).
- Resolver unit tests cover binding/column resolution, join-driven nullability widening, placeholder propagation, alias scoping, `WHERE` predicate requirements, computed projection aliasing, unconstrained placeholders, unknown columns, and conflicting placeholder types (`crates/macros/src/typed_sql/resolver.rs:1539-1685`).
- Lowering unit tests cover successful lowering of a supported `SELECT` plus rejection of unsupported projection or parameter runtime codecs (`crates/macros/src/typed_sql/lower.rs:281-347`).
- Core integration tests verify that `typed_query!` can match an equivalent `Query::raw`, and that the resulting query executes against Postgres in the broader SQL macro integration test file (`crates/core/tests/sql_macro.rs:155-177`, `crates/core/tests/sql_macro.rs:232-320`).
- UI coverage for the macro lives in `crates/core/tests/typed_query_ui.rs`, with a passing example and compile-fail cases for unsupported schema types and unknown columns (`crates/core/tests/typed_query_ui.rs:1-9`, `crates/core/tests/ui/typed_query/pass/basic.rs:1-14`, `crates/core/tests/ui/typed_query/fail/unsupported_type.rs:1-12`, `crates/core/tests/ui/typed_query/fail/unknown_column.rs:1-13`).
- The axum example uses `typed_query!` for both list and lookup reads while the write path stays on `Command::raw`, which documents the current read/query focus in runnable code (`crates/core/examples/axum_service.rs:1-9`, `crates/core/examples/axum_service.rs:92-107`, `crates/core/examples/axum_service.rs:115-126`, `crates/core/examples/axum_service.rs:136-152`).

### Documentation surfaces relevant to typed queries

- The README’s compile-time SQL verification section documents `typed_query!` as an inline-schema, named-placeholder macro that lowers to ordinary `Query` values and reuses parameter slots for repeated placeholder names (`README.md:255-314`).
- Book chapter 2 introduces the macro as a schema-aware alternative that still yields a `Query<P, R>` value and describes its current scope as token-style SQL + inline schema + supported `SELECT` subset only (`docs/book/02-selecting.md:53-77`).
- Book chapter 3 positions `typed_query!` alongside `raw` and `sql!`, describing it as query-only and explicitly noting that writes still use `Command<A>` through the existing surfaces (`docs/book/03-parameterized-commands.md:79-139`).
- The error reference notes that macro-produced `origin` metadata includes `typed_query!` callsites for runtime errors such as `Closed`, `Server`, `ColumnAlignment`, and `SchemaMismatch` (`docs/reference/errors.md:25-29`).

### Observed constraints and limits present today

- Public docs and the axum example consistently describe the current macro as query-only / `SELECT`-only rather than a write surface (`README.md:301-314`, `docs/book/03-parameterized-commands.md:131-139`, `crates/core/examples/axum_service.rs:3-5`).
- The normalized subset currently excludes many SQL features, including `WITH`, `FETCH`, locking clauses, `DISTINCT`, wildcard projections, non-plain table factors, and expression forms outside the enumerated subset (`crates/macros/src/typed_sql/normalize.rs:37-138`, `crates/macros/src/typed_sql/normalize.rs:149-190`, `crates/macros/src/typed_sql/normalize.rs:252-274`, `crates/macros/src/typed_sql/normalize.rs:373-427`).
- Inline schema parsing accepts a broader set of SQL types than runtime lowering currently supports; lowering currently maps only bool/bytea/varchar/text/int2/int4/int8/float4/float8 (plus nullable wrappers) into runtime codecs and returns an unsupported error for other SQL types (`crates/macros/src/lib.rs:191-203`, `crates/macros/src/typed_sql/public_schema.rs:243-269`, `crates/macros/src/typed_sql/lower.rs:183-223`, `crates/macros/src/typed_sql/lower.rs:322-347`).
- Placeholder inference requires every placeholder to become constrained to a concrete SQL type before finalization, and bare `NULL` literals do not finalize to a concrete type (`crates/macros/src/typed_sql/resolver.rs:1360-1392`, `crates/macros/src/typed_sql/resolver.rs:1120-1125`, `crates/macros/src/typed_sql/resolver.rs:1652-1659`).
- Token-style SQL input currently accepts only parentheses grouping in proc-macro tokens and treats placeholders as named `$ident` forms rather than positional `$1` forms (`crates/macros/src/typed_sql/public_input.rs:79-93`, `crates/macros/src/typed_sql/public_input.rs:129-160`, `crates/macros/src/typed_sql/source.rs:309-317`).

## Code References

- `crates/macros/src/lib.rs:188-206` - public `typed_query` proc macro entrypoint and documented macro surface.
- `crates/macros/src/typed_sql/public_schema.rs:18-33` - end-to-end macro compilation path from inline schema and SQL to lowering.
- `crates/macros/src/typed_sql/public_input.rs:11-53` - token-style/public SQL ingestion and span mapping.
- `crates/macros/src/typed_sql/source.rs:270-439` - named-placeholder canonicalization and source-map construction.
- `crates/macros/src/typed_sql/parse_backend.rs:9-38` - backend SQL parser boundary for the current `SELECT` subset.
- `crates/macros/src/typed_sql/normalize.rs:22-107` - normalization of parsed SQL into typed-query IR.
- `crates/macros/src/typed_sql/ir.rs:45-210` - current IR node set for normalized typed SQL.
- `crates/macros/src/typed_sql/resolver.rs:340-505` - schema resolution, placeholder inference, and final checked query assembly.
- `crates/macros/src/typed_sql/lower.rs:131-223` - lowering from checked query metadata to emitted runtime `Query` tokens.
- `crates/core/examples/axum_service.rs:92-152` - public example usage of `typed_query!` in application code.
- `crates/core/tests/sql_macro.rs:155-320` - integration coverage showing parity with `Query::raw` and execution against Postgres.
- `crates/core/tests/typed_query_ui.rs:1-9` - UI harness for `typed_query!` diagnostics.
- `README.md:255-314` - top-level user-facing documentation for `typed_query!`.
- `docs/book/02-selecting.md:53-77` and `docs/book/03-parameterized-commands.md:109-139` - book chapters covering typed-query scope and usage.

## Architecture Documentation

The current typed query implementation is organized as a linear compile-time pipeline: public proc-macro parsing (`typed_query` / `TypedQueryInput`) → inline schema catalog construction → token-style SQL ingestion and span mapping → canonical placeholder rewriting into positional SQL → backend parsing through `sqlparser` → normalization into `ParsedSelect` / `ParsedExpr` IR → schema resolution and placeholder/type inference into `CheckedSelect` → runtime lowering into `Fragment`/`Query` tokens (`crates/macros/src/lib.rs:188-206`, `crates/macros/src/typed_sql/public_schema.rs:18-33`, `crates/macros/src/typed_sql/mod.rs:24-33`, `crates/macros/src/typed_sql/resolver.rs:340-505`, `crates/macros/src/typed_sql/lower.rs:29-48`).

Within that flow, the public/frontend boundary is concentrated in `public_input.rs` and `source.rs`, the structural SQL subset is modeled by `normalize.rs` + `ir.rs`, and semantic typing/nullability/parameter inference is centralized in `resolver.rs`; lowering then turns the checked metadata into runtime codec tuples and canonical SQL text without introducing a separate runtime AST layer (`crates/macros/src/typed_sql/public_input.rs:11-53`, `crates/macros/src/typed_sql/source.rs:270-439`, `crates/macros/src/typed_sql/normalize.rs:22-107`, `crates/macros/src/typed_sql/ir.rs:45-210`, `crates/macros/src/typed_sql/resolver.rs:340-505`, `crates/macros/src/typed_sql/lower.rs:131-230`).

## Open Questions

None observed from the existing implementation map that require user input for the research artifact itself.
