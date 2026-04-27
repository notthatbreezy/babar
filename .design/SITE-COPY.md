# babar — Docs Site Copy

A reference for every string that appears on the babar documentation site. Voice cues, image cues, and rationale live alongside the copy so the next person editing the site doesn't have to guess.

> **Voice**: confident, warm, slightly regal. Think *the king who's also the engineer who actually maintains the system.* French sophistication where it lands naturally; never twee.
>
> **Watch out for**: cute-elephant overload (use the herd metaphor sparingly, where it actually maps to the engineering point), corporate-database-vendor stiffness, and unearned superlatives.

---

## 1. Site shell

### Brand name and tagline

**Wordmark**: `babar`
**Tagline**: *Ergonomic Postgres for Rust.*
**Optional sub-tagline (longer surfaces)**: *Typed, async, no surprises.*

> **Image cue**: the green-suited Babar wordmark with the bow-tie underline (from the brand sheet). Keep it on cream `#F8F4E8`, never on pure white.

### Top navigation

| Slot | Copy | Notes |
|------|------|-------|
| Logo | babar | Always lowercase. |
| Nav 1 | Get started | Lands on the quick start. |
| Nav 2 | Guides | Long-form how-tos and tutorials. |
| Nav 3 | Reference | Generated rustdoc plus codec catalog. |
| Nav 4 | Why babar | Comparison + design principles. |
| Nav 5 | GitHub | External link, opens in a new tab. |

### Search placeholder

> Search the herd…

### Footer

**Tagline column**:
> babar — ergonomic Postgres for Rust.
> *Built for the herd. Maintained in the open.*

**Section: Build**
- Quick start
- Tutorials
- Examples
- Migrations
- Pool & TLS

**Section: Reference**
- Rustdoc
- Codec catalog
- Error codes
- Changelog
- MSRV policy

**Section: Community**
- GitHub
- Issues
- Discussions
- Release notes
- Code of conduct

**Bottom row**:
> © 2026 babar contributors · MIT or Apache-2.0 · v0.1.0

---

## 2. Homepage

### Hero

> **Image cue**: hero-left, the "King at the keyboard" illustration (Babar in green suit typing `SELECT * FROM elephants WHERE kingdom = 'Celeste'`).

**Eyebrow**:
> A Postgres driver for Rust

**Headline (H1)**:
> **Type-safe SQL for a sturdy herd.**

**Subhead**:
> babar is a typed, async Postgres driver for Tokio that speaks the wire protocol directly. No `libpq`. No magic. Just queries, codecs, and clear errors — composed the way you'd compose any other Rust value.

**Primary CTA**:
> Start the tutorial →

**Secondary CTA**:
> Read the design notes

**Install line (monospaced, copy button)**:
```
cargo add babar
```

**Trust strip (small caps under the install line)**:
> Postgres 14 · 15 · 16 · 17     ·     Rust 1.75+     ·     0 unsafe blocks     ·     MIT / Apache-2.0

### Why babar (three-up)

> **Image cue**: lift the "Ergonomic by Design / Postgres at Heart / Built for the Herd" pillars row from the brand sheet. Use the green suited "presenter Babar" beside the column if there's room.

| Pillar | Headline | Body |
|--------|----------|------|
| Ergonomic by Design | **Read it once, understand it forever.** | Queries are typed values. Codecs are imported by name. There is one way to start a transaction, one way to bind a parameter, one way to run a migration. You will not spend an afternoon learning which of seven options to use. |
| Postgres at Heart | **The wire protocol, faithfully.** | babar speaks Postgres directly — extended-protocol prepares, binary results, SCRAM-SHA-256, channel binding over TLS, and binary `COPY FROM STDIN` for bulk ingest. No translation layer between you and the server. |
| Built for the Herd | **Predictable under load.** | A single background task owns the socket and serializes wire I/O, so every public call is cancellation-safe. Pool, statement cache, and `tracing` spans are first-class — not bolted on later. |

### Quick start (homepage version)

**Section heading**:
> Connect, type, query.

**Section sub**:
> Three values: a `Config`, a `Command`, and a `Query`. Codecs come in by name so the compiler can read your intent.

```rust
use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session = Session::connect(
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("secret")
            .application_name("hello-babar"),
    ).await?;

    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    session.execute(&insert, (1, "Ada".into())).await?;

    let select: Query<(), (i32, String)> =
        Query::raw("SELECT id, name FROM users ORDER BY id", (), (int4, text));
    let rows = session.query(&select, ()).await?;
    println!("{rows:?}");

    session.close().await?;
    Ok(())
}
```

