# Feature Specification: Mdbook Docs Rewrite

**Branch**: feature/mdbook-docs-rewrite  |  **Created**: 2025-11-18  |  **Status**: Draft
**Input Brief**: Reorganize and rewrite the babar mdbook site under `docs/` in a Diataxis-aligned, doobie-style structure with full prose, brand imagery, and a clean build.

## Overview

The babar mdbook site at `docs/` is currently a stub: a 9-line landing page, a 4-line `SUMMARY.md`, and one (excellent, 1162-line) tutorial. Two large authoring artifacts — the 452-line voice/microcopy brief `SITE-COPY.md` and the styled `landing-mockup.html` — sit inside `docs/` and would be served by mdbook if the site were properly fleshed out. Brand imagery lives outside the book at the repo root in `images/` with timestamp-laden filenames and Windows `:Zone.Identifier` sidecars. Readers arriving at https://babar.notthatbreezy.io/ today get almost nothing: no overview of what babar is, no quickstart, no chapter on connecting or selecting, no codec catalog, no error reference, no design rationale.

The goal is to turn the site into a real book — modeled on typelevel doobie's *Book of Doobie* — so a Rust developer who lands on the homepage can grasp what babar is in fifteen seconds, run their first query in five minutes, and read end-to-end through numbered chapters that mirror the shape of a real Postgres-backed application: connecting, selecting, parameterized commands, prepared queries and streaming, transactions, pooling, COPY, migrations, error handling, custom codecs, web services, TLS, and observability. Beyond the narrative book, the site should offer a Reference section (codec catalog, error catalog, feature flags, configuration knobs) and an Explanation section (why babar, comparisons, design principles, roadmap).

The voice across all new content matches doobie: second person, conversational, code-first, with inline `// type: T` annotations and short paragraphs. American English. Each chapter is self-contained with imports and setup at the top. The existing 1162-line "Postgres API from Scratch" tutorial is kept verbatim as the marquee Tutorial-quadrant artifact and slotted into the new hierarchy.

