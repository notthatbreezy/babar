---
date: 2026-04-29T22:09:10-04:00
git_commit: 733df3c288ed4a02747dd506daacbafe1bc2d24b
branch: feature/typed-sql-unification
repository: notthatbreezy/babar
topic: "Typed SQL unification code research"
tags: [research, typed-sql, macros, schema, verification, execution]
status: complete
last_updated: 2026-04-30
---

# Research: Typed SQL unification

## Research Question

How the current typed SQL stack is wired end-to-end for unification work: the
`typed_query!` pipeline and entrypoints, how `schema!` integrates with it, what
backs `query!` / `command!` / `sql!` today, how live verification and runtime
type lowering currently work, which execution surfaces must remain as escape
hatches, and which seams are most directly involved in moving `query!` /
`command!` toward schema-aware typed SQL.

## Summary

`typed_query!` is a separate proc-macro path from `query!` / `command!` / `sql!`.
Its public entrypoint in `crates/macros/src/lib.rs` delegates to
`typed_sql::expand_typed_query`, which parses public SQL tokens, canonicalizes
named placeholders and optional groups, parses a SELECT-only SQL subset,
resolves tables/columns/placeholders against an inline or authored schema
catalog, lowers inferred parameter/row types into runtime codecs, and finally
emits a `babar::query::Query` backed by `Fragment::__from_parts` or
`Fragment::__from_dynamic_parts`
(`crates/macros/src/lib.rs:190-229`,
`crates/macros/src/typed_sql/public_schema.rs:20-40`,
`crates/macros/src/typed_sql/public_input.rs:12-53`,
`crates/macros/src/typed_sql/mod.rs:19-33`,
`crates/macros/src/typed_sql/parse_backend.rs:9-37`,
`crates/macros/src/typed_sql/normalize.rs:24-116`,
`crates/macros/src/typed_sql/resolver.rs:346-460`,
`crates/macros/src/typed_sql/lower.rs:46-94`,
`crates/macros/src/typed_sql/lower.rs:418-478`).

By contrast, `query!` and `command!` are still explicit-codec frontends: they
parse `params = ...` and `row = ...`, optionally verify declared shapes against
a live probe, then emit `Query::from_fragment` / `Command::from_fragment` over
`Fragment::__from_parts`; `sql!` similarly rewrites named bindings into a
`Fragment` and can only verify parameter metadata
(`crates/macros/src/lib.rs:119-188`,
`crates/macros/src/lib.rs:232-398`,
`crates/macros/src/verify.rs:21-52`,
`crates/macros/src/verify.rs:124-152`,
`crates/macros/src/verify.rs:155-295`).

The closest existing reuse boundary is therefore the shared runtime statement
layer: both macro families bottom out in `Fragment`, `Query`, and `Command`
(`crates/core/src/query/fragment.rs:142-375`,
`crates/core/src/query/mod.rs:27-173`). The schema-aware path already contains
reusable schema ingestion (`SchemaCatalog`), SQL token handling
(`PublicSqlInput` / `SqlSource`), semantic checking (`resolve_select`), and
lowering-to-`Fragment` logic; the current SELECT-only parse/IR/lowering path and
`LoweredQuery::emit_query_tokens` remain the main typed-query-specific boundary
(`crates/macros/src/typed_sql/source.rs:295-530`,
`crates/macros/src/typed_sql/resolver.rs:112-183`,
`crates/macros/src/typed_sql/lower.rs:418-478`).

The other two major current boundaries are verification and execution. Live
verification today belongs entirely to the explicit-codec macro family through
`verify.rs`, while the schema-aware typed SQL path does not yet call into that
probe machinery (`crates/macros/src/lib.rs:232-299`,
`crates/macros/src/verify.rs:21-52`,
`crates/macros/src/verify.rs:281-295`). Separately, migrations, health checks,
and certain runtime metadata flows still depend on `simple_query_raw`, so typed
SQL unification cannot be treated as “remove all raw SQL execution”
(`crates/core/src/migration/runner.rs:32-75,149-179,216-315`,
`crates/core/src/pool.rs:456-465,490-496`,
`crates/core/src/session/mod.rs:443-510`).

## Documentation System

- **Framework**: mdBook (`book.toml:1-10`)
- **Docs Directory**: `docs/` (`book.toml:1-5`, `docs/:1-7`)
- **Navigation Config**: `docs/SUMMARY.md` via mdBook (`book.toml:1-5`, `docs/:1-7`)
- **Style Conventions**: prose docs live under `docs/` and top-level product guidance stays in `README.md` / `CHANGELOG.md` (`README.md:1-215`)
- **Build Command**: `mdbook build` (`README.md:196-198`)
- **Standard Files**: `README.md`, `CHANGELOG.md` at repo root (`README.md:1-215`)

