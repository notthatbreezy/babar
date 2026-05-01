# Feature Specification: Rust Learning Docs

**Branch**: feature/rust-learning-docs  |  **Created**: 2026-04-30  |  **Status**: Revised Draft
**Input Brief**: Add an optional learning-Rust documentation track that teaches core Rust concepts through the `babar` codebase.

## Overview

`babar` already has strong product documentation for readers who are ready to
use the library, but it leaves a gap for readers who are still learning Rust
itself. This work adds an optional learning path inside the existing docs site
for people who want to understand Rust concepts through real `babar` examples
rather than through unrelated toy programs or a generic Rust textbook.

The intended reader is primarily someone who is new to Rust, with a special
emphasis on readers who are already expert Python programmers. They should be
able to follow a guided path that introduces syntax, types, ownership,
async/await, generics, traits, object-oriented features in Rust, and functional
features in a way that helps them read and understand `babar` code. The path
may use selected Python analogies where those comparisons close a real concept
gap, but it should still teach Rust as Rust rather than translating every
concept into Python terms.

This learning path should remain clearly optional. A reader who already knows
Rust or who simply wants to use `babar` immediately should still be able to
take the current product-docs path without friction. The new material should
therefore appear as an alternate route from the homepage rather than as a
required precondition for the main docs.

The first version should launch with at least nine new pages and cover the
defined concept families in this specification, while still staying rooted in
the `babar` codebase and relying on cross-links instead of duplicating every
existing product explanation. It should explicitly avoid spending that space on
generic project-setup material such as packaging workflows, crate-publishing
material, or unrelated toy programs like a guessing-game tutorial.

## Objectives

- Provide an optional top-level learning path for readers who want to learn Rust
  through the `babar` codebase.
- Teach enough core Rust fundamentals that a new Rust reader can make sense of
  real `babar` examples and docs without leaving the project for a separate
  general Rust course.
- Support expert Python programmers with explicitly labeled comparison notes that
  are paired with direct explanations of the Rust concept on the same page.
- Preserve the existing product-docs path as the primary route for readers who
  are already ready to use `babar`.
- Deliver a first version with at least nine new pages that takes a reader from
  basic syntax into beginner-to-intermediate Rust concepts, including
  object-oriented and functional language features.
- Use the real codebase as the teaching context instead of unrelated toy
  applications.

## User Scenarios & Testing

### User Story P1 – Learn enough Rust to read `babar` examples
Narrative: A reader who is new to Rust wants a guided path that helps them read
real `babar` examples without first leaving the project to study a separate
generic Rust course.
Independent Test: A new Rust reader follows the learning path and can explain a
small `babar` example in terms of structs, `Result`, ownership, and
`async`/`await`.
Acceptance Scenarios:
1. Given a reader who is new to Rust, When they start the learning path from the
   docs homepage, Then they encounter a staged introduction to Rust concepts
   using `babar` examples.
2. Given a reader early in the learning path, When they reach an initial worked
   example, Then they can identify what the Rust syntax and core concepts are
   doing in that example.

### User Story P1 – Keep the learning track optional
Narrative: A reader who already knows Rust or who wants to use `babar`
immediately should still be able to use the product docs without being routed
through learning material they do not need.
Independent Test: A reader can choose either the learning path or the existing
product-docs path from the docs homepage, and the product-docs path remains
usable on its own.
Acceptance Scenarios:
1. Given a reader arriving at the docs homepage, When they scan the available
   paths, Then the learning material is presented as an alternate route rather
   than a prerequisite.
2. Given a reader who ignores the learning track, When they use the current
   onboarding and book content, Then they can still proceed without confusion or
   missing steps.

### User Story P2 – Learn Rust through a real codebase instead of toy apps
Narrative: A reader wants examples that feel grounded in real software rather
than disconnected exercises, so they can understand both Rust concepts and how
those concepts show up in `babar`.
Independent Test: Review the guided pages and confirm that each one teaches at
least one concept through `babar` code, a `babar` docs example, or a directly
derived example from the codebase, and that no guided page centers on an
unrelated toy program.
Acceptance Scenarios:
1. Given a reader progressing through the learning track, When they encounter
   new concepts, Then those concepts are explained through real `babar` code or
   closely related examples from this codebase.
2. Given a reader ready for more depth, When they move beyond the guided
   material, Then they can follow cross-links into the existing book,
   explanation, and reference docs for related details.

