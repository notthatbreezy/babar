# Mdbook Docs Rewrite Implementation Plan

## Overview

Rewrite the babar mdbook site under `docs/` from a 9-line stub plus one tutorial into a real Diataxis-aligned book in doobie voice. Work is sequenced so the build stays green from the very first phase: asset relocation, `book.toml` updates, and the full `SUMMARY.md` skeleton land first (with placeholder content), then chapters are drafted in groups, then polish.

The plan is documentation-only. No code under `crates/`, no `Cargo.toml`, no CI workflow files are modified. The technical source-of-truth for every chapter is `.paw/work/mdbook-docs-rewrite/CodeResearch.md`; the voice and microcopy authority is `.design/SITE-COPY.md` (post-relocation).

## Current State Analysis

From `Spec.md` and `CodeResearch.md`:

- `docs/` today contains: a 9-line `index.md` stub, a 4-line `SUMMARY.md` listing only the existing tutorial, `docs/SITE-COPY.md` (452 lines — voice/microcopy brief, not a published page), `docs/landing-mockup.html` (styled HTML mockup, not a published page), and `docs/tutorials/postgres-api-from-scratch.md` (1162 lines — strong, kept verbatim per Spec A4 and FR-007).
- `book.toml` currently has `title = "babar tutorial"` and `src = "docs"`. Per Spec A1 (user-confirmed at spec milestone), the title becomes `The Book of Babar`.
- Brand imagery lives at repo-root `images/`: four PNGs with whitespace + timestamp filenames (`ChatGPT Image Apr 26, 2026, 10_48_*.png`) and four corresponding Windows `:Zone.Identifier` sidecars. None of these are tracked by git yet (`git status -s` shows them as untracked).
- The user-facing Rust crate is `babar` (`crates/core/`), with re-exports defined in `crates/core/src/lib.rs`. The public surface is exactly: `Config`, `Session`, `Pool`, `Transaction`/`Savepoint`, `CopyIn`, `Migrator`, `Error`/`Result`, `codec` module, `Query`/`Command`/`Fragment`, and macros `sql!`, `query!`, `command!`, `#[derive(Codec)]`.
- `crates/core/examples/` contains 11 examples that map directly onto chapters: `quickstart.rs` (→ Get Started), `pool.rs` (→ Pooling), `transactions.rs` (→ Transactions), `prepared_and_stream.rs` (→ Prepared & Streaming), `copy_bulk.rs` (→ COPY), `derive_codec.rs` (→ Custom Codecs), `migration_cli.rs` (→ Migrations), `axum_service.rs` (→ Web Service), `todo_cli.rs` (→ supplementary), `m0_smoke.rs` and `playground.rs` (→ smoke/dev only, not user-facing chapters).
- CI: `.github/workflows/pages.yml` builds the mdbook site. `mdbook test` is not wired up. Spec A5 accepts this.
- Gaps from CodeResearch that constrain chapter content (must be reflected in prose, not fabricated around):
  - No DSN / `PG*` env-var parsing — `Config` is struct-only.
  - No `Error::kind()` classifier; `Error::Server { code: String, … }` carries SQLSTATE as a string. The Reference "error catalog" page documents the 11 enum variants and the `Server { code, … }` shape; SQLSTATE→guidance content is editorial, not extracted.
  - COPY is binary `COPY FROM STDIN` only — no `COPY TO`, no text/CSV.
  - SCRAM-SHA-256-PLUS, LISTEN/NOTIFY, out-of-band cancel are deferred.
- Image-cue mapping: `docs/SITE-COPY.md` §7 references nine cues but only four PNGs exist. Implementation must pick visually during Phase 2.

## Desired End State