## Verification Commands

- **Test Command**: `cargo +"$MSRV" test --all-features` and `cargo +stable test --all-features` (`README.md:184-187`)
- **Lint Command**: `cargo fmt --check`; `cargo +stable clippy --all-targets --all-features -- -D warnings`; `cargo +"$MSRV" clippy --all-targets --all-features -- -D warnings` (`README.md:174-180`)
- **Build Command**: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `mdbook build` (`README.md:181-198`)
- **Type Check**: `cargo check --all-features` is the documented fast iteration loop (`README.md:205-209`)

## Detailed Findings

### Public macro entrypoints

- `babar` re-exports `command`, `query`, `schema`, `sql`, and `typed_query` from `babar_macros` (`crates/core/src/lib.rs:317-318`).
- In the proc-macro crate, `sql!` calls `compile_input` and emits a `Fragment::__from_parts(...)`; `query!` calls `compile_query_input`; `command!` calls `compile_command_input`; `typed_query!` delegates to `typed_sql::expand_typed_query`; `schema!` delegates to `schema_decl::expand_schema` (`crates/macros/src/lib.rs:127-229`).

### Current `sql!` / `query!` / `command!` path

- `sql!` parses a string literal plus named bindings, rewrites `$name` placeholders into numbered slots, flattens nested `sql!(...)` fragments, rejects missing/unused/duplicate bindings, and returns a `Fragment` carrying origin metadata (`crates/macros/src/lib.rs:30-44`, `crates/macros/src/lib.rs:56-78`, `crates/macros/src/lib.rs:119-154`, `crates/macros/src/lib.rs:435-504`).
- `query!` and `command!` parse explicit codec DSL inputs (`params = ...`, `row = ...`), optionally verify them against a live database, then emit `Query::from_fragment` or `Command::from_fragment` over `Fragment::__from_parts` (`crates/macros/src/lib.rs:45-55`, `crates/macros/src/lib.rs:81-117`, `crates/macros/src/lib.rs:249-299`).
- The verification subsystem discovers `BABAR_DATABASE_URL` first, then `DATABASE_URL`, prepares SQL against a live Postgres server, and compares actual parameter/row metadata to the declared codec DSL (`crates/macros/src/verify.rs:11-52`, `crates/macros/src/verify.rs:124-152`, `crates/macros/src/verify.rs:245-295`).

### Current `typed_query!` pipeline

- `typed_query!` accepts either `schema = { ... }` or the internal authored bridge `__babar_schema = { ... }`, builds a `SchemaCatalog`, parses SQL via `PublicSqlInput`, resolves the checked statement, lowers it, and emits tokens from `LoweredQuery` (`crates/macros/src/typed_sql/public_schema.rs:20-40`, `crates/macros/src/typed_sql/public_schema.rs:43-77`, `crates/macros/src/typed_sql/public_schema.rs:152-225`).
- `PublicSqlInput` accepts either a string literal or tokenized SQL, canonicalizes it, preserves span mappings for diagnostics, and passes the result into `parse_select_source` (`crates/macros/src/typed_sql/public_input.rs:12-53`).
- Canonicalization rewrites named placeholders like `$id` into numbered slots, records placeholder occurrences and optional markers, and tracks optional parenthesized groups `(...)?` for later runtime rendering (`crates/macros/src/typed_sql/source.rs:13-20`, `crates/macros/src/typed_sql/source.rs:197-249`, `crates/macros/src/typed_sql/source.rs:295-530`).
- The backend parser is SELECT-only today: `parse_select` requires exactly one statement, requires `Statement::Query`, and rejects non-SELECT or set-operation bodies; `ParsedSql` currently stores a `ParsedSelect` (`crates/macros/src/typed_sql/mod.rs:19-33`, `crates/macros/src/typed_sql/parse_backend.rs:9-37`).
- Normalization is also SELECT-specific and rejects broader query features, keeping the supported subset to SELECT/FROM/JOIN/WHERE/ORDER BY/LIMIT/OFFSET on plain table references (`crates/macros/src/typed_sql/normalize.rs:24-116`, `crates/macros/src/typed_sql/normalize.rs:118-199`, `crates/macros/src/typed_sql/normalize.rs:218-258`).
- Resolution builds semantic bindings, infers placeholder SQL types and slots, enforces projection/filter/order/limit rules, and returns `CheckedSelect` with ordered `CheckedParameter` metadata (`crates/macros/src/typed_sql/resolver.rs:112-183`, `crates/macros/src/typed_sql/resolver.rs:252-300`, `crates/macros/src/typed_sql/resolver.rs:346-460`, `crates/macros/src/typed_sql/resolver.rs:1409-1428`).
- Lowering converts checked parameters and projections into runtime codec selections, renders canonical SQL, and emits either static `Fragment::__from_parts` or dynamic `Fragment::__from_dynamic_parts` when optional placeholders/groups are present (`crates/macros/src/typed_sql/lower.rs:18-94`, `crates/macros/src/typed_sql/lower.rs:418-520`).
- Runtime lowering currently only maps `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`, `int8`, `float4`, and `float8`; wider authored schema types produce a lowering diagnostic when used in executable typed SQL (`crates/macros/src/typed_sql/lower.rs:1042-1081`).

