---
date: 2026-04-29T11:10:00-04:00
git_commit: 41e2ae049ff888b4a92928e3f51f8f80d6ed609f
branch: feature/external-schemas
repository: notthatbreezy/babar
topic: "External schemas"
tags: [research, typed-query, schema, docs]
status: complete
last_updated: 2026-04-29
---

# Research: External Schemas

## Research Question

Document the current `typed_query!` schema pipeline, the reusable Rust-facing schema structures already present in the repository, and the documentation and verification infrastructure relevant to planning Rust-visible external schema support.

## Summary

Today, `typed_query!` is a proc macro exposed from `babar-macros` and re-exported by the core crate. Its public input shape is still `schema = { ... }, <SQL>`, where an inline table/column DSL is parsed directly by the macro, turned into an internal `SchemaCatalog`, combined with token-style SQL parsing, and then passed through canonicalization, backend parsing, normalization, resolution, and lowering before emitting an ordinary `Query<P, R>` (`crates/macros/src/lib.rs:188-206`, `crates/macros/src/typed_sql/public_schema.rs:18-49`, `crates/macros/src/typed_sql/mod.rs:18-33`, `crates/macros/src/typed_sql/lower.rs:18-93`).

The repository already contains a separate Rust-facing schema layer in `crates/core/src/schema.rs` built around const-friendly types such as `SqlType`, `Nullability`, `TableRef<T>`, `Column<T>`, `Binding<T>`, and `QualifiedColumn<T>`, while the current typed-query macro still only consumes the inline DSL rather than those Rust-visible declarations (`crates/core/src/lib.rs:213`, `crates/core/src/schema.rs:13-19`, `crates/core/src/schema.rs:108-189`, `crates/core/src/schema.rs:225-285`, `crates/core/src/schema.rs:325-385`, `crates/core/src/schema.rs:421-479`, `crates/core/src/schema.rs:515-605`, `crates/macros/src/typed_sql/public_schema.rs:64-137`, `crates/macros/src/typed_sql/public_schema.rs:155-269`).

Documentation and CI are centered on mdBook plus root-level README guidance. The repo’s documented and automated verification surface includes `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, docs builds with `mdbook build`, and additional release/hygiene checks such as `cargo deny`, `cargo audit`, `cargo msrv verify`, `cargo semver-checks`, and `cargo publish --dry-run` (`book.toml:1-10`, `docs/SUMMARY.md:1-39`, `README.md:166-214`, `.github/workflows/ci.yml:10-65`, `.github/workflows/pages.yml:29-44`).

## Documentation System

- **Framework**: mdBook (`book.toml:1-10`).
- **Docs Directory**: `docs/` (`book.toml:1-5`, `docs/index.md:12-75`).
- **Navigation Config**: `docs/SUMMARY.md` (`docs/SUMMARY.md:1-39`).
- **Style Conventions**: documentation is organized into landing/getting-started/book/reference/explanation/tutorial sections, with short prose chapters and fenced code examples in the README and book/tutorial pages (`docs/SUMMARY.md:1-39`, `docs/index.md:12-75`, `docs/getting-started/first-query.md:3-92`, `docs/book/02-selecting.md:53-78`, `docs/book/10-custom-codecs.md:95-118`).
- **Build Command**: `mdbook build` (`README.md:196-198`, `.github/workflows/pages.yml:36-44`).
- **Standard Files**: `README.md`, `CHANGELOG.md`, `docs/SUMMARY.md` (`README.md:1-5`, `CHANGELOG.md:1`, `docs/SUMMARY.md:1-39`).

## Verification Commands

- **Test Command**: `cargo test --all-features` (`README.md:184-187`, `.github/workflows/ci.yml:10-23`).
- **Lint Command**: `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings` (`README.md:174-179`, `.github/workflows/ci.yml:24-35`).
- **Build Command**: `cargo build --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, and `mdbook build` (`README.md:181-198`, `.github/workflows/ci.yml:33-35`, `.github/workflows/pages.yml:36-44`).
- **Type Check**: `cargo check --all-features`; CI hygiene also runs `cargo msrv verify ... cargo check` (`README.md:205-214`, `.github/workflows/ci.yml:47-53`).

