# Rust Learning Docs

## Overview

This work adds an optional Rust-learning track inside the existing `babar` mdBook for readers who want to learn Rust by reading real `babar` code and docs examples. The feature exists to close the gap between the existing product docs and the needs of readers—especially expert Python programmers—who can already program but need help building a Rust mental model before the product docs feel easy to read.

The shipped result keeps the main product-docs route intact while adding a parallel, clearly labeled learning route. Readers can start from the homepage, choose the optional learning track, follow a guided opening sequence, and then branch back into the book, explanation, reference, and tutorial surfaces as their questions become more product-specific.

## Architecture and Design

### High-Level Architecture

The implementation fits the learning track into the existing docs site rather than creating a second book:

- `docs/SUMMARY.md` adds a new top-level **Rust Learning Track (optional)** section.
- `docs/index.md` presents the learning track as a companion route alongside the existing product-docs path.
- `docs/rust-learning/index.md` is the landing page for the new section.
- `docs/rust-learning/01-...md` through `09-...md` provide the guided curriculum.
- Existing product docs now cross-link back to the most relevant learning chapters where that helps readers bridge into Rust concepts.

The chapter layout is intentionally split into two layers:

1. **Opening guided sequence** — chapters 1-5, read in order, focused on the first concepts a new Rust reader needs to parse `babar` examples: reading a file, syntax/control flow, types/structs/`Result`, ownership/borrowing, and async/await.
2. **Follow-on deepening sequence** — chapters 6-9, covering error handling, traits/generics/codecs, structs/`impl`/Rust-flavored OOP, and iterators/closures/functional style.

This makes the section broad enough to be useful as a first release without turning it into a generic Rust textbook.

### Design Decisions

#### Keep the learning track optional

The homepage and summary treat Rust learning as an alternate entry path, not required onboarding. Readers who already know Rust can still move directly into prerequisites, first-query onboarding, and the main book without touching the new section.

#### Anchor every chapter in `babar`

Each guided page starts from a real `babar` docs page, example, or code path. The learning track teaches Rust through `babar`-shaped code instead of toy applications, and uses cross-links when deeper product detail already exists elsewhere in the site.

#### Use Rust-first explanations with explicitly labeled Python bridges

The intended audience includes expert Python programmers, but the pages teach Rust as Rust. Python comparisons are clearly labeled optional bridges, used only when they help close a real mental-model gap.

#### End pages with lightweight self-checks

The track uses checkpoints and reflection prompts rather than exercises, sandboxes, or a separate practice system. That keeps the section lightweight while still giving readers a way to test whether the concept has landed.

#### Bound the first release

The shipped scope explicitly avoids packaging workflows, crate-publishing guidance, and unrelated toy-app chapters. It also avoids duplicating existing product docs when a cross-link to book, explanation, reference, or tutorial content already serves the deeper need.

### Integration Points

- **Homepage and navigation**: `docs/index.md` and `docs/SUMMARY.md` expose the new route and preserve the direct product-docs route.
- **Learning-track landing page**: `docs/rust-learning/index.md` explains the guided-sequence vs follow-on split and points readers toward existing docs anchors.
- **Existing product docs**: `docs/getting-started/first-query.md`, `docs/book/01-connecting.md`, `docs/book/09-error-handling.md`, `docs/explanation/driver-task.md`, and `docs/tutorials/postgres-api-from-scratch.md` now link to the matching learning chapters.
- **Teaching anchors**: the track repeatedly points back into existing product docs instead of restating all product detail inline.

## User Guide

### Prerequisites

This track is for readers who already know how to program and want to learn Rust in the context of `babar`. It is especially friendly to Python-fluent readers, but it does not require Python knowledge.

### Basic Usage

Start from either of these entry points:

- `docs/index.md` → **Rust learning track**
- `docs/SUMMARY.md` → **Rust Learning Track (optional)**

Recommended reading flow:

1. `docs/rust-learning/index.md`
2. Chapters 1-5 in order
3. Chapters 6-9 as needed
4. Follow cross-links into the main product docs when you want deeper product detail

### Advanced Usage

The track is designed as a companion layer, not a replacement docs set. Readers should branch out into:

- **Get Started** for the first successful round-trip
- **The Book** for task-oriented product usage
- **Explanation** for architecture and design rationale
- **Reference** for lookup-oriented details such as errors and configuration
- **Tutorial** for the end-to-end service walkthrough

## API Reference

### Key Components

- **`docs/index.md`** — homepage entry copy that presents the learning track as optional.
- **`docs/SUMMARY.md`** — top-level nav entry plus ordered chapter list.
- **`docs/rust-learning/index.md`** — section overview, framing rules, and outbound anchor links.
- **`docs/rust-learning/01-reading-a-babar-program.md`** — first-pass reading strategy for a `babar` program.
- **`docs/rust-learning/02-syntax-and-control-flow.md`** — syntax/control-flow reading guide in `babar` context.
- **`docs/rust-learning/03-types-structs-and-results.md`** — typed boundaries, named structs, `Option`, and `Result`.
- **`docs/rust-learning/04-ownership-and-borrowing.md`** — ownership and borrowing around queries and `.await` points.
- **`docs/rust-learning/05-async-await-and-the-driver-task.md`** — async mental model plus `Session`/driver-task framing.
- **`docs/rust-learning/06-error-handling-and-service-boundaries.md`** — `?`, `match`, SQLSTATE, and service-boundary translation.
- **`docs/rust-learning/07-traits-generics-and-codecs.md`** — generics, traits, and codec derivation in `babar`.
- **`docs/rust-learning/08-structs-impls-and-rust-oop.md`** — Rust-flavored OOP through structs, `impl`, and composition.
- **`docs/rust-learning/09-iterators-closures-and-functional-style.md`** — iterator/closure/functional patterns grounded in `babar` usage.

### Configuration Options

There are no runtime configuration changes for this work. The implementation is a documentation information-architecture change inside the existing mdBook:

- one new top-level section
- one new landing page
- nine new guided chapters
- targeted cross-links from existing product docs into the new learning pages

## Testing

### How to Test

Relevant validation for this docs feature is:

- build the mdBook and confirm the new section renders and links resolve
- build workspace rustdoc to confirm the touched documentation surfaces still integrate cleanly with the repository's published docs story
- manually verify that the homepage still offers both the direct product-docs route and the optional learning route
- manually verify that each learning page includes checkpoint/reflection material and stays anchored in `babar`

Commands run for final verification:

- `mdbook build`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`

### Edge Cases

- A reader can skip the learning track entirely; the product-docs path must still work on its own.
- Python bridge material must remain optional and clearly labeled so the track still reads coherently for non-Python users.
- Cross-links must deepen existing docs usage rather than turning the learning track into a duplicate book.
- The opening sequence must stage concepts carefully so real `babar` code does not become too advanced too early.

## Limitations and Future Work

- This is still one section inside the existing mdBook, not a separate Rust-learning site.
- The feature adds guided reading and reflection, not interactive labs or exercises.
- The first release intentionally excludes packaging workflows, crate publishing, and toy-app tutorials unrelated to `babar`.
- The learning track is broad enough for a beginner-to-intermediate first pass, but it is not a complete Rust curriculum.
- Later work could split the section into a standalone book, add richer exercises, or standardize Python-bridge style further if the section keeps growing.