### Current verification path and type/runtime gaps

- Live verification discovers `BABAR_DATABASE_URL` first, then `DATABASE_URL`,
  and uses `postgres::Client::prepare` as the probe mechanism
  (`crates/macros/src/verify.rs:21-52`,
  `crates/macros/src/verify.rs:128-152`).
- `sql!` verifies only when every binding codec is inside the narrow verifiable
  subset; otherwise verification is skipped and normal expansion continues
  (`crates/macros/src/lib.rs:232-247,301-307`,
  `crates/core/tests/sql_macro_ui.rs:73-85`,
  `crates/core/tests/ui/sql/pass/verify_skip_unsupported.rs:1-6`).
- `query!` / `command!` always parse explicit codec DSL and call probe
  verification when configured, but that DSL itself only supports
  `int2` / `int4` / `int8` / `bool` / `text` / `varchar` / `bytea`,
  `nullable(...)`, and tuples
  (`crates/macros/src/lib.rs:249-299`,
  `crates/macros/src/verify.rs:172-243`,
  `crates/core/tests/ui/typed_macro/fail/unsupported_codec.stderr:1-5`).
- Verification failures already surface concrete config/shape mismatches through
  UI diagnostics, which is relevant prior art for moving verification into the
  schema-aware typed SQL path
  (`crates/core/tests/ui/sql/fail/verify_invalid_config.stderr:1-5`,
  `crates/core/tests/ui/sql/fail/verify_live_mismatch.stderr:1-5`,
  `crates/core/tests/ui/typed_macro/fail/verify_live_param_mismatch.stderr:1-5`,
  `crates/core/tests/ui/typed_macro/fail/verify_live_row_mismatch.stderr:1-5`).
- The typed SQL authored schema surface already accepts `uuid`, `date`, `time`,
  `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric`
  (`crates/macros/src/typed_sql/public_schema.rs:331-357`,
  `crates/macros/src/lib.rs:193-205`,
  `crates/macros/src/schema_decl.rs:331-356`), and `resolver::SqlType` already
  models them (`crates/macros/src/typed_sql/resolver.rs:20-39`), but runtime
  lowering still maps only `bool`, `bytea`, `varchar`, `text`, `int2`, `int4`,
  `int8`, `float4`, and `float8`
  (`crates/macros/src/typed_sql/lower.rs:1042-1068`).
- The mismatch is therefore specifically in typed SQL lowering, not in the
  broader runtime, which already has wider type metadata and codec support
  (`crates/core/src/types.rs:96-111,248-255`,
  `crates/core/src/codec/mod.rs:38-72,79-130`).
- Authored-schema diagnostics already explicitly explain this mismatch to users
  when a declared type cannot yet be lowered into executable typed SQL
  (`crates/macros/src/typed_sql/public_schema.rs:80-99`,
  `crates/core/tests/ui/typed_query/fail/authored_unsupported_declared_type.stderr:1-8`).

### Execution surfaces, escape hatches, and operational constraints

- `Query::raw` and `Command::raw` are the generic explicit-codec builders and
  remain the main raw extended-protocol escape hatches; unlike macro-built
  fragments, they do not carry macro callsite origin metadata
  (`crates/core/src/query/mod.rs:32-49,123-131`).
- `simple_query_raw` is the raw simple-protocol surface returning `Vec<RawRows>`
  and multi-result sets
  (`crates/core/src/session/mod.rs:60-80`,
  `crates/core/src/session/driver.rs:49-59`).
- The migration runner depends on `simple_query_raw` for state-table creation,
  transactional script execution, explicit `BEGIN` / `COMMIT` / `ROLLBACK`, and
  advisory lock management
  (`crates/core/src/migration/runner.rs:32-75,149-179,216-315`).
