# Migration Support Plan

## Summary
Add a first-class PostgreSQL migration system for application startup and deploy workflows, built around **paired SQL files** (`up` / `down`) with both a **library API** and a **CLI**.

The intended v1 safety model includes:
- migration tracking table
- checksums
- drift detection
- advisory locking
- transactional execution where possible
- dry-run / plan output
- rollback support
- idempotent startup integration

## Current State Analysis
- babar does not currently have a migration subsystem.
- The existing library already has strong primitives for one:
  - `Session`
  - `Transaction` / `Savepoint`
  - `Pool`
  - explicit errors and SQL reporting
- There is already a small `clap`-based CLI example (`todo_cli`), which is a good pattern for a future migration CLI wrapper.

## Key Decisions
- **Database migrations**, not repo-internal API migration planning.
- **Simple SQL files** with paired `up` / `down`.
- **Library API plus CLI**, with the library as the real engine.
- **Postgres-native safety features** like advisory locks should be used directly.
- **Strict checksum/drift enforcement** should be default behavior.

## Work Items

### 1. `migration-architecture`
- Define migration file naming/layout.
- Define the migration state table schema.
- Define the public library API shape.
- Decide how non-transactional migrations are declared.

Architecture decisions sharpened in code:
- File grammar: `<version>__<name>.up.sql` and `<version>__<name>.down.sql`
  where `version` is a `u64` and `name` is lowercase `snake_case`.
- Non-transactional execution is a per-file top-of-file pragma:
  `--! babar:transaction = none`
- Default state table: `public.babar_schema_migrations`
  with `version`, `name`, `up_checksum`, `down_checksum`,
  `up_transaction_mode`, `down_transaction_mode`, and `applied_at`.
- Shared library entry point: `Migrator<S>` with `MigratorOptions`,
  backed by a `MigrationSource` abstraction so the eventual CLI stays thin.

### 2. `migration-source-and-plan`
- Discover migrations from disk.
- Validate ordering and pairing.
- Compute and compare checksums.
- Build status / dry-run / plan output.
- Detect drift.

### 3. `migration-runner`
- Acquire/release advisory lock.
- Apply pending `up` migrations.
- Roll back `down` migrations.
- Handle transactional vs non-transactional execution.
- Persist migration history.

### 4. `migration-cli`
- Build a CLI wrapper over the shared engine.
- Support at least `status`, `plan`, `up`, and `down`.
- Reuse babar-style config/env loading where practical.

### 5. `migration-validation-docs`
- Add integration tests for happy path and failure modes.
- Add lock/concurrency tests.
- Add drift/checksum tests.
- Add docs/examples for CLI use and startup integration.

## Suggested Phase Breakdown

### Phase 1: Architecture
- file grammar
- table schema
- API design

### Phase 2: Planning layer
- source discovery
- checksum model
- drift detection
- dry-run/status

### Phase 3: Runner
- advisory lock
- up/down execution
- transaction policy

### Phase 4: CLI
- thin wrapper commands
- output formatting

### Phase 5: Docs and validation
- integration tests
- startup example
- user-facing docs

## Notes
- The cleanest v1 is library-first with a thin CLI wrapper, not two separate implementations.
- PostgreSQL-only scope is a strength here: advisory locks and transactional DDL can be part of the core design instead of optional abstractions.
- Non-transactional migrations need an explicit opt-out mechanism from the start, even if most migrations stay transactional.

## Final Status Notes
- `migration-validation-docs` completed with end-to-end validation for the library-first migration engine, filesystem source, and CLI wrapper.
- User-facing docs now cover migration file grammar, startup-safe `Migrator` usage, CLI commands, drift/checksum behavior, advisory locking, transaction modes, and rollback limits.
- Validation passed with `cargo fmt --check`, `cargo clippy -p babar --all-targets --all-features -- -D warnings`, `cargo test -p babar --all-features`, and `RUSTDOCFLAGS="-D warnings" cargo doc -p babar --all-features --no-deps`.
