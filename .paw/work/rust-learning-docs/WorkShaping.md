# Rust Learning Docs

## Problem Statement

`babar` already has strong product documentation, but it assumes readers are at
least somewhat comfortable reading Rust. There is an opportunity to add an
optional learning path for readers who are new to Rust—especially expert Python
programmers—so they can learn core Rust concepts while reading and using the
real `babar` codebase.

The goal is not to create required onboarding before someone can use `babar`,
and not to turn the docs into a full generic Rust textbook. The goal is to
teach core Rust fundamentals broadly, but always through real `babar` examples,
with occasional Python analogies where they help bridge mental models.

## Intended Reader and Value

### Primary audience

- New Rust users
- Especially readers who are already expert Python programmers
- Secondary audience: readers who already know some Rust but want to understand
  how these concepts show up in `babar`

### Reader value

- Learn basic Rust syntax and mental models through a real codebase
- Understand how types, ownership, `Result`, async/await, and traits/generics
  appear in `babar`
- Build confidence reading the existing `babar` docs and examples
- Use this as an optional alternate path from the homepage, not a prerequisite
  to using the library

## Proposed Shape

### Recommendation

Create a **new top-level section inside the existing mdBook**, linked from the
homepage as an alternate path. Do **not** create a separate second book/site in
the first version.

### Why this shape fits

- Keeps navigation, hosting, and cross-linking simple
- Avoids duplicating product docs into a parallel site
- Makes it easy to cross-link into the existing Book, Explanation, and
  Reference content
- Preserves the option to split the learning track into a separate book later if
  it grows into something clearly distinct

### Audience boundary

The learning track should feel **moderately separate** from the product docs:
an optional learning path with many links into the main docs, but not a second
competing documentation product.

## Curriculum Direction

### Recommended structure

Use a **hybrid structure**:

1. a short guided path for the first successful read-through
2. optional topic deep-dives afterward

### Desired learning progression

The preferred early sequence is:

1. basic Rust syntax
2. types, structs, and enums
3. ownership and borrowing
4. async/await
5. traits and generics

### First success moment

Very early in the track, the reader should be able to read a small `babar`
example and understand:

- what the structs are doing
- what `Result` means
- what `async` / `await` are doing
- where ownership shows up in the example

## Suggested Content Breakdown

### Core functionality

- Add a new top-level learning section to the existing mdBook
- Introduce Rust concepts using real `babar` examples rather than toy programs
- Provide occasional Python analogies where they genuinely aid comprehension
- Cross-link into existing product docs instead of duplicating them
- Include light reflection or checkpoint prompts rather than full exercise sets

### Supporting functionality

- Homepage entry point that frames this as an optional alternate path
- Curriculum framing that makes clear this is “learn Rust through `babar`”
- Cross-links from the learning track into specific book/explanation/reference
  pages once the reader is ready for more depth
- Consistent signal that this is not required reading before using `babar`

## Strong Existing Teaching Anchors in the Codebase

Based on codebase fit research, these are promising anchors:

- `docs/getting-started/first-query.md` — strongest first entry point for
  `Result`, structs, basic syntax, and macro usage
- `docs/book/01-connecting.md` — good bridge into ownership and async concepts
- `docs/book/02-selecting.md` — useful for type shapes and `Query<A, B>`
- `docs/book/09-error-handling.md` — practical `Result` and enum/pattern
  matching material
- `docs/explanation/driver-task.md` — good async/ownership deep-dive once the
  reader is ready
- `docs/book/11-web-service.md` — strong real-world async application example
- `crates/core/examples/quickstart.rs` — concise runnable anchor for the first
  success moment
- `docs/tutorials/postgres-api-from-scratch.md` — useful later-stage guided
  material for deeper application structure

## Edge Cases and Expected Handling

- **Reader wants to use `babar` immediately**: The learning track must remain
  visibly optional and not replace the existing product onboarding path.
- **Reader is new to Rust but not to backend programming**: Use Python analogies
  selectively, but do not flatten Rust into Python terminology.
- **Real `babar` examples become too advanced too quickly**: Start with small,
  curated examples and link to heavier internals only after the reader has basic
  syntax and ownership context.
- **Learning track starts duplicating existing product docs**: Prefer short
  summaries plus cross-links rather than repeating whole chapters.
- **Track grows too broad**: Keep the first version centered on core Rust
  fundamentals that help readers understand `babar`, not on unrelated Rust app
  building.

## Rough Architecture

### Component relationships

- **Homepage** links to the learning track as an alternate reader path
- **Learning section** introduces concepts progressively with curated `babar`
  examples
- **Existing Book** remains the product/task-oriented guide
- **Explanation pages** provide deeper conceptual follow-up once the learner has
  the right vocabulary
- **Reference pages** remain lookup-oriented and should not become learning-path
  chapters

### Information flow

1. Reader discovers the optional learning path from the homepage
2. Reader gets a short guided sequence for Rust basics in `babar` context
3. Reader is cross-linked into existing Book/Explanation content for deeper use
4. Reader can continue into optional topic deep-dives as needed

## Critical Analysis

### Value assessment

This work has strong value because it helps a real adjacent audience—experienced
engineers who are curious about Rust but not yet fluent—without weakening the
main product docs. It also reinforces `babar` as a readable, instructive
codebase without making the core docs carry two jobs at once.

### Build vs modify tradeoff

This should begin as a **modification and extension of the existing mdBook**,
not a new standalone book. The existing docs structure already has clear
Diataxis-like roles and enough stable content to anchor a learning path. A
second book is more justified only if the learning track later becomes large
enough to need its own identity and maintenance rhythm.

## Codebase Fit

- The current mdBook structure already supports another top-level section cleanly
- The repo has several good existing examples that can be repurposed as teaching
  anchors
- The explanation docs are already strong places to point readers once they have
  enough Rust context
- The current content has very little Python-oriented scaffolding, which creates
  room for a distinct learning layer without heavy conflict

## Risks and Gotchas

- The real codebase can become advanced very quickly (macros, async driver task,
  typed SQL generics)
- Too much Python analogy could distort Rust concepts instead of clarifying them
- Too little Python analogy would miss one of the most useful bridges for the
  intended audience
- A broad “learn Rust” ambition could sprawl into content unrelated to `babar`
- The new section could accidentally become a maintenance burden if it duplicates
  product chapters instead of linking strategically

## Open Questions for Spec Stage

- What should the top-level section actually be called in navigation?
- How many pages should the initial guided path include in version one?
- Should the deep-dives live inside the same section, or mostly point into
  existing Book/Explanation pages?
- Which specific Python analogies are helpful enough to standardize as a docs
  pattern?
- Should the first version introduce reflection prompts inline, or at the end of
  each chapter?

## Session Notes

- The user wants this to serve both:
  - people new to Rust learning concepts from this codebase
  - people learning how those concepts are used specifically in `babar`
- The user explicitly wants Python-oriented analogies when appropriate and
  assumes a reader who is an expert Python programmer
- The user prefers a new top-level section inside the existing mdBook rather
  than a separate book/site for now
- The section should be linked from the homepage as an alternate path
- The track should feel optional and should not become required reading before
  using `babar`
- The desired instructional pattern is a guided path first, then optional
  deep-dives
- The first version should include light reflection/checkpoint prompts only, not
  a full exercise system
- The work should not sprawl into a generic Rust textbook or unrelated toy apps
