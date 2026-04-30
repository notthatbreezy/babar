# Feature Specification: Minor API Improvements

**Branch**: feature/minor-api-improvements  |  **Created**: 2026-04-30  |  **Status**: Revised Draft
**Input Brief**: Batch a small set of accepted API and documentation improvements into one cohesive cleanup wave.

## Overview

This work improves how `babar` feels to a new reader without expanding the SQL
feature set again. The goal is to simplify a few remaining API rough edges,
remove transitional naming that no longer adds value in a greenfield project,
and make the main documentation path read more like a stable product description
than a running migration story.

The bundled scope is intentional. The accepted API cleanup items and the
accepted documentation items all affect the same reader journey: how someone
discovers the primary typed SQL surface, how they understand the raw fallbacks,
what example style they copy into their own code, and where they go for deeper
architectural understanding. Treating them as one improvement wave keeps the
public story coherent.

The documentation portion of the wave is not just copy editing. It should leave
readers with a clearer sense of where to learn by doing, where to look up facts,
where to understand design tradeoffs, and where to find a technical explanation
of the macro pipeline. The outcome should be a surface that is simpler to learn,
less cluttered by transitional language, and more useful to both ordinary users
and deeply technical readers.

## Objectives

- Make the raw fallback constructors easier to understand by separating the
  zero-argument case from the explicit-codec case.
- Remove transitional typed-SQL compatibility names from the public and
  product-facing experience.
- Establish field-named struct examples as the normal style after the earliest
  introductory material.
- Rework targeted documentation so it reads as product documentation rather than
  as an internal milestone log or migration narrative.
- Clarify documentation roles using Diataxis framing so readers can more easily
  tell where to learn, where to look up, where to understand, and where to go
  for advanced technical explanation.
- Add one dedicated technical explanation of the macro pipeline for advanced
  Rust readers.

## User Scenarios & Testing

### User Story P1 – Use a natural raw fallback for statements with no parameters
Narrative: A babar user who needs a raw fallback for setup SQL or another
statement with no bind parameters should be able to construct that statement
without placeholder boilerplate.
Independent Test: Construct and execute a no-parameter raw statement without
providing an empty parameter codec placeholder, and separately construct a
parameterized raw statement through the explicit-codec path.
Acceptance Scenarios:
1. Given a raw statement with no bind parameters, When a user constructs it,
   Then the no-argument path reads as a dedicated zero-argument constructor.
2. Given a raw statement with bind parameters, When a user constructs it, Then a
   separate explicit-codec constructor remains available.

### User Story P1 – Learn one primary typed SQL surface
Narrative: A new babar user should not need to learn extra compatibility names
to understand the primary typed SQL story.
Independent Test: Review the public API, generated schema-scoped wrappers, and
the main docs/examples path and confirm they teach one primary typed SQL naming
scheme rather than a mix of primary and transitional names.
Acceptance Scenarios:
1. Given a user reading the public API and examples, When they identify the main
   typed SQL path, Then they encounter one primary naming scheme instead of a
   compatibility-alias story.
2. Given a user working inside schema-authored modules, When they use the
   generated local wrappers, Then those wrappers follow the same primary naming
   scheme.

### User Story P2 – See field-named examples soon after the first introduction
Narrative: Once a reader moves beyond the earliest introductory examples, the
docs should model the style they are most likely to keep using in real code:
named structs rather than tuple-only examples.
Independent Test: Review the main onboarding path and confirm the earliest
introductory material may still use tuples for brevity, but subsequent major
examples favor field-named struct-shaped params or rows.
Acceptance Scenarios:
1. Given the first introductory material, When a reader is still learning the
   core shapes, Then tuple examples may still be used where they clearly reduce
   ceremony.
2. Given later getting-started, book, and example content, When a reader copies
   a representative example, Then that example usually uses field-named
   struct-shaped values when more than one field is involved.

