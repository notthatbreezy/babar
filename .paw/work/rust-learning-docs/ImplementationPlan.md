# Rust Learning Docs Implementation Plan

## Overview

Implement a new optional Rust-learning track inside the existing `babar` mdBook
so readers can learn core Rust concepts through the real codebase rather than
through unrelated toy programs. The plan preserves the existing product-docs
path as the primary route for readers who already want to use `babar`, while
adding a parallel top-level section that teaches the concept families defined in
the spec through curated `babar` examples, cross-links, and lightweight
checkpoint prompts.

The work is documentation-heavy, but it still needs phased implementation. The
first phase establishes navigation, entry points, naming, and the reusable
teaching framework. The next phases deliver the guided sequence and then the
deeper concept coverage needed to satisfy the nine-plus-page first release.
Documentation as an artifact remains the final phase so the workflow still
produces an as-built `Docs.md` plus final docs-integrity verification.

## Current State Analysis

Today the repo has one mdBook rooted at `docs/` with top-level sections for
Get Started, The Book of Babar, Reference, Explanation, and Tutorial
(`docs/SUMMARY.md`, `book.toml`). The homepage at `docs/index.md` already
describes alternate docs roles, but it does not provide a dedicated Rust-
learning route. The best existing teaching anchors are spread across the
product docs rather than organized as a learning progression: `docs/getting-started/first-query.md`,
`docs/book/01-connecting.md`, `docs/book/02-selecting.md`,
`docs/book/09-error-handling.md`, `docs/book/11-web-service.md`,
`docs/explanation/driver-task.md`, `docs/tutorials/postgres-api-from-scratch.md`,
and `crates/core/examples/quickstart.rs`.

Those files already contain useful examples of structs, `Result`, async/await,
typed SQL, and service-style code, but they assume more Rust comfort than the
new audience has. The repo therefore needs a new docs layer that is neither a
generic Rust textbook nor a rewrite of the existing book. The plan must keep
the learning-track pages anchored in `babar`, use cross-links rather than
duplicating all existing product explanations, and explicitly exclude generic
setup topics such as packaging workflows or crate-publishing material from the
first version.

## Desired End State

The docs site has a new top-level learning section, linked directly from the
homepage, that readers can enter without disturbing the existing product-docs
path. That section contains at least nine new pages and covers the concept
families in the approved spec: syntax/control flow, types/data modeling,
ownership/borrowing, async/await, error handling, traits/generics,
object-oriented features in Rust, and functional features in Rust. Every guided
page uses `babar` code, a `babar` docs example, or a directly derived example
from the codebase, and every guided page includes either a checkpoint block or
one to three reflection prompts.

Implementation success also means the new section is bounded and maintainable:
it does not become the only route into the docs, it does not duplicate existing
product chapters as standalone replacements, and it uses explicit cross-links to
book/explanation/reference pages for deeper follow-up. Final verification
includes mdBook build integrity and a repository-level docs validation command
appropriate for the affected surfaces.

## What We're NOT Doing

- Creating a separate second site or standalone book in this iteration
- Replacing the main product-docs path or making the learning track a
  prerequisite for using `babar`
- Turning the learning track into a generic Rust textbook detached from the
  `babar` codebase
- Adding packaging workflows, crate-publishing guidance, or unrelated toy-app
  chapters to the first release
- Building a full exercise platform, sandbox, grader, or lab environment
- Rewriting existing book, explanation, or reference pages as duplicates when a
  cross-link can carry the reader to the deeper material

## Phase Status
- [x] **Phase 1: Learning Track Information Architecture** - Add the new top-level section, homepage entry point, navigation, and reusable chapter framework for the learning path.
- [x] **Phase 2: Guided Foundations Sequence** - Deliver the opening guided chapters that teach the first-read Rust concepts needed to understand a small `babar` example.
- [x] **Phase 3: Deep Concept Coverage** - Deliver the remaining concept-family pages and integrate them with existing book/explanation/reference anchors.
- [x] **Phase 4: Documentation** - Record the as-built architecture in `Docs.md` and run final docs-integrity verification.

## Phase Candidates
- [ ] Split the learning track into a separate standalone book later if the section becomes large enough to justify separate identity and maintenance
- [ ] Add richer exercises or interactive labs in a later iteration
- [ ] Add a standardized Python-analogy style guide if the first implementation shows repeated comparison patterns across many pages

---

## Phase 1: Learning Track Information Architecture