## Detailed Findings

### Public macro surface and entry path

- The `typed_query` proc macro is defined in `babar-macros` and documented as an inline-schema, typed-SQL-v1 surface that infers parameter and row codecs (`crates/macros/src/lib.rs:188-206`).
- The core crate re-exports the macro alongside `sql!`, `query!`, `command!`, and `Codec`, so end users call it as `babar::typed_query!(...)` (`crates/core/src/lib.rs:262-263`).
- Core crate docs describe the current public model as an inline schema DSL that avoids evaluating user-defined Rust schema constants at expansion time (`crates/core/src/lib.rs:143-146`).

### Current inline schema input model

- `TypedQueryInput` parses `schema = { ... }, SQL` and requires the schema section before the query text (`crates/macros/src/typed_sql/public_schema.rs:18-49`).
- The inline schema DSL supports repeated `table [schema.]name { column: type, ... }` blocks, with duplicate table/column detection during parsing (`crates/macros/src/typed_sql/public_schema.rs:64-137`, `crates/macros/src/typed_sql/public_schema.rs:155-240`).
- Column type parsing accepts a base SQL type or `nullable(inner)`, and the accepted names include `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`, `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric` (`crates/macros/src/typed_sql/public_schema.rs:243-269`).
- The inline DSL is converted into internal `SchemaCatalog`, `SchemaTable`, and `SchemaColumn` structures used by the resolver (`crates/macros/src/typed_sql/public_schema.rs:88-137`, `crates/macros/src/typed_sql/resolver.rs:111-249`).

### Typed SQL pipeline and schema interaction

- Public SQL input is parsed into `PublicSqlInput`, which accepts either a single SQL string literal or token-style SQL and records span mappings for diagnostics (`crates/macros/src/typed_sql/public_input.rs:11-53`).
- Named placeholders are assembled from `$` + identifier tokens in the public input stage, then canonicalized into positional slots while recording placeholder occurrences and optional-group metadata (`crates/macros/src/typed_sql/public_input.rs:129-160`, `crates/macros/src/typed_sql/source.rs:295-530`).
- Backend SQL parsing is restricted to a single top-level `SELECT` path through `parse_select_source` and the typed SQL module entrypoints (`crates/macros/src/typed_sql/mod.rs:18-33`, `crates/macros/src/typed_sql/parse_backend.rs:9-63`).
- Normalization maps the backend AST into the typed-query IR and constrains the supported query subset for projections, predicates, ordering, limit, and offset (`crates/macros/src/typed_sql/normalize.rs:24-145`, `crates/macros/src/typed_sql/normalize.rs:152-257`, `crates/macros/src/typed_sql/normalize.rs:260-379`, `crates/macros/src/typed_sql/normalize.rs:381-633`, `crates/macros/src/typed_sql/ir.rs:45-210`).
- Resolution binds table and column references against the schema catalog, tracks scope/bindings, and infers placeholder types and nullability before producing checked query metadata (`crates/macros/src/typed_sql/resolver.rs:346-511`, `crates/macros/src/typed_sql/resolver.rs:721-979`, `crates/macros/src/typed_sql/resolver.rs:1106-1474`).
- Lowering maps checked types into runtime codec selections and emits the final `Query` tokens (`crates/macros/src/typed_sql/lower.rs:18-93`, `crates/macros/src/typed_sql/lower.rs:418-478`, `crates/macros/src/typed_sql/lower.rs:1042-1081`).

### Existing Rust-visible schema declarations in core