- `docs/SITE-COPY.md` and `docs/landing-mockup.html` no longer exist under `docs/`; both are tracked at `.design/` at the repo root and are not built into the site.
- The four brand PNGs live at `docs/assets/img/` with kebab-case names; `:Zone.Identifier` sidecar files do not exist anywhere in the working tree.
- `book.toml` has `title = "The Book of Babar"`. The existing `[output.html]` configuration is preserved (`navy` theme, `git-repository-url`, `site-url`).
- `docs/SUMMARY.md` reflects the four-section hierarchy (landing → Get Started → Book → Reference → Explanation → Tutorial), with every linked file present.
- `docs/index.md` is a doobie-style landing page: babar wordmark + tagline (*Ergonomic Postgres for Rust.*), one brand image, three pillars, a runnable code snippet, primary "Get Started" CTA. (FR-001, SC-001.)
- A "Get Started" first-query quickstart exists at `docs/getting-started/first-query.md` (FR-002).
- All thirteen numbered Book chapters (FR-003) exist as full prose drafts under `docs/book/`, each meeting the chapter-shape gate from SC-004.
- Reference section under `docs/reference/`: codec catalog, error catalog, feature flags, configuration knobs (FR-005).
- Explanation section under `docs/explanation/`: design philosophy, comparison, background-driver-task model, no-unsafe / validate-early, roadmap (FR-006).
- Existing `docs/tutorials/postgres-api-from-scratch.md` is byte-identical to `main` (FR-007, SC-007).
- `mdbook build` exits 0 with no warnings (FR-013, SC-013).
- `git diff main -- crates/ Cargo.toml '**/Cargo.toml'` is empty (FR-014, SC-014).
- `git ls-files | grep ':Zone.Identifier'` returns nothing (SC-010).
- Voice: every new page passes the SC-008 mechanical chapter-shape check (≥1 second-person pronoun and ≥1 fenced code block before the first prose paragraph after setup) and shows no British-English markers.

**Verification approach** (composite end-to-end):

1. `mdbook build` — must exit 0 with no warnings on stderr.
2. Manual: open the built `book/` site root in a browser, click through the SUMMARY top-to-bottom, confirm every link resolves and every image loads.
3. `git diff --stat main -- crates/ '**/Cargo.toml'` — empty.
4. `! find . -path ./.git -prune -o -name '*Zone.Identifier*' -print | grep .` — exits non-zero (no `:Zone.Identifier` files anywhere on the filesystem under the repo).
5. `git diff main -- docs/tutorials/postgres-api-from-scratch.md` — empty (verbatim preservation).
6. `! grep -RInE '(behaviour|colour|organis|optimis|catalogu)' docs/` — exits 0 outside the verbatim-preserved tutorial.
7. Spot-check three random Book chapters against the chapter-shape gate (SC-004).

## What We're NOT Doing

- **Not editing the existing 1162-line tutorial** (`docs/tutorials/postgres-api-from-scratch.md`). It is preserved byte-identical per Spec A4 / FR-007 / SC-007. The only acceptable touches would be image-path fixes if the tutorial referenced any (CodeResearch confirms it has zero internal links / image refs, so this is moot).
- **Not modifying `crates/`**, **not modifying any `Cargo.toml`**, **not modifying the CI workflow at `.github/workflows/pages.yml`** (Spec FR-014 / out-of-scope).
- **Not adding mdbook preprocessors** (Spec A8). Stock mdbook only.
- **Not embedding rustdoc** into the site (Spec out-of-scope). The Reference section *links* to docs.rs.
- **Not running `mdbook test`** (Spec A5 / CodeResearch §20). Code samples are sourced from `crates/core/examples/*.rs` and validated by inspection against `crates/core/src/`, not by a build step.
- **Not setting up new CI** for the docs (out-of-scope).
- **Not changing the mdbook theme** beyond using existing `navy` and adding asset references.
- **Not fabricating APIs that don't exist**. Where the codebase lacks something a chapter would naturally cover (DSN parsing, `Error::kind()`, `COPY TO`, LISTEN/NOTIFY), the chapter says "not yet implemented" or refers to MILESTONES.md, never invents an API.
- **Not writing comparison claims about other libraries blind**. Phase 7 (Explanation) surfaces each comparison claim from `SITE-COPY.md` §2 to the user for confirmation before publishing.
- **Not committing `:Zone.Identifier` files anywhere**. They are deleted in Phase 1, not relocated.
- **Not introducing translation / non-English content**. American English only.

## Phase Status

- [x] **Phase 1: Asset Relocation & Site Skeleton** — Move authoring artifacts out, relocate brand images, update `book.toml`, scaffold the full `SUMMARY.md` hierarchy with placeholder pages so `mdbook build` is green from this point onward.
- [x] **Phase 2: Landing Page & Quickstart** — Author `docs/index.md` (doobie hero) and `docs/getting-started/first-query.md`.
- [x] **Phase 3: Book Foundations (Chapters 1–4)** — Connecting, Selecting, Parameterized Commands, Prepared Queries & Streaming.
- [ ] **Phase 4: Book Composition (Chapters 5–8)** — Transactions, Pooling, Bulk Loads (COPY), Migrations.
- [ ] **Phase 5: Book Production (Chapters 9–13)** — Error Handling, Custom Codecs / `derive(Codec)`, Web Service (Axum), TLS & Security, Observability/Tracing.
- [ ] **Phase 6: Reference Section** — Codec catalog, error catalog, feature flags, configuration knobs.
- [ ] **Phase 7: Explanation / Why Babar** — Design philosophy, comparison, background-driver-task model, no-unsafe / validate-early, roadmap.
- [ ] **Phase 8: Polish, Voice Sweep & Documentation** — American-English/voice sweep across all new content, `mdbook build` green & warning-free with full content, Docs.md and CHANGELOG entry.