### User Story P2 – Read explanation docs that are grounded and clearly scoped
Narrative: A reader should be able to move through the documentation without
encountering milestone jargon, transition phrasing, or explanation pages that
sound promotional instead of informative.
Independent Test: Review the targeted documentation surfaces and confirm they do
not use milestone labels, do not describe the product as a transition from an
earlier public state, and present explanation pages as focused answers to
design-oriented questions.
Acceptance Scenarios:
1. Given a reader browsing explanation material, When they read the targeted
   pages, Then the pages explain design choices and tradeoffs without milestone
   jargon or self-congratulatory framing.
2. Given a reader moving between onboarding, book, explanation, and reference
   surfaces, When they follow the documented paths, Then the role of each
   surface is clearer under Diataxis-style boundaries.

### User Story P2 – Find a technical macro explanation in one place
Narrative: An advanced Rust reader should be able to find one dedicated
explanation of how babar's macros work end to end without piecing the story
together from scattered pages.
Independent Test: Locate the dedicated technical macro explanation and confirm it
covers macro entrypoints, schema-generated wrappers, parsing, validation,
lowering, diagnostics, verification, and runtime statement shapes at an
explanatory level.
Acceptance Scenarios:
1. Given an advanced reader looking for macro internals, When they search the
   docs, Then they can find one dedicated technical explanation page.
2. Given that page, When the reader works through it, Then they can understand
   the macro pipeline and its boundaries without relying on source spelunking as
   their first step.

### Edge Cases

- A raw statement may return rows even when it binds no parameters.
- Some documentation surfaces may still need tuple examples in the earliest
  introduction, but later material should not fall back to tuples by default.
- Transitional typed-SQL compatibility names may still exist in generated
  wrappers, docs, examples, tests, or user guidance and must be removed
  consistently.
- A documentation tone pass must not remove useful technical context while
  cleaning up phrasing.
- A technical macro explanation must remain explanatory rather than turning into
  an unstructured reference dump.

## Requirements

### Functional Requirements

- FR-001: The raw fallback API must provide one dedicated constructor for raw
  statements with no bind parameters and one distinct constructor for raw
  statements that still require explicit codec input. (Stories: P1)
- FR-002: The public typed SQL surface must expose one primary naming scheme for
  typed query and command entrypoints, without preserving transitional
  compatibility names as recommended user-facing surfaces. (Stories: P1)
- FR-003: Schema-generated local wrappers must follow the same primary naming
  scheme as the public typed SQL surface. (Stories: P1)
- FR-004: The main onboarding and book/example path must keep tuple-shaped
  examples only where they are part of the earliest introductory material and
  otherwise favor field-named struct-shaped params or rows. (Stories: P2)
- FR-005: Targeted documentation surfaces must avoid milestone labels and must
  avoid describing the product as a transition from a previous public state.
  (Stories: P2)
- FR-006: The documentation revision must define the role of the affected docs
  surfaces using Diataxis framing, so the targeted onboarding, book,
  explanation, and reference surfaces each present a distinct learning, task,
  explanatory, or lookup purpose rather than overlapping ambiguously. (Stories: P2)
- FR-007: The targeted explanation pages must focus on concrete design questions
  and tradeoffs rather than promotional claims about the product. (Stories: P2)
- FR-008: The docs set must include one dedicated technical explanation of the
  macro pipeline for advanced readers. That explanation must cover macro
  entrypoints, generated wrappers, parsing/validation flow, diagnostics,
  verification, lowering, and runtime statement shapes. (Stories: P2)
- FR-009: The work must update the affected public/product-facing surfaces as one
  coordinated bundle: raw fallback naming, primary typed SQL naming, generated
  wrappers, main onboarding/book/example content, targeted explanation pages,
  and the new technical macro explanation. (Stories: P1,P2)

### Key Entities

- Raw No-Argument Constructor: The raw statement entrypoint used when a command
  or query binds no parameters.
- Raw Explicit-Codec Constructor: The raw statement entrypoint used when the
  caller still needs explicit codec input.
- Primary Typed SQL Surface: The typed query/command entrypoints and generated
  wrappers a new user is expected to learn first.
- Introductory Material: The earliest onboarding content where reduced ceremony
  may still justify tuple-shaped examples.