### User Story P2 – Support Python-fluent readers without turning Rust into Python
Narrative: A reader who is expert in Python wants occasional analogies that help
them bridge mental models, but still wants to learn the actual Rust concepts
and tradeoffs.
Independent Test: Review the learning track and confirm that any Python-oriented
comparison is paired with a Rust-first explanation in the same section and that
the overall track remains understandable without treating Python terminology as
the primary explanation.
Acceptance Scenarios:
1. Given a Python-expert reader encountering an unfamiliar Rust concept, When a
   Python analogy would genuinely aid understanding, Then the docs may provide a
   comparison that clarifies the concept.
2. Given any reader using the learning track, When they read the material as a
   whole, Then the docs still teach the Rust concept directly rather than
   reducing it to Python-only framing.

### User Story P2 – Offer a broad first version, not just a small appendix
Narrative: A reader who commits to the learning path should find enough material
in the first version to make meaningful progress from beginner into
intermediate-level understanding.
Independent Test: The first version presents at least nine new pages and covers
the defined progression from syntax through types, ownership, async,
traits/generics, object-oriented features, and functional features.
Acceptance Scenarios:
1. Given a reader who starts the learning path, When they browse its contents,
   Then they find at least nine new pages of learning material rather than only
   a few short introductory notes.
2. Given the defined concept progression, When the reader follows the initial
   sequence, Then the path covers syntax, types, ownership, async, and
   traits/generics in a coherent order.
3. Given the first version as a whole, When the reader reviews its concept
   coverage, Then it also includes object-oriented and functional language
   features taught through `babar` examples.

### Edge Cases

- A reader may want to use `babar` immediately and skip the learning material
  entirely.
- Real `babar` code can become too advanced too quickly for beginners if the
  learning path does not carefully stage exposure.
- Python analogies may help some readers but may confuse others if overused or
  stretched too far.
- A broad first version may sprawl if it starts reproducing the entire existing
  product docs instead of linking strategically.
- Some deeper internal topics may be better introduced through cross-links than
  by expanding the guided learning path itself.
- A reader may expect generic Rust-book topics such as packaging or
  crate-publishing guidance that this track intentionally does not cover.

## Requirements

### Functional Requirements

- FR-001: The docs site must provide a distinct top-level learning path for
  learning Rust through the `babar` codebase. (Stories: P1)
- FR-002: The learning path must be presented as an optional alternate route
  from the main docs entry experience rather than as required onboarding.
  (Stories: P1)
- FR-003: The learning path must guide a new Rust reader through an ordered
  progression that includes syntax and control flow, types and data modeling,
  ownership and borrowing, async/await, error handling, traits and generics,
  object-oriented features in Rust, and functional features in Rust. (Stories:
  P1,P2)
- FR-004: The learning path must use examples and explanations grounded in the
  `babar` codebase rather than relying on unrelated toy applications as primary
  chapter themes. Each guided page must teach at least one concept through
  `babar` code, a `babar` docs example, or a directly derived example from this
  codebase. (Stories: P2)
- FR-005: The learning path must include cross-links into the existing product
  docs so readers can move from guided learning into task-oriented, explanatory,
  and reference material. (Stories: P2)
- FR-006: Python-oriented analogies are optional, but when present they must be
  clearly framed as comparisons and paired with a Rust-first explanation in the
  same section. (Stories: P2)
- FR-007: The first version must launch with at least nine new pages of learning
  material. (Stories: P2)
- FR-008: Each guided page in the learning track must include either a checkpoint
  block or one to three reflection prompts, without requiring a full exercise
  system. (Stories: P1,P2)
- FR-009: The homepage and docs navigation must continue to expose the existing
  product-docs path directly, with the learning path presented as a separate
  labeled option rather than as an intermediate required step. (Stories: P1)

### Key Entities

- Learning Path: The optional Rust-learning route inside the docs site that uses
  `babar` as the teaching context.
- Product Docs Path: The existing onboarding, book, explanation, and reference
  flow for readers focused on using `babar`.
- Guided Sequence: The ordered early portion of the learning path that teaches
  foundational concepts in a staged way.
- Topic Deep-Dive: Follow-on material or linked destinations that help readers
  explore specific concepts in more depth after the initial guided sequence.
- Python Analogy: A comparison or explanation that bridges a Rust concept to a
  concept familiar to expert Python programmers.

