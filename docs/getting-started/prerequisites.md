# Prerequisites

Before you connect, you need a Postgres to connect *to*. The cheapest
debugger you'll get on this whole journey is a Postgres that prints
every byte it does back at you, so let's run one of those.

## A Postgres that talks back

Open a terminal, paste this, and leave it running. It's a throwaway
container — `--rm` means it disappears when you `Ctrl-C`, so nothing
leaks past your tutorial session.

```bash
docker run --rm -it \
  --name babar-pg \
  -p 5432:5432 \
  -e POSTGRES_PASSWORD=postgres \
  postgres:17 \
  -c log_statement=all \
  -c log_min_duration_statement=0 \
  -c log_connections=on \
  -c log_disconnections=on
```

What each flag is doing for you:

- `--rm -it` — foreground, throwaway, `Ctrl-C` to stop. No daemon, no
  cleanup chores later.
- `-p 5432:5432` — Postgres' default port, exposed on `localhost`.
- `-e POSTGRES_PASSWORD=postgres` — sets the password for the default
  `postgres` superuser. The `postgres:17` image already creates that
  role and a database of the same name on first boot, so we just need
  to give it a password.
- `-c log_statement=all` — every SQL statement gets logged.
- `-c log_min_duration_statement=0` — every statement also gets a
  duration logged, no threshold.
- `-c log_connections=on` / `-c log_disconnections=on` — connection
  lifecycle in the same stream.

The connection string for everything that follows is:

```text
postgres://postgres:postgres@localhost:5432/postgres
```

…which in `Config` form is:

```rust
use babar::Config;

let cfg = Config::new("localhost", 5432, "postgres", "postgres")  // type: Config
    .password("postgres")
    .application_name("first-query");
```

## Why foreground?

Because the second window — the one tailing those logs — is where
you'll see *exactly* what babar sent on the wire. Prepared-statement
names, parameter values, every `BEGIN` and `COMMIT`. When something
surprises you in chapter 3 or chapter 7, your first move is to glance
at that window. It is faster than any println you will ever write.

## Stop it

`Ctrl-C` in the Postgres window. `--rm` cleans up the container; the
data goes with it. That's the point — every tutorial run is a fresh
database.

## Next

- [Your first query →](first-query.md) — connect, query, decode.