- Pool control-plane behavior also relies on `simple_query_raw` for health-check
  ping/reset operations (`crates/core/src/pool.rs:456-465,490-496`), and
  session type-OID resolution uses it as well
  (`crates/core/src/session/mod.rs:443-510`).
- `typed_query!` is still query-only today: it lowers only to `Query`, the
  parser requires a single top-level `SELECT`, and schema modules synthesize
  only a `typed_query!` bridge
  (`crates/macros/src/lib.rs:190-208`,
  `crates/macros/src/typed_sql/parse_backend.rs:18-37`,
  `crates/macros/src/schema_decl.rs:103-117`).
- Docs and examples currently reflect that write gap explicitly: the Axum
  example keeps writes on `Command::raw`, and the parameterized-command docs
  still teach `command!` / `Command::raw` rather than schema-aware typed writes
  (`crates/core/examples/axum_service.rs:3-5,97-105,139-149`,
  `docs/book/03-parameterized-commands.md:109-169`,
  `README.md:304-347`).
- Optional typed SQL lowers to runtime-dependent SQL via dynamic fragments, and
  those statements cannot currently be prepared as named server-side statements
  (`crates/macros/src/typed_sql/lower.rs:46-94`,
  `crates/core/src/query/fragment.rs:149-150,288-304,354-374`,
  `crates/core/src/session/mod.rs:267-280,341-353`).
- Relevant current behavior coverage already exists for optional SQL rendering,
  prepared statements, and streaming
  (`crates/core/tests/sql_macro.rs:379-425`,
  `crates/core/tests/prepared.rs:39-257`,
  `crates/core/tests/streaming.rs:37-123`).

### How `schema!` integrates with `typed_query!`

- `schema!` parses authored modules containing one or more `table` declarations, expands nested table modules/constants, exports `TABLES` and `SCHEMA`, and synthesizes a local `typed_query!` wrapper as a `macro_rules!` bridge (`crates/macros/src/schema_decl.rs:22-123`).
- That wrapper expands to `::babar::typed_query!(__babar_schema = { ... }, ...)`, with `expand_typed_query_bridge` translating authored table/column declarations into the inline schema token form expected by the typed SQL pipeline (`crates/macros/src/schema_decl.rs:103-117`, `crates/macros/src/schema_decl.rs:233-250`, `crates/macros/src/schema_decl.rs:330-336`).
- Authored schema declarations also materialize runtime-visible schema metadata (`TableDef`, `SchemaDef`, `TableRef`, `ColumnDef`, `Column`) in `babar::schema` (`crates/macros/src/schema_decl.rs:216-229`, `crates/core/src/schema.rs:162-291`, `crates/core/src/schema.rs:293-420`).
- Cross-crate tests show the authored wrapper and inline `typed_query!` path produce the same SQL and OID metadata (`crates/core/tests/external_schema_export.rs:7-25`, `crates/core/tests/external_schema_export/src/lib.rs:5-13`, `crates/core/tests/sql_macro.rs:243-317`).

### Relevant reuse / replacement seams for typed-SQL unification

- **Shared runtime seam**: every macro family already converges on `Fragment`, then `Query` or `Command`; this is the common execution/runtime boundary (`crates/core/src/query/fragment.rs:269-375`, `crates/core/src/query/mod.rs:32-173`).
- **Schema ingestion seam**: `schema!` already has a bridge that turns authored schema declarations into the inline schema tokens consumed by the typed SQL compiler (`crates/macros/src/schema_decl.rs:103-117`, `crates/macros/src/schema_decl.rs:233-250`).
- **Typed SQL front-door seam**: `compile_typed_query` is the single orchestration point for schema catalog creation, SQL parsing, resolution, lowering, and token emission (`crates/macros/src/typed_sql/public_schema.rs:28-40`).
- **Semantic-analysis seam**: `SchemaCatalog`, `resolve_select`, and placeholder inference concentrate table/column lookup, ambiguity handling, inferred parameter slots, and type/nullability reasoning (`crates/macros/src/typed_sql/resolver.rs:112-183`, `crates/macros/src/typed_sql/resolver.rs:346-460`, `crates/macros/src/typed_sql/resolver.rs:1409-1428`).
- **Lowering seam**: `lower_select` and `LoweredQuery::emit_query_tokens` are where inferred typed SQL becomes runtime codecs plus `Fragment` construction; this is also where optional runtime SQL shapes attach via `__from_dynamic_parts` (`crates/macros/src/typed_sql/lower.rs:46-94`, `crates/macros/src/typed_sql/lower.rs:418-520`).
- **Current verification seam**: live verification exists today only in the explicit-codec path through `verify.rs`; `typed_query!` does not call that probe path (`crates/macros/src/lib.rs:232-299`, `crates/macros/src/verify.rs:21-52`, `crates/macros/src/verify.rs:281-295`).
- **Observed current boundary**: the typed SQL compiler is structured around `ParsedSelect` / `CheckedSelect` / `LoweredQuery` and a SELECT-only parser, so the present typed path is query-shaped rather than command-shaped (`crates/macros/src/typed_sql/mod.rs:19-33`, `crates/macros/src/typed_sql/parse_backend.rs:18-37`, `crates/macros/src/typed_sql/lower.rs:418-478`).
- **Escape-hatch seam**: `Query::raw`, `Command::raw`, and `simple_query_raw`
  already divide unsupported SQL into extended-query and simple-query lanes;
  this separation matters because migrations and control-plane behavior are
  already coupled to the simple-query lane
  (`crates/core/src/query/mod.rs:32-49,123-131`,
  `crates/core/src/session/mod.rs:60-80`,
  `crates/core/src/migration/runner.rs:32-75`).

