# 7. Bulk loads with COPY

In this chapter we'll ingest many rows in a single round-trip with
binary `COPY FROM STDIN`. We'll also be honest about what babar's
COPY support doesn't do yet.

## Setup

```rust
use babar::query::Query;
use babar::{Config, CopyIn, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct VisitRow {
    id: i32,
    email: String,
    active: bool,
    note: Option<String>,
    visits: i64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch07-copy"),
    )
    .await?;

    session
        .simple_query_raw(
            "CREATE TEMP TABLE bulk_visits (\
                id int4 PRIMARY KEY,\
                email text NOT NULL,\
                active bool NOT NULL,\
                note text,\
                visits int8 NOT NULL\
            )",
        )
        .await?;

    let rows = vec![
        VisitRow { id: 1, email: "ada@example.com".into(),  active: true,  note: Some("first".into()), visits: 7  },
        VisitRow { id: 2, email: "bob@example.com".into(),  active: false, note: None,                 visits: 3  },
        VisitRow { id: 3, email: "cara@example.com".into(), active: true,  note: Some("news".into()),  visits: 12 },
    ];

    let copy: CopyIn<VisitRow> = CopyIn::binary(                      // type: CopyIn<VisitRow>
        "COPY bulk_visits (id, email, active, note, visits) FROM STDIN BINARY",
        VisitRow::CODEC,
    );
    let affected: u64 = session.copy_in(&copy, rows.clone()).await?;  // type: u64
    println!("copied {affected} rows");

    let select: Query<(), VisitRow> = Query::raw(
        "SELECT id, email, active, note, visits FROM bulk_visits ORDER BY id",
        (),
        VisitRow::CODEC,
    );
    for row in session.query(&select, ()).await? {
        println!("{row:?}");
    }

    session.close().await?;
    Ok(())
}
```

## What `CopyIn::binary` is doing

`CopyIn::binary(sql, codec)` describes a `COPY ... FROM STDIN BINARY`
statement plus a codec for one row. `session.copy_in(&copy, rows)`
sends Postgres' binary COPY framing — a header, one length-prefixed
binary tuple per row, and a trailer — and returns the rows-affected
count once the server acknowledges.

The `babar::Codec` derive on `VisitRow` expands to an
`Encoder<VisitRow>` / `Decoder<VisitRow>` pair, with field order
matching the struct. That same `VisitRow::CODEC` is reusable for a
`SELECT` decoder, as the example shows. One row type, one codec, two
directions.

## Why "binary" and "STDIN"?

- **Binary** beats text for throughput: no string parsing on the
  server, no escaping rules, exact round-trip for `bytea`, `numeric`,
  timestamps, and so on.
- **STDIN** is the direction where babar streams *into* Postgres. The
  driver task feeds rows as you produce them, so memory usage stays
  bounded — you can pass an iterator of millions of rows without
  buffering them all.

## What COPY support does **not** include yet

babar's COPY support is deliberately narrow at the moment:

- `COPY ... TO STDOUT` (reading rows back via COPY) is **not yet
  implemented** — it's on the roadmap, see
  [explanation/roadmap.md](../explanation/roadmap.md).
- Text and CSV formats (`FORMAT text`, `FORMAT csv`) are **deferred**.
  Use `BINARY` for now.
- `COPY FROM PROGRAM` and `COPY ... FROM <file>` are server-side; they
  don't go through the driver and aren't part of babar's surface.

If you need text/CSV ingest today, format the rows yourself and use
`session.execute` with multi-row `INSERT`s. That's slower, but it
works against the same `Session`.

## Next

[Chapter 8: Migrations](./08-migrations.md) introduces `Migrator`,
`FileSystemMigrationSource`, and the migrations table.