### Changes Required:
- **`docs/SUMMARY.md`**: Add a new top-level learning section with the initial page list and ordering that supports the approved concept progression.
- **`docs/index.md`**: Add the alternate homepage entry path for the Rust-learning track while preserving direct access to the existing product-docs path.
- **`docs/` (new learning-track directory)**: Create the section landing page plus the initial chapter scaffolding and shared framing for checkpoints/reflection prompts.
- **Existing anchor pages** such as **`docs/getting-started/first-query.md`** and **`docs/book/01-connecting.md`**: Add or adjust cross-links so the new section and existing product docs point at one another coherently.
- **Tests**: Use the mdBook navigation/build surface to confirm the new section is wired correctly and that no broken links or missing pages are introduced at this stage.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Lint/typecheck: `cargo doc --workspace --no-deps`

#### Manual Verification:
- [ ] The homepage presents the learning track and the main product-docs path as separate labeled routes.
- [ ] The new section appears as a top-level part of the existing mdBook rather than being nested ambiguously under Get Started or Tutorials.

---

## Phase 2: Guided Foundations Sequence

### Changes Required:
- **New learning-track pages under the new `docs/` section**: Deliver the opening guided sequence covering syntax/control flow, types/data modeling, ownership/borrowing, and the first async/`Result` reading experience.
- **`docs/getting-started/first-query.md`** and **`crates/core/examples/quickstart.rs`**: Treat them as first-success anchors, updating cross-links or surrounding framing if needed so the learning track can point into them cleanly without rewriting the product page itself into a learner chapter.
- **Checkpoint design within each guided page**: Add either a checkpoint block or one to three reflection prompts tied to the page’s worked `babar` example.
- **Tests**: Validate the new pages build cleanly and that the opening sequence satisfies the spec’s “identify structs / `Result` / async / ownership in an introductory example” requirement.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Tests pass: `cargo test --doc -p babar`

#### Manual Verification:
- [ ] A reader can progress through the opening sequence in order and encounter the first-success `babar` example with enough surrounding explanation to identify structs, `Result`, async/await, and ownership-sensitive values.
- [ ] Every guided page in this phase includes checkpoint or reflection material rather than ending as pure exposition.

---

## Phase 3: Deep Concept Coverage

### Changes Required:
- **Additional learning-track pages under the new section**: Deliver the remaining concept-family coverage so the first version reaches at least nine pages and includes error handling, traits/generics, object-oriented features in Rust, and functional features in Rust.
- **Existing anchor docs** — especially **`docs/book/02-selecting.md`**, **`docs/book/09-error-handling.md`**, **`docs/book/11-web-service.md`**, **`docs/explanation/driver-task.md`**, and **`docs/tutorials/postgres-api-from-scratch.md`**: Add or refine cross-links so deep topics route into the existing product docs instead of being duplicated wholesale.
- **Python comparison notes across the learning pages**: Standardize them as explicitly labeled, Rust-first companion notes rather than the primary explanatory mode.
- **Content audit**: Ensure each guided page is anchored in `babar` code, a `babar` docs example, or a directly derived example, and confirm no page centers on packaging, crate-publishing, or unrelated toy programs.
- **Tests**: Re-run full docs validation once the nine-plus-page scope and all cross-links are in place.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Lint/typecheck: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`

#### Manual Verification:
- [ ] The learning track contains at least nine new pages and collectively covers the concept families defined in the specification.
- [ ] Any Python-oriented comparison is explicitly labeled and paired with a Rust-first explanation in the same section.
- [ ] No learning-track page duplicates an existing product-docs topic as a standalone replacement when a link to the existing page serves the deeper need.

---

## Phase 4: Documentation

### Changes Required:
- **`.paw/work/rust-learning-docs/Docs.md`**: Record the as-built learning-track architecture, navigation decisions, concept-family mapping, cross-link strategy, and validation approach.
- **Project documentation surfaces touched in earlier phases**: Final consistency pass so homepage, new learning-track pages, and linked product docs describe the alternate route coherently.
- **Docs verification**: Run the final repository-level docs commands for mdBook and rustdoc after all learning pages and links are in place.

### Success Criteria:

#### Automated Verification:
- [ ] Docs build: `mdbook build`
- [ ] Lint/typecheck: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`

#### Manual Verification:
- [ ] `Docs.md` explains the final learning-track structure, its relationship to the existing product docs, and the limits of the first release.
- [ ] The published docs experience still makes the learning track feel optional rather than required before using `babar`.

---

## References
- Issue: none
- Spec: `.paw/work/rust-learning-docs/Spec.md`
- Research: none