**Below the snippet, single line**:
> No build script. No offline cache. No env var. The macros are opt-in when you want them.

### Feature grid (six cards)

> **Image cue**: small thumbnail per card — the watercolor illustrations work as quiet section anchors here. Use the "data → code → herd → impact" flow as a divider above the grid.

| Card | Title | One-liner | Link label |
|------|-------|-----------|------------|
| 1 | Typed queries | `Query<Params, Row>` and `Command<Params>` carry their codecs as values. | See the API → |
| 2 | Compile-time SQL | `query!` and `command!` verify against a live database when you set `BABAR_DATABASE_URL`. Off by default. | How verification works → |
| 3 | Caret-rendered errors | Errors carry SQLSTATE, the originating SQL, and a caret pointing at the offending fragment. | Error reference → |
| 4 | Bulk `COPY` | Stream `Vec<T>` into Postgres with binary `COPY FROM STDIN` and `#[derive(Codec)]`. | Bulk ingest guide → |
| 5 | Migrations | Library-first migration engine. Advisory-lock safe, checksummed, transactional by default. | Migrations guide → |
| 6 | Pool, TLS, tracing | deadpool-backed pool. rustls or native-tls. OpenTelemetry-friendly spans out of the box. | Operating babar → |

### Extension showcase

> **Image cue**: the "extensions panel" illustration (pgvector, postgis, pg_trgm, hstore, citext, pgcrypto, pg_partman, timescaledb) lifted from the brand sheet, used as a full-width band.

**Section heading**:
> One database. The whole herd of extensions.

**Section sub**:
> babar ships codec families behind feature flags so you only carry what you use. The extension stays in Postgres; the typed view lives in your Rust.

| Extension | Feature flag | What you get |
|-----------|-------------|--------------|
| pgvector | `pgvector` | A `Vector` wrapper plus a dynamic `vector` codec that resolves the extension OID per session. |
| PostGIS | `postgis` | Binary codecs for common 2D shapes (`Point`, `LineString`, `Polygon`, multis), with optional SRID metadata. |
| hstore | `hstore` | A stable `Hstore` map wrapper backed by binary codec. |
| citext | `citext` | Case-insensitive text mapped to Rust `String`. |
| Full-text search | `text-search` | `TsVector` and `TsQuery` wrappers, exact to the SQL form. |
| Ranges & multiranges | `range`, `multirange` | Built-in scalar range families with binary inner codecs. |
| And more | `uuid`, `time`, `chrono`, `json`, `numeric`, `net`, `interval`, `array`, `macaddr`, `bits` | All opt-in. None pulled into your build unless you ask. |

**Section footer**:
> Need an extension we don't ship? `#[derive(Codec)]` and `Encoder`/`Decoder` are public. Bring your own.

### Comparison band

> **Image cue**: the "Building Better Together" round seal as a quiet decoration to the right.

**Section heading**:
> Honest about trade-offs.

**Two-column compare**:

**babar vs sqlx**

> **Where babar wins**: explicit runtime codecs, no compile-time database for normal builds, SQL-origin caret rendering on every error.
>
> **Where sqlx wins**: broader compile-time macros, broader database coverage, larger ecosystem and longer production track record.

**babar vs tokio-postgres**

> **Where babar wins**: typed query/command values are the API, prepare-time schema validation, richer error rendering with SQL origin tracking.
>
> **Where tokio-postgres wins**: years of production hardening, broader feature coverage today (notably `COPY TO`, text/CSV `COPY`, `LISTEN`/`NOTIFY`, out-of-band cancel), no need to buy into the explicit codec model.

**Section footer**:
> Pick the tool that fits the job. babar is the right fit when you want one obvious way to do things and you'd rather see a typed value than a clever macro.

### Closing CTA band

> **Image cue**: the "herd embracing in front of the castle" illustration as a soft full-bleed background, dimmed to ~20% opacity.

**Headline**:
> **Bring your queries home.**

**Body**:
> babar is open source under MIT or Apache-2.0. The tutorial walks you from `cargo new` to a deployed Axum service in an afternoon.

**Primary CTA**:
> Start the tutorial →

**Secondary CTA**:
> Browse the source on GitHub

---

## 3. Get-started page

### Page hero