## Phase Candidates

<!-- Capture ideas that arise during implementation but aren't part of the eight planned phases. -->
<!-- Example: - [ ] Add an mdbook-toc preprocessor for in-page tables of contents -->

---

## Phase 1: Asset Relocation & Site Skeleton

**Objective**: Get the working tree into its final shape (assets relocated, SUMMARY full, every linked page exists at least as a placeholder) so every subsequent phase only adds prose to existing files and `mdbook build` stays green throughout.

### Changes Required

- **Authoring artifacts** — move out of `docs/`:
  - Create `.design/` at repo root.
  - `git mv docs/SITE-COPY.md .design/SITE-COPY.md`
  - `git mv docs/landing-mockup.html .design/landing-mockup.html`
  - Add a tiny `.design/README.md` explaining: "Authoring artifacts (voice brief, landing mockup). Not part of the published mdbook site at `docs/`."
- **Brand images** — relocate from repo-root `images/` to `docs/assets/img/` (authoritative mapping confirmed by maintainer):
  - Create `docs/assets/img/`.
  - Rename targets (Spec A3, source-of-truth):
    - `ChatGPT Image Apr 26, 2026, 10_48_17 PM.png` → `babar-extensions.png` (Postgres extensions showcase)
    - `ChatGPT Image Apr 26, 2026, 10_48_27 PM.png` → `babar-brand-sheet.png` (master brand sheet — landing-page hero)
    - `ChatGPT Image Apr 26, 2026, 10_48_32 PM.png` → `babar-scenes.png` (seven-panel scene grid)
    - `ChatGPT Image Apr 26, 2026, 10_48_23 PM.png` → `babar-collage-alt.png` (alternate brand collage)
  - **Filesystem-rename safety** (Spec edge case): all four target names differ from the source whitespace-laden filenames in characters beyond case alone; safe on case-insensitive filesystems.
  - Use `mv` (NOT `git mv`) because the source files are untracked. After the move, run `git add docs/assets/img/` so the four PNGs are tracked.
  - Delete all four `:Zone.Identifier` sidecar files (root-owned; `sudo` may be required).
  - Remove the now-empty `images/` directory.
- **Image-cue policy** (resolves the SITE-COPY §7 nine-cue / four-PNG mismatch flagged in Current State Analysis): where SITE-COPY anticipates an image cue but no corresponding asset exists in the four available PNGs, the chapter or section **omits** the image rather than reusing one from elsewhere. This policy applies during Phases 2–7. Image reuse is allowed only where SITE-COPY explicitly calls for the same asset twice.
- **`book.toml`**:
  - Change `title = "babar tutorial"` → `title = "The Book of Babar"`.
  - Preserve all other fields. (`src = "docs"`, the `[output.html]` block, `git-repository-url`, `site-url`.)
  - No preprocessor additions (per Spec A8).
- **`docs/SUMMARY.md`** — rewrite to the full hierarchy. Every entry below points at a file that must exist by end of Phase 1 (placeholders are fine, full prose comes in later phases):

  ```markdown
  # Summary

  [Welcome](index.md)

  # Get Started
  - [Your first query](getting-started/first-query.md)

  # The Book of Babar
  - [1. Connecting](book/01-connecting.md)
  - [2. Selecting](book/02-selecting.md)
  - [3. Parameterized commands](book/03-parameterized-commands.md)
  - [4. Prepared queries & streaming](book/04-prepared-and-streaming.md)
  - [5. Transactions](book/05-transactions.md)
  - [6. Pooling](book/06-pooling.md)
  - [7. Bulk loads with COPY](book/07-copy.md)
  - [8. Migrations](book/08-migrations.md)
  - [9. Error handling](book/09-error-handling.md)
  - [10. Custom codecs](book/10-custom-codecs.md)
  - [11. Building a web service](book/11-web-service.md)
  - [12. TLS & security](book/12-tls.md)
  - [13. Observability](book/13-observability.md)

  # Reference
  - [Codec catalog](reference/codecs.md)
  - [Error catalog](reference/errors.md)
  - [Feature flags](reference/feature-flags.md)
  - [Configuration knobs](reference/configuration.md)

  # Explanation
  - [Why babar](explanation/why-babar.md)
  - [Design principles](explanation/design-principles.md)
  - [Comparisons](explanation/comparisons.md)
  - [The background driver task](explanation/driver-task.md)
  - [Roadmap](explanation/roadmap.md)

  # Tutorial
  - [Postgres API from scratch](tutorials/postgres-api-from-scratch.md)
  ```