### Cross-Cutting / Non-Functional

- The learning path should remain optional and should not be the only route from
  the homepage into the main product docs.
- The learning track must not include standalone chapters centered on generic
  Rust topics without `babar` examples, `babar` docs examples, directly derived
  examples, or direct links into existing `babar` docs.
- When a topic is already covered in the existing product docs, the learning
  track must link to the existing page and must not duplicate that topic as a
  standalone chapter in the learning path.
- The track should cover the concept families enumerated in FR-003 and should
  exclude packaging workflows, crate-publishing guidance, and unrelated
  toy-program chapters from version one.

## Success Criteria

- SC-001: The opening guided sequence includes checkpoint material for an
  introductory `babar` example, and the expected answers identify the struct
  definitions, the `Result`-returning boundary, the `async`/`await` call sites,
  and the ownership-sensitive values or borrows in that example. (FR-001,
  FR-003, FR-004, FR-008)
- SC-002: The docs homepage presents separate labeled entry paths for the
  learning track and the main product docs, and the product-docs path is
  reachable in one step from the homepage without first navigating through the
  learning track. (FR-001, FR-002, FR-009)
- SC-003: The first version contains at least nine new learning-track pages and
  those pages collectively cover syntax and control flow, types and data
  modeling, ownership and borrowing, async/await, error handling, traits and
  generics, object-oriented features in Rust, and functional features in Rust.
  (FR-003, FR-007)
- SC-004: Every guided page teaches at least one concept through `babar` code, a
  `babar` docs example, or a directly derived example from the codebase, and no
  guided page centers on an unrelated toy program. (FR-004, FR-005)
- SC-005: Any Python-oriented comparison in the learning track appears alongside
  a Rust-first explanation in the same section, and the track contains no page
  whose primary explanation depends on Python terminology. (FR-006)
- SC-006: Every guided page includes either a checkpoint block or one to three
  reflection prompts, and the learning track does not require a full exercise
  system. (FR-008)

## Assumptions

- The first version will stay inside the existing docs site rather than launch
  as a separate second book or standalone site.
- A first release with at least nine new pages is acceptable as long as it
  remains scoped to the concept families named in FR-003 and taught through
  `babar`.
- The top-level navigation name and exact chapter titles can be finalized later
  without changing the core product intent of this specification.
- Cross-linking into existing book, explanation, and reference content is
  preferable to duplicating those materials wholesale.

## Scope

In Scope:
- A new top-level Rust-learning path inside the existing docs site
- Homepage positioning for that path as an optional alternate route
- A first version with at least nine new pages covering beginner-to-intermediate
  Rust concepts through `babar`
- Python-oriented analogy patterns where they close a real concept gap for the
  intended audience
- Guided learning content plus supporting cross-links into existing docs
- Light reflection or checkpoint prompts

Out of Scope:
- Replacing the main product-docs path with the learning track
- Requiring the learning path before readers can use `babar`
- Launching a separate second docs site or standalone book in version one
- Building a full exercise platform, automated grader, or tutorial sandbox
- Turning the docs into a generic Rust textbook detached from `babar`
- Covering packaging workflows, crate-publishing guidance, or unrelated toy
  programs as standalone teaching chapters in version one

## Dependencies

- The existing mdBook docs site and homepage entry experience
- Existing product docs that the learning path will cross-link into
- Existing `babar` examples and docs surfaces suitable for use as teaching
  anchors

## Risks & Mitigations

- Learning content may become too broad and hard to maintain: Mitigation: keep
  the path centered on Rust fundamentals that help readers understand `babar`,
  require each guided page to stay anchored in `babar`, and rely on cross-links
  instead of duplicating every deeper topic.
- Real `babar` examples may become too advanced too early: Mitigation: stage
  the guided sequence carefully and introduce deeper internals only after core
  concepts are established.
- Python analogies may distort Rust concepts if overused: Mitigation: use them
  selectively as bridges, not as the primary explanatory mode.
- The optional learning path may accidentally feel mandatory: Mitigation:
  preserve a clearly visible main product-docs path and describe the learning
  route as an alternate entry point.
- A broad first version may lose coherence: Mitigation: keep a clear ordered
  progression, require explicit concept-family coverage, and separate the guided
  core from deeper follow-on material.

## References

- Issue: none
- Research: none