**Eyebrow**: First contact
**H1**: **Get started in five minutes.**
**Sub**: You'll connect to a local Postgres, run one `Command`, run one `Query`, and read a row back. That's the whole loop.

**Prereq callout**:
> **You'll need**
> · Rust 1.75 or newer
> · A local Postgres 14+ (Docker is fine)
> · Five quiet minutes

### Step headings

1. **Add babar to your project.**
2. **Spin up a Postgres.**
3. **Write your first session.**
4. **Run it.**
5. **What just happened?**

### Step microcopy snippets

**After step 1**:
> One crate, no companion macro crate to install separately. Feature flags are additive.

**After step 2 (Docker block)**:
> Any Postgres works. We test against 14, 15, 16, and 17.

**After step 3**:
> Notice the codecs come in as values: `(int4, text)`. That tuple *is* the type signature for your parameters — the compiler will tell you if you reorder them.

**After step 5**:
> You started a `Session`, which is a handle to a background driver task that owns the socket. Every call you made was cancellation-safe — drop the future and the in-flight command unwinds cleanly.

### Next steps card

**Heading**: Where to next?

> · **Build a real service** with the [Postgres-API-from-scratch tutorial](./tutorials/postgres-api-from-scratch.md).
> · **Type your domain** with [`#[derive(Codec)]`](./guides/derive-codec.md).
> · **Operate it** with [pool, TLS, and tracing](./guides/operating-babar.md).

---

## 4. Reference & catalog pages

### Codec catalog page

**H1**: **Codec catalog.**
**Sub**: Every codec babar ships, what it maps to, and the feature flag (if any) that turns it on.

**Empty-search state**:
> No codec matches that filter. Try `int`, `text`, `json`, or `geo`.

**Per-row helper**:
> Click a codec name for the SQL OID, the binary wire format notes, and an example.

### Error reference page

**H1**: **Error reference.**
**Sub**: Every public `Error` variant babar can return, what causes it, and what to do about it. Error messages carry SQLSTATE, the originating SQL, and a caret. We try to leave you with somewhere obvious to go next.

**Per-error template**:

> **`<ErrorVariant>`**
> *What happened.* One sentence.
> *Why.* One or two sentences with the typical cause.
> *Fix.* Concrete next step.

---

## 5. Microcopy library

### Buttons & CTAs

| Context | Copy |
|---------|------|
| Primary docs CTA | Start the tutorial |
| Secondary docs CTA | Read the design notes |
| Install command copy button | Copy |
| After copy | Copied — go forth |
| GitHub external link | View on GitHub |
| Edit-this-page link | Improve this page on GitHub |
| Search submit | Search |
| Pagination next | Next chapter → |
| Pagination prev | ← Previous chapter |

### Status & toast messages