- **Placeholder content for every new file** — each placeholder file contains only a `# {Chapter title}` heading plus a one-line "Coming in Phase N" note (in HTML comment so it doesn't render). This keeps `mdbook build` warning-free.
- **`docs/index.md`**: replace stub with a placeholder heading + comment (full content lands in Phase 2).
- **`.gitignore`** — verify `book/` (mdbook output) is already ignored; add it if not. (Currently `book/` shows as untracked — confirm and add to root `.gitignore` if absent.)

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` from repo root exits 0 with no warnings on stderr.
- [ ] No `:Zone.Identifier` files anywhere on disk under the repo: `! find . -path ./.git -prune -o -name '*Zone.Identifier*' -print | grep .` (exits non-zero — no matches).
- [ ] The four brand PNGs are git-tracked under `docs/assets/img/`: `[ "$(git ls-files docs/assets/img/ | wc -l)" -eq 4 ]`.
- [ ] `! test -f docs/SITE-COPY.md && ! test -f docs/landing-mockup.html` (both removed from `docs/`).
- [ ] `test -f .design/SITE-COPY.md && test -f .design/landing-mockup.html` (both present in `.design/`).
- [ ] `test -d docs/assets/img && [ "$(ls docs/assets/img/ | wc -l)" -eq 4 ]` (four images, no sidecars).
- [ ] `! test -d images` at repo root (or directory contains only non-book residue, explicitly justified).
- [ ] `grep -E '^title = "The Book of Babar"$' book.toml` matches.
- [ ] `git diff --stat main -- crates/ '**/Cargo.toml'` is empty.

#### Manual Verification
- [ ] Open the built site: confirm SUMMARY renders all sections; placeholder pages render as their `# Title` heading; no broken links visible in the sidebar.
- [ ] Confirm the four renamed images are visually appropriate for their proposed kebab names; if not, rename in this phase rather than deferring.

---

## Phase 2: Landing Page & Quickstart

**Objective**: Replace the placeholders for the landing page and the first-query quickstart with full doobie-style prose. After this phase, the P1 user story (first-time evaluator) is satisfied end-to-end.

### Changes Required

- **`docs/index.md`** — full doobie-style landing page:
  - Hero: `babar` wordmark, tagline *Ergonomic Postgres for Rust.*, sub-tagline *Typed, async, no surprises.*, one brand image (recommended: `babar-brand-sheet.png` — master brand sheet).
  - Three pillars (sourced from `.design/SITE-COPY.md` §2 "Why babar three-up").
  - One runnable code snippet (a stripped-down `SELECT 1` from `crates/core/examples/quickstart.rs`, with inline `// type: T` annotations).
  - Primary CTA: link to `getting-started/first-query.md`.
  - Voice: per `.design/SITE-COPY.md`. American English.
- **`docs/getting-started/first-query.md`** — first-query chapter:
  - Pattern: doobie chapter 1. Imports + setup at top, then code, then prose.
  - Source: `crates/core/examples/quickstart.rs` (cite file:line in chapter notes if needed).
  - Show: `Config::new(host, port, user, database)` (NOT a `Config::host()` builder — the constructor takes positional fields per CodeResearch §2; chained methods like `.password(...)`, `.application_name(...)` are for *optional* fields), `.connect()` returning `Session`, then a typed `query!` macro invocation OR `Query::raw(sql_fragment, encoder, decoder)` to read a row. Make the `sql!` (Fragment) → `query!` (Query) → `session.query(&query, args)` chain explicit so the reader doesn't think `session.query(sql!(...))` works directly — it does not, because `sql!` produces `Fragment<A>` and `session.query` takes `&Query<A, B>`.
  - Inline `// type: Session`, `// type: Query<(), Row>`, `// type: Vec<Row>` annotations.
  - End with a "Next" pointer to `book/01-connecting.md`.
  - Reflect CodeResearch gap: explicitly note `Config` is struct-only (no DSN parsing) — direct readers to set fields rather than expecting `Config::from_env()`.
- **`docs/getting-started/.gitkeep`** is unnecessary because `first-query.md` lives there; no other action needed.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] `grep -q 'Ergonomic Postgres for Rust' docs/index.md`
- [ ] `grep -qE '!\[.*\]\(assets/img/' docs/index.md` (at least one image reference into `assets/img/`).
- [ ] `grep -q 'getting-started/first-query' docs/index.md` (CTA link present).
- [ ] `head -40 docs/getting-started/first-query.md | grep -q 'use '` (imports at top).
- [ ] `head -40 docs/getting-started/first-query.md | grep -qE '^```'` (a fenced block exists in the first 40 lines).
- [ ] `grep -qE '// type: ' docs/getting-started/first-query.md` (inline type annotation present).