Operationally, the rewrite also cleans up the working tree: authoring artifacts (`SITE-COPY.md`, `landing-mockup.html`) move out of `docs/` into a non-served `.design/` directory at repo root. Brand images move into `docs/assets/img/` with kebab-case names; `:Zone.Identifier` sidecars are dropped. `book.toml` is updated (better title, ensure non-served directories aren't built). `mdbook build` finishes with no warnings.

## Objectives

- Give first-time readers a doobie-quality landing experience: hero with brand imagery, three pillars, a runnable quickstart snippet, and clear primary CTAs.
- Provide a numbered, narrative book that mirrors the lifecycle of a Postgres-backed Rust application, drafted in full prose rather than stubs.
- Provide a Reference section that catalogs every codec, every error variant (SQLSTATE → variant), every feature flag, and every configuration knob, so that integrators have a single page to scan.
- Provide an Explanation section that answers "why babar?" — design principles, comparison vs `tokio-postgres` / `sqlx` / `diesel`, the background-driver-task model, the validate-early principle, the no-unsafe stance, and the milestone roadmap.
- Match the doobie voice consistently: second person, code-first, inline type annotations, short paragraphs, self-contained chapters, slightly playful, never corporate.
- Keep the existing 1162-line tutorial as the Tutorial quadrant, slotted into the new structure unchanged.
- Move authoring artifacts out of the served book; relocate and rename brand imagery into the book.
- Ship a green `mdbook build` with no warnings.

## User Scenarios & Testing

### User Story P1 – First-time evaluator lands and gets oriented in under a minute

**Narrative**: A Rust developer evaluating Postgres drivers opens https://babar.notthatbreezy.io/. Within fifteen seconds they understand what babar is (typed, async, native Postgres wire-protocol driver), see brand imagery that signals personality, and find a primary CTA leading to a quickstart. Within five minutes they have a runnable `SELECT 1` against a local Postgres.

**Independent Test**: Open the built site at the root URL. Confirm the landing page (a) names babar with its tagline, (b) shows at least one brand image, (c) shows three pillars (or equivalent), (d) shows a code snippet that runs against Postgres, and (e) links to a "Get started" page. Then click that link and follow it to a runnable first-query example without leaving the site.

**Acceptance Scenarios**:
1. Given the built mdbook site, When a reader loads `index.html`, Then they see a hero with the babar wordmark/tagline, at least one brand image, three pillars, a code snippet, and a primary CTA to "Get started".
2. Given the landing page, When the reader clicks "Get started", Then they reach a quickstart chapter that takes them from `cargo add babar-core` (or equivalent) to a successful `SELECT 1` in fewer than five minutes of reading.
3. Given the landing page, When the reader scrolls past the hero, Then they encounter navigation entries for Get started, Book/Guides, Reference, and Why babar.

### User Story P2 – Working developer reads the book in order to build a real app

**Narrative**: A developer who has decided to use babar reads the numbered book chapters in order. They are introduced to connecting, selecting, parameterized commands, prepared queries and streaming, transactions, pooling, COPY, migrations, error handling, custom codecs, web services (Axum), TLS, and observability — each chapter self-contained, code-first, with inline type annotations.

**Independent Test**: Read each numbered chapter top-to-bottom. Each must (a) open with imports/setup, (b) lead with a code block before prose, (c) include at least one `// type: T` inline annotation on a Rust expression, (d) be readable on its own without forward references, and (e) end on a "Next" pointer to the following chapter.

**Acceptance Scenarios**:
1. Given the book section, When the reader opens any numbered chapter, Then the chapter begins with a setup block (imports + connection), then a code block, then prose explaining what just happened.
2. Given the book section, When the reader views the table of contents, Then chapters are numbered and follow the lifecycle order specified in `Scope`.
3. Given any chapter, When the reader reaches the end, Then they see a brief "Next" pointer to the following chapter.

### User Story P3 – Integrator looks up a codec, an error variant, or a feature flag

**Narrative**: An integrator already using babar needs a quick reference: which codec handles `JSONB`? What variant does babar return for SQLSTATE `23505`? What does the `tls-rustls` feature flag enable?

**Independent Test**: Open the Reference section. Each reference page (codec catalog, error catalog, feature flags, configuration knobs) must be a scannable table or table-per-section, not narrative prose, and must cover the codecs, errors, flags, and knobs that exist in the codebase today.

**Acceptance Scenarios**:
1. Given the Reference section, When the reader opens the codec catalog, Then they see a table mapping each Postgres type babar supports to its Rust type and the codec module.
2. Given the Reference section, When the reader opens the error catalog, Then they see a table mapping SQLSTATE codes to babar's error variants with a short description per row.
3. Given the Reference section, When the reader opens the feature-flags page, Then they see every cargo feature defined in the codebase with a one-line description.

### User Story P4 – Skeptical reader asks "why babar instead of tokio-postgres or sqlx?"

**Narrative**: A reader familiar with the Rust Postgres ecosystem wants the design rationale: what does babar do differently, and why does that matter?

**Independent Test**: Open the Explanation / Why babar section. It must contain a comparison vs `tokio-postgres`, `sqlx`, and `diesel`; a design-principles page; and a roadmap that references `MILESTONES.md`.

**Acceptance Scenarios**:
1. Given the Explanation section, When the reader opens "Why babar", Then they see at minimum: design principles (typed/async/native protocol/validate-early/no-unsafe), a comparison table or band against named alternatives, and a roadmap pointer.
2. Given the comparison content, When the reader scans it, Then claims about other libraries are factual and citation-friendly (no unearned superlatives).

### Edge Cases

- The tutorial currently at `docs/tutorials/postgres-api-from-scratch.md` is kept verbatim. Internal links into it from the new hierarchy must resolve.
- `docs/SITE-COPY.md` and `docs/landing-mockup.html` must NOT be rendered into the published site. After relocation to `.design/`, they must not be picked up by mdbook.
- The `:Zone.Identifier` Windows sidecar files in `images/` must not appear in `docs/assets/img/`.
- `mdbook test` may or may not be runnable against the rust toolchain in this repo; if it doesn't work cleanly, document why and leave `mdbook build` as the build gate. (Code samples should still typecheck *in spirit* against the public API.)
- The four brand images have whitespace in filenames; renaming must be safe on case-insensitive filesystems and use kebab-case.

## Requirements

### Functional Requirements

- **FR-001**: The site root (`docs/index.md`) renders a doobie-style landing page containing the babar wordmark, the tagline (*Ergonomic Postgres for Rust.*), at least one brand image, three pillars, a runnable code snippet, and a primary "Get started" CTA. (Stories: P1)
- **FR-002**: A "Get started" / first-query quickstart chapter exists that mirrors doobie's chapter 1 in shape — imports + setup at top, then the smallest possible runnable code, then explanatory prose. (Stories: P1)
- **FR-003**: The book / how-to chapters cover, in numbered order: Connecting; Selecting; Parameterized Commands; Prepared Queries & Streaming; Transactions; Pooling; Bulk Loads (COPY); Migrations; Error Handling; Custom Codecs / `derive(Codec)`; Web Service (Axum); TLS & Security; Observability/Tracing. (Stories: P2)
- **FR-004**: Each book chapter is fully prose-drafted (not a stub), opens with imports/setup, leads with a code block, contains at least one inline `// type: T` annotation on a Rust expression where helpful, ends with a "Next" pointer, and is self-contained. (Stories: P2)
- **FR-005**: The Reference section contains: a codec catalog (table per codec module), an error catalog (SQLSTATE → variant table), a feature-flags page, a configuration-knobs page, and pointers to docs.rs for the public API surface. (Stories: P3)
- **FR-006**: The Explanation / Why babar section contains: design philosophy, a comparison vs `tokio-postgres` / `sqlx` / `diesel` (sourced from `SITE-COPY.md`), the background-driver-task model, the validate-early principle, the no-unsafe stance, and the milestone roadmap. (Stories: P4)
- **FR-007**: The existing `docs/tutorials/postgres-api-from-scratch.md` content is preserved verbatim and is reachable via the new `SUMMARY.md`. (Stories: P2, edge cases)
- **FR-008**: All new content is in American English (e.g., "behavior", "color", "initialize"), second person, code-first, with short paragraphs. (Stories: P1, P2, P3, P4)
- **FR-009**: `docs/SITE-COPY.md` and `docs/landing-mockup.html` are moved out of `docs/` to `.design/` at the repo root and are NOT rendered into the published site. (Edge cases)
- **FR-010**: Brand images are moved from `images/` to `docs/assets/img/` with kebab-case filenames; `:Zone.Identifier` sidecar files are deleted; the new `docs/assets/img/` paths are referenced by the landing page and used as section dividers where appropriate. (Edge cases)
- **FR-011**: `book.toml` is updated: the `title` field changes from "babar tutorial" to `The Book of Babar` (per A1); `.design/` and the repo-root `images/` directory are not built into the site (achieved either by relocation or by mdbook configuration). (Edge cases)
- **FR-012**: `docs/SUMMARY.md` reflects the full new hierarchy: landing → Get started → Book → Reference → Explanation → Tutorial. (Stories: P1, P2, P3, P4)
- **FR-013**: `mdbook build` succeeds with zero warnings against the new docs tree. (Edge cases)
- **FR-014**: No files under `crates/` are modified; no `Cargo.toml` is modified. The change is documentation-only. (Scope)
- **FR-015**: Code samples in chapters are sourced from or adapted from `crates/core/examples/*.rs` (e.g., `quickstart.rs`, `pool.rs`, `transactions.rs`, `prepared_and_stream.rs`, `copy_bulk.rs`, `derive_codec.rs`, `migration_cli.rs`, `axum_service.rs`). Samples are expected to be consistent with the public API in `crates/core/src/` but are NOT gated on `mdbook test` or `cargo check` — fidelity is verified by sourcing from working examples, not by a build step (see A5). (Stories: P2)

### Cross-Cutting / Non-Functional

- **NFR-001**: The published site contains no broken internal links. (Verified by `mdbook build` not warning, plus manual spot-checks.)
- **NFR-002**: All new content is reviewable as plain Markdown — no custom mdbook preprocessor is required, unless one is configured deliberately and documented.
- **NFR-003**: Voice is consistent across new chapters per the doobie target described in `Overview` and `Objectives`. Verified subjectively at impl-review and final-review, not at build time; see SC-008 for the mechanical chapter-shape gate.

## Success Criteria

- **SC-001**: A reader who lands on the site root sees the babar wordmark, tagline, at least one brand image, three pillars, a code snippet, and a "Get started" CTA — all visible without scrolling on a typical desktop viewport (≥1024px wide). *(FR-001)*
- **SC-002**: From the landing page, a reader reaches a runnable `SELECT 1`-class first-query example in two clicks or fewer. *(FR-002)*
- **SC-003**: The book TOC contains numbered chapters covering the thirteen topics enumerated in FR-003, in that order. *(FR-003)*
- **SC-004**: Every numbered book chapter passes the chapter-shape checklist: opens with imports/setup, leads with a code block, includes at least one inline `// type: T` annotation, ends with a "Next" pointer. *(FR-004)*
- **SC-005**: The Reference section contains four pages (codec catalog, error catalog, feature flags, configuration knobs) and each is a scannable table or table-per-section rather than long prose. *(FR-005)*
- **SC-006**: The Explanation section contains the six items enumerated in FR-006. *(FR-006)*
- **SC-007**: The 1162-line tutorial is byte-identical to the pre-rewrite version (verified via `git diff`). *(FR-007)*
- **SC-008**: A scan finds no British-English markers (`-our`, `-ise`, `colour`, `behaviour`, etc.) in new content, AND every numbered book chapter's opening passes a mechanical shape check: at least one second-person pronoun ("you" / "your") and at least one fenced code block before the first prose paragraph that follows the imports/setup block. *(FR-004, FR-008)*
- **SC-009**: After the rewrite, `git ls-files docs/` does not include `SITE-COPY.md` or `landing-mockup.html`; both exist under `.design/`. *(FR-009)*
- **SC-010**: After the rewrite, `images/` no longer exists at repo root (or contains only files explicitly designated as non-book assets); `docs/assets/img/` contains the four brand images with kebab-case names; no `:Zone.Identifier` files exist anywhere in the repo. *(FR-010)*
- **SC-011**: `book.toml` `title` is `The Book of Babar`; `.design/` is not in `docs/` and so is not built. *(FR-011)*
- **SC-012**: `docs/SUMMARY.md` matches the four-section hierarchy. *(FR-012)*
- **SC-013**: `mdbook build` exits 0 with zero warnings on stderr. *(FR-013)*
- **SC-014**: `git diff main -- crates/ Cargo.toml` is empty. *(FR-014)*

## Assumptions

- **A1 — Site title**: `book.toml` `title` becomes `The Book of Babar` (confirmed by user at the spec milestone pause). The lowercase wordmark `babar` continues to appear as the brand mark in the landing hero, navigation, and footer per `SITE-COPY.md`; only the `book.toml` `title` field uses the long form.
- **A2 — Tagline**: The hero tagline on the landing page is *Ergonomic Postgres for Rust.* with optional sub-tagline *Typed, async, no surprises.*, taken directly from `docs/SITE-COPY.md` §1.
- **A3 — Brand image filenames** (authoritative mapping confirmed by maintainer during Phase 1):
  - `ChatGPT Image Apr 26, 2026, 10_48_17 PM.png` → `babar-extensions.png` — Postgres extensions showcase.
  - `ChatGPT Image Apr 26, 2026, 10_48_27 PM.png` → `babar-brand-sheet.png` — master brand sheet (used as landing-page hero).
  - `ChatGPT Image Apr 26, 2026, 10_48_32 PM.png` → `babar-scenes.png` — seven-panel scene grid.
  - `ChatGPT Image Apr 26, 2026, 10_48_23 PM.png` → `babar-collage-alt.png` — alternate brand collage.

  All four target names are unique in characters beyond case alone, so the rename is safe on case-insensitive filesystems. Destination directory: `docs/assets/img/` (matches `book.toml` `src = "docs"`).
- **A4 — Tutorial verbatim**: The existing 1162-line `postgres-api-from-scratch.md` is slotted in unchanged. If a small fix is unavoidable (e.g., a now-broken relative link), it is called out explicitly in the implementation phase rather than silently edited.
- **A5 — Code-sample fidelity**: Samples are adapted from `crates/core/examples/*` and are expected to typecheck against the public API as documented in `crates/core/src/`, but are not gated on `mdbook test` succeeding; if `mdbook test` doesn't work against this repo's toolchain, that's acceptable.
- **A6 — Diataxis as compass**: The four-quadrant model informs the hierarchy (Tutorial / How-to / Reference / Explanation) but the chapter-by-chapter book is a hybrid of How-to + Explanation that mirrors doobie, not a strict Diataxis template.
- **A7 — `.design/` location**: Authoring artifacts move to `.design/` at the repo root. The `.` prefix signals "internal/non-shipping". `.design/` is tracked in git (so the assets aren't lost) but is not served by mdbook because it's outside `src = "docs"`.
- **A8 — No new mdbook preprocessors**: The rewrite uses only stock mdbook features. If a preprocessor would clearly help (e.g., `mdbook-toc`), it's noted during planning, not silently added.
- **A9 — Voice inheritance**: For voice/microcopy on the landing and headers, `docs/SITE-COPY.md` is the authoritative source; new chapter prose is original but consistent with that voice.

## Scope

### In Scope

- Authoring `docs/index.md` (landing), a "Get started" quickstart chapter, and ~13 numbered book chapters in full prose.
- Authoring four Reference pages (codec catalog, error catalog, feature flags, configuration knobs).
- Authoring four-to-six Explanation pages (why babar, design principles, comparison, background-driver-task model, no-unsafe / validate-early, roadmap pointing at `MILESTONES.md`).
- Updating `docs/SUMMARY.md` to the new hierarchy.
- Updating `book.toml`.
- Moving `docs/SITE-COPY.md` and `docs/landing-mockup.html` to `.design/` (with a small `.design/README.md` orienting future contributors).
- Renaming and relocating the four brand PNGs into `docs/assets/img/`; deleting `:Zone.Identifier` sidecars.
- Running `mdbook build` to a green, warning-free result.

### Out of Scope

- Any modification to crates under `crates/` or any `Cargo.toml`.
- Editing the existing 1162-line tutorial's prose (it is preserved verbatim per A4).
- Adding or configuring an mdbook preprocessor unless the planning stage explicitly justifies one.
- Setting up CI for `mdbook build` (this rewrite assumes existing CI, if any, continues to work).
- Generating rustdoc or wiring it into the site (the Reference section *links* to docs.rs; it does not embed rustdoc).
- Translation / non-English versions.
- Visual design changes beyond using the existing mdbook `navy` theme and the already-chosen brand imagery.

## Dependencies

- `mdbook` (existing toolchain, version per CI).
- The four brand PNGs in `images/`.
- `docs/SITE-COPY.md` as voice/microcopy authority.
- `crates/core/examples/*.rs` as code-sample source of truth.
- `crates/core/src/` as the public API reference.
- `MILESTONES.md`, `PLAN.md`, `README.md`, `CLAUDE.md` as project context.

## Risks & Mitigations

- **R1 — Voice drift across chapters**: Many chapters drafted in sequence may drift from the doobie target. *Impact*: Inconsistent reading experience. *Mitigation*: A short "voice checklist" in the implementation plan; spot-review against doobie sample in the impl-review and final-review stages.
- **R2 — Code samples that don't typecheck**: Samples adapted from examples may diverge from the actual public API. *Impact*: Reader confusion, broken examples. *Mitigation*: Source samples directly from `crates/core/examples/*.rs` rather than inventing; cross-check against `crates/core/src/`; flag any inferred APIs explicitly.
- **R3 — Comparison claims about other libraries become inaccurate**: `SITE-COPY.md` may have comparison content that's stale or unfair. *Impact*: Reputational risk; reader distrust. *Mitigation*: During the Why-babar implementation phase, surface each comparison claim as a checkpoint and ask the user to confirm or drop, per the user's process note.
- **R4 — `mdbook build` warnings on relocated tutorial links**: Moving images and reorganizing pages can break relative links inside the existing tutorial. *Impact*: Build warnings, broken images. *Mitigation*: Do the asset move and `book.toml` update in an early phase, then rebuild; resolve link issues in that phase before drafting new prose. If the existing tutorial has a now-broken link, flag it under A4 rather than silently editing.
- **R5 — `:Zone.Identifier` files leaking into the repo**: Easy to forget, will not break the build but pollutes the tree. *Impact*: Cosmetic / hygiene. *Mitigation*: Explicit deletion step in the asset-move phase; verify with `git ls-files | grep Zone.Identifier`.
- **R6 — Scope creep on the Reference section**: The codec/error catalogs could expand indefinitely. *Impact*: Schedule risk. *Mitigation*: Constrain Reference content to what is verifiably present in `crates/core/src/` today; defer "would-be-nice" entries to a follow-up.

## References

- Issue: none
- Project context: `README.md`, `CLAUDE.md`, `PLAN.md`, `MILESTONES.md`
- Voice/microcopy authority: `docs/SITE-COPY.md` (to be relocated)
- Visual mockup: `docs/landing-mockup.html` (to be relocated)
- Tutorial (preserved verbatim): `docs/tutorials/postgres-api-from-scratch.md`
- Code-sample sources: `crates/core/examples/*.rs`
- Voice target: typelevel doobie's *Book of Doobie* (https://typelevel.org/doobie/docs/index.html)
- Diataxis framework: https://diataxis.fr/
- Research: not required (no SpecResearch.md generated)
