# 8. Migrations

In this chapter we'll point a `Migrator` at a directory of paired
`.up.sql` / `.down.sql` files, ask it for a plan, apply pending
migrations, and roll back when we change our minds.

## Setup

```rust
use std::path::PathBuf;

use babar::migration::FileSystemMigrationSource;
use babar::{Config, Migrator, MigratorOptions, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch08-migrate"),
    )
    .await?;

    let migrator: Migrator<FileSystemMigrationSource> =               // type: Migrator<FileSystemMigrationSource>
        Migrator::with_options(
            FileSystemMigrationSource::new(PathBuf::from("migrations")),
            MigratorOptions::new(),
        );

    // What's applied? What's pending?
    let applied = migrator.applied_migrations(&session).await?;
    let status = migrator.status(&applied)?;
    println!("{status:?}");

    // What would `up` do?
    let plan = migrator.plan_apply(&applied)?;
    println!("plan: {plan:?}");

    // Apply pending migrations.
    let applied_plan = migrator.apply(&session).await?;
    println!("applied: {applied_plan:?}");

    // Roll back the most recent migration.
    let rolled = migrator.rollback(&session, 1).await?;
    println!("rolled back: {rolled:?}");

    session.close().await?;
    Ok(())
}
```

## File layout

`FileSystemMigrationSource` expects pairs of files in one directory:

```text
migrations/
├── 0001__create_users.up.sql
├── 0001__create_users.down.sql
├── 0002__add_email_index.up.sql
└── 0002__add_email_index.down.sql
```

The naming convention is `<version>__<name>.{up,down}.sql`. Versions
sort lexicographically — keep them zero-padded so `10` doesn't sort
before `2`. Each `.up.sql` must have a matching `.down.sql`; missing
or unpaired files surface as a clear `Error` at `Migrator` build
time, not at apply time.

## The migrations table

By default `Migrator` records applied migrations in
`public.babar_migrations`. The schema and table name are configurable
on `MigratorOptions` (`.table(MigrationTable::new(schema, name)?)`),
and there's an advisory-lock id (`.advisory_lock_id(...)`) that
serializes concurrent migrators across processes — only one can hold
the lock and apply at a time, so a deploy that races itself won't
double-apply.

## Plan first, apply second

`migrator.plan_apply(&applied)?` returns a `MigrationPlan` describing
exactly what it would do — same value `apply()` would consume — without
touching the database. Use it for dry-runs in CI, for printing a
migration preview, or for human approval gates.

`migrator.apply(&session).await?` runs the same plan transactionally,
one migration per transaction by default. The transaction mode is
configurable per migration via `MigrationTransactionMode` for the rare
DDL that can't run inside a transaction (`CREATE INDEX
CONCURRENTLY`, for example).

## Rolling back

`migrator.rollback(&session, n).await?` runs the `.down.sql` of the
most recent `n` applied migrations, in reverse. If you need to undo
just one, pass `1`. If you need a planned dry-run first,
`plan_rollback(&applied, n)?` is its read-only sibling.

## The example CLI is just an example

`crates/core/examples/migration_cli.rs` is a thin, helpful wrapper
around the Migrator API — `babar-migrate status`, `plan`, `up`, `down
--steps N`. It's an *example*, not a shipped binary. You can copy it
into your project verbatim, adapt it, or ignore it entirely and call
the `Migrator` API from your own deploy script.

## Next

[Chapter 9: Error handling](./09-error-handling.md) walks through the
`babar::Error` enum and how to classify failures from `apply` and
everything else by inspecting the variant directly.