#### Manual Verification
- [ ] Built landing page renders with hero image visible above the fold on a 1024px-wide viewport (SC-001).
- [ ] Following the CTA from the landing page reaches the quickstart in one click (SC-002 — CTA + 1 click).
- [ ] Voice spot-check: read both pages aloud; matches doobie-style ("In this chapter we'll …", "Let's break this down").

---

## Phase 3: Book Foundations (Chapters 1–4)

**Objective**: Draft the four foundational Book chapters in full doobie-style prose. These four are sequenced because chapters 2–4 each build on chapter 1's connection setup.

### Changes Required

- **`docs/book/01-connecting.md`**: `Config` struct fields, `connect()` returning `Session`, the implicit background driver task (set up the conceptual model — full deep-dive lives in `explanation/driver-task.md`). Source: `crates/core/examples/quickstart.rs` and `crates/core/src/config.rs` / `session.rs` per CodeResearch.
- **`docs/book/02-selecting.md`**: `session.query(&query, args)` returning `Vec<B>` (the codec decoder determines `B`), the `query!` macro for typed queries, the `sql!` macro for parameterless `Fragment<()>` cases. Note explicitly: babar does NOT expose a `Row::get::<T, _>(...)` accessor — decoded values come back as the codec's target type. Source: `crates/core/examples/quickstart.rs`, `crates/core/examples/todo_cli.rs` (its read paths), and `crates/core/src/codec/mod.rs` for the `Decoder<A>` trait shape.
- **`docs/book/03-parameterized-commands.md`**: the `query!` / `command!` macros, parameter binding semantics, the `Encoder<A>` / `Decoder<A>` codec traits at a user level (note the generic-over-A signatures). Source: `crates/core/examples/todo_cli.rs`, `crates/core/src/codec/mod.rs` for the trait definitions (per CodeResearch §4 / §11).
- **`docs/book/04-prepared-and-streaming.md`**: prepared statements, `query_stream` / row streaming. Source: `crates/core/examples/prepared_and_stream.rs`. Note any back-pressure semantics CodeResearch surfaced.

Each chapter conforms to the chapter-shape gate (FR-004 / SC-004): imports + setup at top, fenced code block before any prose paragraph following the setup, ≥1 inline `// type: T` annotation, "Next" pointer at the end. Each chapter is self-contained — readers can land directly on chapter 3 and run the code, even though they are clearly building on chapter 1's setup.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] For each of the four chapter files: presence of `use ` line in first 40 lines, presence of fenced code block before first prose paragraph after setup, presence of ≥1 `// type: ` annotation, presence of a "Next" link to the following chapter.
- [ ] No British-English markers via `grep -InE '(behaviour|colour|organis|optimis|catalogu)' docs/book/0[1-4]*.md` (must produce zero output).

#### Manual Verification
- [ ] Read chapters 1–4 in order. Voice consistency holds; chapter 2 is comprehensible without re-explaining concepts from chapter 1, but chapter 2's setup block stands on its own.
- [ ] Code samples match the public surface in `crates/core/src/lib.rs` (CodeResearch §1) — no fabricated method names.

---

## Phase 4: Book Composition (Chapters 5–8)

**Objective**: Draft the next four chapters covering composition primitives — transactions, pooling, COPY, migrations.

### Changes Required