- Targeted Documentation Surfaces: The library overview/onboarding path, the
  main book chapters that demonstrate querying and commands, the explanation
  pages that describe design/positioning, and the new technical macro
  explanation page.

### Cross-Cutting / Non-Functional

- The improvement wave must reduce conceptual clutter instead of introducing a
  new transitional layer.
- Documentation language should be descriptive and concrete rather than
  promotional or milestone-oriented.
- The docs revision should improve content boundaries, not just rename sections.
- The technical macro explanation should stay explanatory and architecture-level
  rather than becoming a line-by-line source tour.

## Success Criteria

- SC-001: Users can construct a raw statement with no bind parameters through a
  zero-argument raw constructor, and can still construct parameterized raw
  statements through a distinct explicit-codec constructor. (FR-001)
- SC-002: The public typed SQL story and schema-generated local wrappers use one
  primary naming scheme, with transitional compatibility names removed from the
  product-facing experience. (FR-002, FR-003)
- SC-003: In the targeted onboarding/book/example surfaces after the earliest
  introductory material, representative multi-field examples use field-named
  struct-shaped params or rows rather than tuple-only shapes. (FR-004, FR-009)
- SC-004: The targeted documentation surfaces do not use milestone labels and do
  not describe the documented behavior as a transition from an earlier public
  state. (FR-005, FR-009)
- SC-005: The targeted explanation pages describe design choices and tradeoffs
  without milestone language or product self-praise, and the targeted
  onboarding, book, explanation, and reference surfaces each present a distinct
  purpose consistent with Diataxis framing rather than overlapping ambiguously.
  (FR-006, FR-007, FR-009)
- SC-006: Advanced readers can find one dedicated technical macro explanation
  that covers macro entrypoints, generated wrappers, parsing/validation,
  diagnostics, verification, lowering, and runtime statement shapes. (FR-008)

## Assumptions

- This remains greenfield enough that removing transitional naming is an
  acceptable cleanup rather than a compatibility risk.
- The accepted exact naming decisions for the raw constructors are already part
  of the workflow input and do not need to be re-litigated in the spec.
- A small amount of tuple usage in the very first introductory material is still
  acceptable if it clearly lowers initial cognitive load.
- The documentation revision can improve structure and tone within the existing
  docs set without requiring a complete site redesign.

## Scope

In Scope:
- Raw fallback constructor cleanup
- Removal of transitional typed-SQL compatibility naming from the public and
  product-facing surface
- Generated wrapper naming alignment
- Struct-first example revisions after the earliest introductory material
- Targeted documentation cleanup across the library overview/onboarding path,
  the main book surfaces that demonstrate querying/commands, relevant examples,
  and the explanation pages that describe design/positioning
- One dedicated technical macro explanation for advanced readers
- Explicit Diataxis-style clarification of the role of the affected docs
  surfaces

Out of Scope:
- Expanding the typed SQL subset again
- Broad runtime behavior changes beyond the accepted raw-constructor cleanup
- Rewriting every introductory tuple example out of existence
- A full documentation-site redesign unrelated to this accepted bundle
- New compatibility shims or extra transitional naming layers

## Dependencies

- The already-implemented unified typed SQL surface
- Existing generated schema-wrapper behavior
- Existing docs/book/explanation/reference surfaces that this wave revises
- The accepted improvement decisions already captured for this workflow

## Risks & Mitigations

- Transitional names may remain in overlooked public surfaces: Mitigation: treat
  API names, generated wrappers, examples, tests, and product-facing docs as one
  coordinated cleanup target.
- Struct-first revisions may introduce too much ceremony too early: Mitigation:
  keep tuple examples only in the earliest introduction, not as the general
  pattern afterward.
- Tone cleanup may accidentally remove useful technical context: Mitigation:
  revise explanation pages toward concrete design questions and tradeoffs rather
  than simply shortening them.
- Diataxis framing may stay superficial if only headings change: Mitigation:
  require clearer role separation across the targeted docs surfaces.
- The technical macro explanation may drift into low-value implementation sprawl:
  Mitigation: keep the deliverable centered on reader understanding of the macro
  pipeline and its boundaries.

## References

- Issue: none
- Research: none