- The core crate exposes `pub mod schema`, which contains const-friendly schema building blocks rather than the inline macro DSL (`crates/core/src/lib.rs:213`, `crates/core/src/schema.rs:1-6`).
- `SqlType` and `Nullability` model column shape at the type/value level (`crates/core/src/schema.rs:13-19`, `crates/core/src/schema.rs:21-106`).
- `TableRef<T>` and `Column<T>` represent tables and columns, while `Binding<T>` and `QualifiedColumn<T>` model bound and qualified references (`crates/core/src/schema.rs:108-189`, `crates/core/src/schema.rs:225-285`, `crates/core/src/schema.rs:325-385`, `crates/core/src/schema.rs:421-479`, `crates/core/src/schema.rs:515-605`).
- Repository tests use marker enums plus `pub const` table/column declarations as handwritten/generated-like fixtures within this schema layer (`crates/core/src/schema.rs:607-823`).

### Existing Rust-facing user API patterns

- The crate already teaches derive-driven user APIs through `#[derive(Codec)]`, field-level `#[pg(codec = \"...\")]`, and generated associated constants such as `Struct::CODEC` (`crates/macros/src/lib.rs:377-404`, `crates/macros/src/lib.rs:483-698`, `docs/book/10-custom-codecs.md:95-118`, `crates/core/examples/derive_codec.rs:13-22`, `crates/core/examples/derive_codec.rs:75-104`).
- Runtime/user-facing APIs also use thin wrapper types and marker-like generic forms for richer semantics, including `Range<T>`, `Multirange<T>`, `Vector`, `TsVector`, `TsQuery`, `Hstore`, `Geometry<T>`, `Geography<T>`, and `Srid` (`crates/core/src/codec/range.rs:15-63`, `crates/core/src/codec/multirange.rs:12-64`, `crates/core/src/codec/pgvector.rs:14-69`, `crates/core/src/codec/text_search.rs:12-58`, `crates/core/src/codec/hstore.rs:14-65`, `crates/core/src/codec/postgis.rs:3-9`, `crates/core/src/codec/postgis.rs:57-113`, `crates/core/src/codec/postgis.rs:142-210`).
- The codec module also exposes lowercase value-style codec constants and combinators such as `nullable(...)`, which mirrors the style used in current inline typed-query schemas (`crates/core/src/codec/mod.rs:12-24`, `crates/core/src/codec/mod.rs:74-130`, `crates/macros/src/typed_sql/public_schema.rs:243-269`).

### Tests, examples, and documentation relevant to external schemas

- UI coverage for `typed_query!` lives in `crates/core/tests/typed_query_ui.rs` with pass/fail fixtures for valid schemas, unknown columns, unsupported types, and optional-syntax misuse (`crates/core/tests/typed_query_ui.rs:3-12`, `crates/core/tests/ui/typed_query/pass/basic.rs:4-13`, `crates/core/tests/ui/typed_query/fail/unknown_column.rs:4-12`, `crates/core/tests/ui/typed_query/fail/unsupported_type.rs:4-12`, `crates/core/tests/ui/typed_query/fail/invalid_optional_projection.rs:4-12`, `crates/core/tests/ui/typed_query/fail/ambiguous_optional_ownership.rs:4-11`, `crates/core/tests/ui/typed_query/fail/invalid_optional_limit_group.rs:4-11`).
- Runtime behavior for typed queries is exercised in the SQL macro integration tests, including parity with `Query::raw` and execution against Postgres (`crates/core/tests/sql_macro.rs:164-186`, `crates/core/tests/sql_macro.rs:188-230`, `crates/core/tests/sql_macro.rs:367-383`, `crates/core/tests/sql_macro.rs:417-463`).
- The Axum example demonstrates typed queries in user-facing service code, including reusable helper-based query construction for reads (`crates/core/examples/axum_service.rs:1-6`, `crates/core/examples/axum_service.rs:119-133`, `crates/core/examples/axum_service.rs:157-173`).
- README and book chapters teach `typed_query!` as a query-only, SQL-first feature and document the current inline schema model and service-style read usage (`README.md:255-320`, `docs/book/02-selecting.md:53-78`, `docs/book/03-parameterized-commands.md:109-149`, `docs/getting-started/first-query.md:3-92`).