- **`docs/book/05-transactions.md`**: `Session::transaction()`, `Transaction` / `Savepoint`, commit/rollback. Source: `crates/core/examples/transactions.rs`.
- **`docs/book/06-pooling.md`**: `Pool::builder()`, sizing/timeout knobs, `pool.acquire()` returning a `Session`. Source: `crates/core/examples/pool.rs`. Cite the actual config field names from CodeResearch §7.
- **`docs/book/07-copy.md`**: `CopyIn`, binary `COPY FROM STDIN`. Source: `crates/core/examples/copy_bulk.rs`. Explicit callout that `COPY TO` and text/CSV are not implemented (CodeResearch §8 / Spec scope).
- **`docs/book/08-migrations.md`**: `Migrator` API, the migration directory layout the example uses. Source: `crates/core/examples/migration_cli.rs` and `crates/core/src/migrator.rs` per CodeResearch §9. Note that the CLI is an *example*, not a shipped binary.

Same chapter-shape gate as Phase 3.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] Chapter-shape gate passes for all four files (same checks as Phase 3, scoped to `docs/book/0[5-8]*.md`).
- [ ] Chapter 7 contains an explicit "not yet implemented" or "deferred" reference for `COPY TO` / text/CSV (`grep -qiE '(copy to|text|csv).*(deferred|not.*implemented|roadmap)' docs/book/07-copy.md`).
- [ ] No British-English markers in `docs/book/0[5-8]*.md`.

#### Manual Verification
- [ ] Each chapter's code is sourced from the corresponding example file; spot-check by diffing the chapter's primary code block against the example.
- [ ] Voice and shape consistent with chapters 1–4.

---

## Phase 5: Book Production (Chapters 9–13)

**Objective**: Draft the final five Book chapters covering production concerns.

### Changes Required

- **`docs/book/09-error-handling.md`**: The `Error` enum's 11 variants (per CodeResearch §10), pattern-matching on `Error::Server { code, .. }` for SQLSTATE-based recovery. Be explicit: there is no `Error::kind()` classifier; classification is by inspecting `code` directly. Cross-link to `reference/errors.md`.
- **`docs/book/10-custom-codecs.md`**: `#[derive(Codec)]`, when to write a custom codec, the encode/decode trait shape. Source: `crates/core/examples/derive_codec.rs`. Cross-link to `reference/codecs.md`.
- **`docs/book/11-web-service.md`**: integrating babar with axum — per-request session checkout, error mapping. Source: `crates/core/examples/axum_service.rs`. Note this chapter does NOT add `axum` to `Cargo.toml` since axum is already a dev-dep behind the example.
- **`docs/book/12-tls.md`**: feature-flag-gated TLS (`rustls` default, `native-tls` optional per CodeResearch §14), `Config` TLS toggles, when to disable. Cross-link to `reference/feature-flags.md`.
- **`docs/book/13-observability.md`**: `tracing` instrumentation in `crates/core/src/telemetry.rs`, span and target names (cite specific names from CodeResearch §15), how to wire a subscriber.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] Chapter-shape gate passes for all five files.
- [ ] Chapter 9 contains `Error::Server` and an explicit "no `Error::kind()`" callout (`grep -qE 'Error::Server' docs/book/09-error-handling.md`).
- [ ] Chapter 12 mentions both `rustls` and `native-tls` (`grep -q rustls docs/book/12-tls.md && grep -q native-tls docs/book/12-tls.md`).
- [ ] Chapter 13 references concrete span / target names that exist in `crates/core/src/telemetry.rs` (manual diff).
- [ ] No British-English markers across `docs/book/`.

#### Manual Verification
- [ ] Read chapters 9–13. Each is self-contained and points to the right Reference page.
- [ ] No fabricated APIs (cross-reference each public name against `crates/core/src/lib.rs` re-exports).

---

## Phase 6: Reference Section

**Objective**: Author the four Reference pages as scannable tables / table-per-section, not narrative prose (per FR-005 / SC-005).

### Changes Required