| State | Copy |
|-------|------|
| Code copied | Copied to clipboard. |
| Search no results | Nothing matched. Try a broader term, or [open an issue](#) — maybe we should write that page. |
| Page-load failure | The herd's a bit slow today. Refresh and try again. |
| Offline | You're offline. The pages you've already visited still work. |

### Banners

**New release banner**:
> 🟢 **babar 0.1.0 is out.** Read the [release notes](#) or jump straight to [what's new](#).

**Pre-release banner (when applicable)**:
> Pre-release docs. APIs in this version may shift before 1.0.

**Deprecation banner**:
> This page documents `<thing>`, which was retired in v`<X.Y>`. See [`<replacement>`](#) for the supported alternative.

### Empty states

**No results in tutorial search**:
> **Nothing here yet.** We're still writing this corner of the docs. Have a request? [Tell us what you're stuck on.](#)

**Empty changelog filter**:
> **No releases in that range.** babar ships when it's ready, not on a calendar.

**Empty examples folder (preview UI)**:
> **No examples to show.** Examples live in [`crates/core/examples/`](#) — we'll surface them here once you've cloned the repo.

### Confirmation dialogs

| Trigger | Title | Body | Confirm | Dismiss |
|---------|-------|------|---------|---------|
| Reset playground state | Clear the playground? | Your unsaved snippets will be gone. The schema and seed data stay. | Clear playground | Keep editing |
| Switch tutorial language (when added) | Switch language? | We'll reload the page. Your scroll position will be preserved. | Switch | Stay here |

### Tooltips

| Element | Tooltip |
|---------|---------|
| `cancellation-safe` badge | Drop the future at any time — the in-flight command unwinds cleanly without leaving the connection in a bad state. |
| `feature-gated` badge | Behind a Cargo feature flag. Add it to your `Cargo.toml` to enable. |
| `MSRV 1.75` badge | Minimum supported Rust version. We bump it deliberately, not casually. |
| `0 unsafe` badge | The core crate forbids `unsafe`. Verified in CI by Miri. |
| `binary protocol` badge | Uses Postgres' binary wire format — no text parsing on the hot path. |

### Form fields (newsletter / feedback)

**Email label**: Your email
**Email placeholder**: ada@example.com
**Submit**: Subscribe to release notes
**Helper**: One email per release. No marketing. Unsubscribe in one click.
**Success**: You're on the list. Welcome to the herd.
**Error (invalid email)**: That doesn't look like an email — mind double-checking?
**Error (already subscribed)**: You're already on the list. Nothing else to do.

**Feedback prompt** (bottom of every page):
> **Was this page useful?** 👍 Yes · 👎 Not really
> *(Followup if 👎)*: What were you trying to do? We read every response.

### 404 page

**H1**: **The herd's wandered off.**
**Sub**: This page doesn't exist — or it used to and we moved it. Try the [home page](/), the [tutorial](#), or the [search bar](#).

### 500 page

**H1**: **Something broke on our side.**
**Sub**: Refresh the page. If it keeps happening, please [open an issue](#) and tell us what you were trying to read. We'll fix it.

---

## 6. Voice & tone reference

### When to lean into the herd metaphor

Use sparingly, and only where the engineering meaning lands:

- **"Built for the herd"** — multi-process safety, pool behavior, advisory locks.
- **"Strong herd"** — type safety, compile-time checks, no runtime surprises.
- **"Welcome to the herd"** — onboarding moments only (newsletter signup, first run, install).

Avoid: stretching it across every section heading, calling users "elephants," anything cutesy at the moment of failure.

### Tone by context

| Context | Tone | Example |
|---------|------|---------|
| First-run success | Quietly proud | "You're connected. Let's run a query." |
| Build success | Plain | "All migrations applied successfully." |
| Recoverable error | Empathetic, specific | "Schema mismatch on column `email`. The query expected `text`, the database has `varchar`. Re-run `babar migrate` or update your codec." |
| Unrecoverable error | Honest, brief | "The connection was closed by the server. babar can't continue." |
| Deprecation | Direct, with a path | "`old_thing` is gone in 0.2. Use `new_thing` instead — same shape, fewer surprises." |
| Empty state | Inviting | "No projects yet. The first one is the hardest." |
| Loading | Reassuring without lying | "Compiling… (this is the longest step)." |

### Phrases to keep

- "ergonomic Postgres for Rust"
- "no surprises"
- "one obvious way"
- "the wire protocol, faithfully"
- "built for the herd"

### Phrases to retire

- "blazingly fast" (unverified, overused)
- "best-in-class"
- "robust" (means nothing)
- "elephants in the room" (cute but adds nothing)
- "trumpet" / "trumpeting" (please, no)

### Localization notes

- Avoid the herd metaphor in headlines for translated builds — it doesn't carry across all languages, and translators will reach for the literal animal noun. Prefer the engineering phrasing for translated headers; keep the herd flavor in marketing surfaces only.
- French copy is not currently shipped, despite the Babar of it all. If we add it, the brand permits "Bonjour, le troupeau" and "Ça marche" in casual moments only — not in error messages.
- Watch for character-expansion on the install line (`cargo add babar` is the same in every language, so no growth issue there).

---

## 7. Image cues — quick map

| Surface | Illustration |
|---------|--------------|
| Hero | "King at the keyboard" — Babar typing the `SELECT … kingdom = 'Celeste'` query. |
| Three-pillar row | The bottom-of-the-brand-sheet pillar icons (feather, elephant, layers, shield, herd). |
| Quick-start section | "Deploy, repeat" — Babar at laptop with headphones. |
| Feature grid divider | "Data → Code → Herd → Impact" flow. |
| Extension band | The full extensions panel (pgvector, postgis, pg_trgm, hstore, citext, pgcrypto, pg_partman, timescaledb). |
| Comparison band | "Building Better Together" round seal as quiet decoration. |
| Closing CTA | "Herd embracing in front of the castle," soft full-bleed at low opacity. |
| 404 page | "King with map / pointing at globe" — wandered off. |
| Newsletter success | "Nurturing better APIs together" watering-can vignette. |

---

*Last reviewed: 2026-04-26. When you change copy, change the rationale too.*