### Observed constraints present in the current codebase

- The current typed-query public contract requires the inline `schema = { ... }` block and does not consume Rust-visible schema declarations directly (`crates/macros/src/typed_sql/public_schema.rs:40-49`, `crates/core/src/lib.rs:143-146`).
- The public docs and examples consistently describe `typed_query!` as query-only and `SELECT`-subset-focused (`README.md:301-314`, `docs/book/03-parameterized-commands.md:131-139`, `crates/core/examples/axum_service.rs:3-5`).
- The schema parser recognizes more SQL type names than the current lowering path emits runtime codecs for (`crates/macros/src/typed_sql/public_schema.rs:243-269`, `crates/macros/src/typed_sql/lower.rs:1042-1081`).
- Optional typed-query SQL can become runtime-dynamic, and `Session::prepare` rejects dynamic fragments (`crates/macros/src/typed_sql/lower.rs:46-94`, `crates/core/src/session/mod.rs:267-280`).

## Code References

- `crates/macros/src/lib.rs:188-206` - public `typed_query` proc macro entrypoint.
- `crates/macros/src/typed_sql/public_schema.rs:18-49` - typed query input parsing entry.
- `crates/macros/src/typed_sql/public_schema.rs:64-269` - inline schema DSL parsing and SQL type mapping.
- `crates/macros/src/typed_sql/public_input.rs:11-53` - public SQL token/string ingestion.
- `crates/macros/src/typed_sql/source.rs:295-530` - placeholder and optional-group canonicalization.
- `crates/macros/src/typed_sql/parse_backend.rs:9-63` - backend parser boundary.
- `crates/macros/src/typed_sql/normalize.rs:24-633` - normalization for the supported typed-query subset.
- `crates/macros/src/typed_sql/resolver.rs:111-249` - internal schema catalog structures.
- `crates/macros/src/typed_sql/resolver.rs:346-1474` - resolution and type inference.
- `crates/macros/src/typed_sql/lower.rs:18-93` - lowering entry and runtime dynamic handling.
- `crates/macros/src/typed_sql/lower.rs:1042-1081` - runtime codec mapping.
- `crates/core/src/schema.rs:13-605` - Rust-visible schema primitives.
- `crates/core/src/schema.rs:607-823` - schema fixture patterns using marker enums and const declarations.
- `README.md:166-214` - documented verification commands.
- `.github/workflows/ci.yml:10-65` - CI verification matrix.
- `book.toml:1-10` and `docs/SUMMARY.md:1-39` - mdBook configuration and navigation.

## Architecture Documentation

The current typed-query path is a linear compile-time pipeline: proc-macro entry (`typed_query`) → public input parse (`TypedQueryInput` plus `PublicSqlInput`) → inline schema DSL to internal catalog → placeholder/optional canonicalization → backend SQL parse → normalization into typed-query IR → schema resolution and placeholder inference → lowering into `Fragment`/`Query` tokens (`crates/macros/src/lib.rs:188-206`, `crates/macros/src/typed_sql/public_schema.rs:18-49`, `crates/macros/src/typed_sql/public_input.rs:11-53`, `crates/macros/src/typed_sql/source.rs:295-530`, `crates/macros/src/typed_sql/mod.rs:18-33`, `crates/macros/src/typed_sql/resolver.rs:346-511`, `crates/macros/src/typed_sql/lower.rs:18-93`).

In parallel to that macro-only inline schema path, the core crate already contains a Rust-visible schema representation based on const-friendly table/column/type symbols. This is a distinct layer in today’s codebase rather than the current `typed_query!` input path (`crates/core/src/lib.rs:213`, `crates/core/src/schema.rs:13-605`, `crates/macros/src/typed_sql/public_schema.rs:40-49`).

## Open Questions

- None from the current code-mapping pass.