- **`docs/reference/codecs.md`**: One table per codec module under `crates/core/src/codec*` (CodeResearch §11). Columns: Postgres type / OID / Rust type / codec module / notes. Each Reference page also includes a "See also" header line linking to the relevant docs.rs page (FR-005's "pointers to docs.rs"), e.g., `> Generated rustdoc: <https://docs.rs/babar/latest/babar/codec/index.html>`.
- **`docs/reference/errors.md`**: Two parts: (a) a table of every `Error` enum variant (per CodeResearch §10) with shape and a one-line description; (b) an editorial guidance section listing common SQLSTATEs (e.g., `23505` unique violation, `40P01` deadlock, `40001` serialization failure, `42P01` undefined table) with recovery patterns. The editorial section is clearly labeled as guidance, not extracted facts.
- **`docs/reference/feature-flags.md`**: A table of every cargo feature defined in `crates/core/Cargo.toml` and `crates/babar-macros/Cargo.toml` (per CodeResearch §12), with a one-line description per row.
- **`docs/reference/configuration.md`**: A table of every public field on `Config` (per CodeResearch §13), with type, default, and one-line description. Pool config knobs in their own subsection.
- Cross-links: each Reference page links back to the corresponding Book chapter at the top, AND includes a one-line `> Generated rustdoc: <https://docs.rs/babar/...>` pointer (FR-005).

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] Each Reference page contains at least one Markdown table (`grep -qE '^\|.*\|.*\|' docs/reference/*.md` per file).
- [ ] Codec catalog row count ≥ codec module count from CodeResearch §11.
- [ ] Error catalog includes all 11 variants from CodeResearch §10.
- [ ] Feature flags page includes every feature listed in CodeResearch §12.
- [ ] Each Reference page links to docs.rs (`for f in docs/reference/*.md; do grep -q docs.rs "$f" || echo "MISSING: $f"; done` produces no output).

#### Manual Verification
- [ ] Spot-check 5 random codec table rows against `crates/core/src/codec/`. No fabricated types.
- [ ] SQLSTATE editorial section is clearly delimited from the variant table and labeled as guidance.

---

## Phase 7: Explanation / Why Babar

**Objective**: Author the five Explanation pages. This is the phase where comparison claims about other libraries (`tokio-postgres`, `sqlx`, `diesel`) get surfaced for user sign-off per Spec R3 / process notes.

### Changes Required

- **`docs/explanation/why-babar.md`**: Top-level pillar page sourced from `.design/SITE-COPY.md` §2 "Why babar three-up". Links onward to the four sub-pages.
- **`docs/explanation/design-principles.md`**: Typed / async / native protocol / **validate-early / no-unsafe**. Both "validate-early" and "no-unsafe" must appear by name with at least a paragraph of treatment each (this is where FR-006's six-item list folds two items into one page rather than spawning four separate pages). Cite `CLAUDE.md` for the no-unsafe stance and `MILESTONES.md` for ethos.
- **`docs/explanation/comparisons.md`**: vs `tokio-postgres`, `sqlx`, `diesel`. Sourced from `.design/SITE-COPY.md` §2 "Comparison band". **Mid-phase user-confirmation step**: before merging this file, surface each comparison claim as a checklist for user sign-off (per Spec R3). Drop or rephrase any claim the user marks as overreaching.
- **`docs/explanation/driver-task.md`**: The per-connection background task model — what it is, what channel types it uses, why it exists. Cite `crates/core/src/session.rs` / wherever the task lives per CodeResearch §16.
- **`docs/explanation/roadmap.md`**: A pointer-page that links to `MILESTONES.md` and summarizes the milestone structure. Lists the deferred items (SCRAM-SHA-256-PLUS, LISTEN/NOTIFY, COPY TO, etc.) so readers know they are intentional.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] `docs/explanation/comparisons.md` contains references to all three named alternatives (`grep -q tokio-postgres && grep -q sqlx && grep -q diesel`).
- [ ] `docs/explanation/design-principles.md` contains both "validate-early" and "no-unsafe" / "unsafe" treatments (`grep -qiE 'validate.early' docs/explanation/design-principles.md && grep -qiE 'no.unsafe|unsafe' docs/explanation/design-principles.md`) — this is how SC-006's six-item gate is mechanically witnessed when validate-early and no-unsafe are folded into the design-principles page.
- [ ] `docs/explanation/design-principles.md` is at least 50 non-blank lines (`[ "$(grep -cv '^[[:space:]]*$' docs/explanation/design-principles.md)" -ge 50 ]`) — coarse depth gate so the page isn't left as a placeholder (FR-006 "design philosophy").
- [ ] `docs/explanation/why-babar.md` is at least 30 non-blank lines (same shape, applied to the Explanation pillar page).
- [ ] `docs/explanation/roadmap.md` references `MILESTONES.md` (`grep -q MILESTONES docs/explanation/roadmap.md`).
- [ ] No British-English markers across `docs/explanation/`.

#### Manual Verification
- [ ] **User sign-off on every comparison claim** in `comparisons.md` before the phase is marked complete (Spec R3 mitigation).
- [ ] `driver-task.md` matches what's actually in `crates/core/src/session.rs` (or wherever the task lives per CodeResearch §16) — no fabricated channel types.

---

## Phase 8: Polish, Voice Sweep & Documentation