## Code References

- `crates/macros/src/lib.rs:127-229` - Public proc-macro entrypoints for `sql!`, `query!`, `command!`, `typed_query!`, and `schema!`
- `crates/macros/src/lib.rs:232-398` - Explicit-codec compile/verification path for `sql!`, `query!`, and `command!`
- `crates/macros/src/schema_decl.rs:22-123` - `schema!` module generation and local `typed_query!` wrapper
- `crates/macros/src/schema_decl.rs:233-336` - Authored-schema bridge into inline typed-query schema tokens
- `crates/macros/src/typed_sql/public_schema.rs:20-40` - `typed_query!` orchestration entrypoint
- `crates/macros/src/typed_sql/public_input.rs:12-53` - Public SQL token/literal parsing with diagnostic span mapping
- `crates/macros/src/typed_sql/source.rs:295-530` - Canonicalization of placeholders and optional groups
- `crates/macros/src/typed_sql/parse_backend.rs:9-37` - SELECT-only backend parsing gate
- `crates/macros/src/typed_sql/normalize.rs:24-116` - SQL subset normalization into typed_sql IR
- `crates/macros/src/typed_sql/resolver.rs:346-460` - Table/column resolution and semantic analysis flow
- `crates/macros/src/typed_sql/lower.rs:46-94` - Emission of static vs dynamic `Query` tokens
- `crates/macros/src/typed_sql/lower.rs:418-478` - Lowering from checked select to runtime statement form
- `crates/macros/src/typed_sql/lower.rs:1042-1081` - Current runtime codec mapping limits for typed SQL
- `crates/macros/src/verify.rs:21-52` - Live verification config discovery
- `crates/macros/src/verify.rs:124-152` - Probe connection/prepare path
- `crates/macros/src/verify.rs:172-243` - Explicit codec DSL parsing for verifiable query/command macros
- `crates/core/src/query/fragment.rs:142-375` - Shared runtime fragment construction and dynamic SQL support
- `crates/core/src/query/mod.rs:27-173` - `Query` / `Command` wrappers over `Fragment`
- `crates/core/src/session/mod.rs:60-80` - `simple_query_raw` execution path
- `crates/core/src/session/mod.rs:267-280` - Prepare rejection for runtime-dynamic typed SQL
- `crates/core/src/migration/runner.rs:32-75` - Migration dependency on `simple_query_raw`
- `crates/core/src/pool.rs:456-465` - Pool control-plane dependency on `simple_query_raw`
- `crates/core/src/types.rs:96-111,248-255` - Broader runtime type metadata already exists outside typed SQL lowering
- `crates/core/src/codec/mod.rs:38-72,79-130` - Broader runtime codec modules available to the core runtime

## Architecture Documentation

The repository currently has two distinct macro architectures for typed SQL. The
older `sql!` / `query!` / `command!` family is codec-declaration-first and
probe-backed; the newer `typed_query!` family is schema-first and
compiler-pipeline-backed. They already share the same runtime statement
substrate (`Fragment`, `Query`, `Command`), while `schema!` already acts as an
authored-schema adapter that feeds the typed-query compiler through an internal
bridge macro.

The main architectural constraints for unification are now clear:

1. the typed SQL compiler is still SELECT-shaped,
2. live verification is still explicit-codec-shaped,
3. runtime lowering is narrower than the broader codec runtime,
4. and raw execution cannot be removed wholesale because migrations and
   control-plane behaviors require `simple_query_raw`.

## Open Questions

None from code reading. The remaining questions are product/planning choices,
not missing system facts.