**Objective**: Final pass across all new content. Ensure voice, American English, link integrity, and produce the standard PAW Docs.md plus a CHANGELOG entry.

### Changes Required

- **Voice and American-English sweep** — read every new file from Phases 2–7 and:
  - Eliminate any British-English markers (`-our`, `-ise`, `colour`, `behaviour`, `optimisation`, `cataloguing`, etc.). Use `grep -P` (PCRE) or a plain alternation without lookahead, e.g. `grep -RInE '(behaviour|colour|organise|organising|organisation|optimis|catalogu|ise\b|isation\b)' docs/` and inspect each hit.
  - Run a coarse **chapter-shape gate** (mechanizes the second half of SC-008) for each numbered Book chapter:
    ```sh
    for f in docs/book/*.md; do
      head -80 "$f" | grep -qE '\byou(r)?\b' || echo "NO 2nd-person: $f"
      awk 'BEGIN{infence=0;sawcode=0} /^```/{infence=!infence; if(infence) sawcode=1; next} !infence && /^[A-Za-z]/ && sawcode==0 {print FILENAME": prose-before-code"; exit} ' "$f"
    done
    ```
    Any output indicates a chapter to revisit. This is coarse on purpose — the goal is to catch obvious regressions, not produce a bulletproof gate.
  - Verify each chapter's "Next" pointer resolves correctly and points to the right adjacent chapter.
  - Read three random chapters aloud against the doobie sample to catch drift.
- **Link integrity**:
  - `mdbook build` warnings would surface broken internal links; confirm the build is warning-free.
  - Spot-check by clicking through every SUMMARY entry in the built `book/` site.
- **`docs/tutorials/postgres-api-from-scratch.md` byte-identical check**: `git diff main -- docs/tutorials/postgres-api-from-scratch.md` returns nothing.
- **`.paw/work/mdbook-docs-rewrite/Docs.md`**: standard PAW Docs.md (load `paw-docs-guidance` for template). Captures the new docs hierarchy, asset locations, voice conventions, and verification commands so a future contributor can extend the book consistently.
- **`CHANGELOG.md`**: add an entry under the appropriate section noting the documentation rewrite (no API changes; doc-site-only). One paragraph, includes the link to the published site.

### Success Criteria

#### Automated Verification
- [ ] `mdbook build` exits 0 with no warnings.
- [ ] `git diff main -- docs/tutorials/postgres-api-from-scratch.md` is empty (SC-007).
- [ ] `git diff --stat main -- crates/ '**/Cargo.toml'` is empty (SC-014).
- [ ] `! grep -RInE '(behaviour|colour|organis(?!ation\b)|optimis|catalogu)' docs/` returns no matches (excluding the verbatim tutorial file by path filter if needed) (SC-008).
- [ ] `! git ls-files | grep -i 'zone.identifier'` exits 0 (SC-010, git-tracking gate) AND `! find . -path ./.git -prune -o -name '*Zone.Identifier*' -print | grep .` exits non-zero (filesystem gate).
- [ ] `test -f .paw/work/mdbook-docs-rewrite/Docs.md`.
- [ ] `grep -q 'Book of Babar\|mdbook' CHANGELOG.md` (CHANGELOG entry present).

#### Manual Verification
- [ ] Click through every SUMMARY entry in the built site — every page renders, every internal link resolves, every image loads.
- [ ] Spot-check three random chapters against the chapter-shape gate (second-person pronoun + code-block-before-prose).
- [ ] Voice consistency holds across the full site (read landing → Get Started → 3 random Book chapters → 1 Reference page → 1 Explanation page).

---

## References

- Issue: none
- Spec: `.paw/work/mdbook-docs-rewrite/Spec.md`
- Code research: `.paw/work/mdbook-docs-rewrite/CodeResearch.md`
- Project context: `README.md`, `CLAUDE.md`, `PLAN.md`, `MILESTONES.md`
- Voice/microcopy authority: `docs/SITE-COPY.md` (relocated to `.design/SITE-COPY.md` in Phase 1)
- Visual mockup: `docs/landing-mockup.html` (relocated to `.design/landing-mockup.html` in Phase 1)
- Tutorial (preserved verbatim): `docs/tutorials/postgres-api-from-scratch.md`
- Code-sample sources: `crates/core/examples/*.rs`
- Voice target: typelevel doobie's *Book of Doobie* (https://typelevel.org/doobie/docs/index.html)
- Diataxis framework: https://diataxis.fr/
