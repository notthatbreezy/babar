# Feature Specification: Struct Support in Macros

**Branch**: feature/struct-support-in-macros  |  **Created**: 2026-05-05  |  **Status**: Draft
**Input Brief**: Make the schema-aware typed SQL feature support struct-shaped inputs and query rows with strict pre-execution validation.

## Overview

`babar` already encourages users to model application data with named structs,
and its lower-level statement-building path already supports that style. The gap
is that the schema-aware typed SQL path does not currently preserve the same
struct-centric experience. A user can write schema-aware SQL that appears to fit
naturally with normal application data models, but the generated statement
contract still pushes users toward positional shapes instead of the named shapes
they expect to keep at the boundaries of their program.

This work closes that gap by making the schema-aware typed SQL experience
support struct-shaped command inputs, query inputs, and query output rows no
matter whether the user works from a reusable schema module or declares schema
facts directly in the statement. The desired result is that users can keep
organizing database-facing values as named structs without giving up the
authored-schema workflow. Instead of forcing users to switch to positional
shapes, the feature should preserve named structs wherever the statement
contract can be expressed as a named field set.

The feature must remain strict. A user should not be able to pass a struct that
is only “close enough” to the SQL contract. If a required field is missing, if a
field type does not align with the SQL type or nullability, or if the struct
contains extra fields that do not belong to the statement contract, the mismatch
should be rejected before the program can run. The user should be able to rely
on the schema-aware macro path as a correctness boundary rather than a lenient
conversion layer.

This work also needs to align the product story around examples and docs. Once
the schema-aware typed SQL feature genuinely supports structs, the public
examples should prefer struct-shaped values as the normal path and avoid
splitting write and read models unless the statement contract truly requires
different data structures. The goal is that the public guidance teaches the same
value-shaping rules that users actually encounter when they adopt the feature.

## Objectives

- Let users use named structs as the normal data shape for schema-aware typed
  SQL inputs and query rows.
- Preserve struct support across both reusable-schema and inline-schema typed SQL
  entry surfaces.
- Enforce strict contract matching so missing fields, extra fields, type
  mismatches, and nullability mismatches are rejected before execution.
- Support both directly named struct selection and inferred struct selection
  where the declared statement type makes the intended shape clear.
- Align public examples and tutorial-style documentation so they teach named
  structs, rather than positional shapes, whenever a statement contract can be
  represented as a named field set.
- Use a single struct in examples when the statement input field set and query
  output field set are identical, and use separate structs only when those
  field sets differ.

## User Scenarios & Testing

### User Story P1 – Use named structs with schema-aware typed SQL inputs
Narrative: A user wants to keep using normal Rust structs for database-facing
values instead of switching to positional tuples when they adopt schema-aware
typed SQL.
Independent Test: A user writes schema-aware commands and queries, passes a
named struct into execution, and the generated statement accepts that
struct shape without requiring a tuple-based rewrite.
Acceptance Scenarios:
1. Given a user defining a schema-aware command, When they bind a
   named struct whose fields match the required statement inputs, Then the
   command accepts that struct as the input value.
2. Given a user defining a parameterized schema-aware query,
   When they bind a named struct whose fields match the query inputs, Then the
   query accepts that struct as the input value.
3. Given a user who prefers to declare statement types in surrounding type
   positions, When the surrounding type makes the intended struct shape clear,
   Then the generated statement can use that struct shape without requiring
   a tuple contract.

### User Story P1 – Use named structs for schema-aware query rows
Narrative: A user wants query results from schema-aware typed SQL to decode into
named row structs so the application boundary stays readable and domain-shaped.
Independent Test: A user defines a schema-aware query, requests a named row
struct as the query output shape, and receives decoded rows in that struct
shape.
Acceptance Scenarios:
1. Given a schema-aware query whose projected fields match a named row struct,
   When the user executes the query, Then the returned rows decode into that
   struct shape.
2. Given a user who wants to make the row shape explicit, When they identify a
   row struct for the query, Then the query result type can be expressed in
   terms of that struct rather than a positional tuple.

### User Story P1 – Reject struct/SQL mismatches strictly
Narrative: A user wants the schema-aware typed SQL path to behave like a strict
contract so shape mismatches are caught early and cannot silently slip through.
Independent Test: A user tries to compile statements whose struct shape is
missing required fields, contains extra fields, or uses incompatible field
types, and each mismatch is rejected before execution.
Acceptance Scenarios:
1. Given a schema-aware statement that requires three named fields for an input
   contract,
   When the user supplies a struct missing one of those required fields, Then the
   statement is rejected before it can run.
2. Given a schema-aware statement whose SQL contract expects specific field
   types or nullability, When the user supplies a struct with incompatible field
   types or nullability, Then the statement is rejected before it can run.
3. Given a schema-aware statement whose contract only includes a defined field
   set, When the user supplies a struct with additional unrelated fields, Then
   the statement is rejected before it can run.

### User Story P2 – Keep struct support consistent across both typed SQL surfaces
Narrative: A user should not have to remember one struct story for
reusable-schema typed SQL and a different one for inline-schema typed SQL.
Independent Test: Review both typed SQL surfaces and confirm that struct-shaped
inputs and query rows are supported under the same behavioral rules.
Acceptance Scenarios:
1. Given a user working from a reusable schema declaration, When they use the
   reusable-schema typed SQL surface, Then struct support follows the same rules
   as the inline-schema surface.
2. Given a user writing a one-off inline-schema example, When they use the
   inline-schema surface, Then struct support remains available under the same
   strict matching rules.

### User Story P2 – Learn the recommended struct-centric path from the docs
Narrative: A user reading the docs should learn a macro workflow that matches
the real implementation and should not be pushed toward unnecessary tuple usage
or unnecessary splitting of near-identical read/write structs.
Independent Test: Review the updated examples and confirm they prefer
struct-shaped inputs and rows whenever the statement contract can be represented
as a named field set, while only using separate structs when the input field set
and output field set differ.
Acceptance Scenarios:
1. Given a user following the getting-started and book examples, When they read
    the schema-aware macro examples, Then structs are presented as the normal
    value shape where appropriate.
2. Given an example whose input field set and output field set are identical,
   When the docs show the statement flow, Then the example uses a single shared
   struct shape.
3. Given an example whose input field set and output field set differ, When the
   docs show the statement flow, Then the example may use separate struct
   shapes for those distinct contracts.

### Edge Cases

- A query projection may expose aliased or reordered output names that require a
  clear matching rule for row structs.
- Optional placeholders and nullable SQL types may require strict handling of
  optional fields without weakening the missing-field checks.
- Some statements may still need tuple-style or alternative shapes when the SQL
  contract is not naturally representable as a named struct.
- Users may want to express the struct shape explicitly in some cases and rely
  on surrounding type inference in others; those routes must not conflict
  unpredictably.
- Existing docs/examples that currently imply struct support need to be brought
  into line with the implemented behavior once the feature lands.

## Requirements

### Functional Requirements
- FR-001: The reusable-schema and inline-schema typed SQL surfaces must support
  named struct-shaped values as command
  input parameters when the struct fields match the statement input contract.
  (Stories: P1,P2)
- FR-002: The reusable-schema and inline-schema typed SQL surfaces must support
  named struct-shaped values as query
  input parameters when the struct fields match the statement input contract.
  (Stories: P1,P2)
- FR-003: The reusable-schema and inline-schema typed SQL surfaces must support
  named struct-shaped output rows for query results
  when the struct fields match the query result contract. (Stories: P1)
- FR-004: When both explicit struct selection and surrounding-type inference are
  present for the same statement, explicit struct selection must take precedence.
  (Stories: P1)
- FR-005: Output row matching must be determined by final output field names,
  including aliases when the statement defines them, rather than by projection
  order alone. (Stories: P1)
- FR-006: The feature must reject a struct-shaped statement contract when any
  required SQL field is missing from the struct. (Stories: P1)
- FR-007: The feature must reject a struct-shaped statement contract when any
  struct field type or nullability does not align with the SQL contract.
  (Stories: P1)
- FR-008: The feature must reject a struct-shaped statement contract when the
  struct contains extra fields outside the SQL contract, for both input structs
  and output row structs. (Stories: P1)
- FR-009: The feature must support explicit struct selection for schema-aware
  typed SQL inputs and query rows. (Stories: P1,P2)
- FR-010: The feature must support surrounding-type inference for schema-aware
  typed SQL inputs and query rows where the surrounding statement type makes the
  intended struct shape unambiguous. (Stories: P1)
- FR-011: Public examples and documentation for schema-aware typed SQL usage
  must use named structs, rather than positional shapes, whenever the statement
  contract can be represented as a named field set. (Stories: P2)
- FR-012: Updated examples and documentation must avoid splitting otherwise
  identical write/read struct shapes. When the statement input field set and the
  query output field set are identical, the example must use one shared struct;
  when those field sets differ, the example may use separate structs. (Stories: P2)

### Key Entities
- Struct-Shaped Input: A named Rust struct used as the bound parameter value for
  a schema-aware command or query.
- Struct-Shaped Row: A named Rust struct used as the decoded row value for a
  schema-aware query result.
- SQL Contract: The required set of statement inputs or projected query outputs,
  including field names, types, and nullability.
- Explicit Struct Selection: A user action that directly identifies the intended
  struct shape for a macro-generated statement.
- Surrounding-Type Inference: A workflow in which the intended struct shape is
  taken from the enclosing statement type when that context is unambiguous.

### Cross-Cutting / Non-Functional
- The feature must preserve strict contract validation and must not degrade into
  a best-effort or runtime-only matching model.
- Error behavior for mismatched struct contracts must identify the field or
  contract rule that caused the rejection.
- The supported struct-centric path should be taught consistently across docs
  and examples so the public story matches the actual macro behavior.

## Success Criteria
- SC-001: On both the reusable-schema and inline-schema typed SQL surfaces, a
  command input contract with three required named fields accepts a struct with
  exactly those three fields and rejects a struct missing any one of them.
  (FR-001, FR-006)
- SC-002: A schema-aware statement that expects a non-null integer, text, and
  date contract rejects struct fields whose types or nullability do not align
  with those expectations. (FR-006, FR-007)
- SC-003: A schema-aware statement whose SQL contract contains no field beyond a
  specific named set rejects any candidate input struct or output row struct
  that adds unrelated extra fields. (FR-008)
- SC-004: A reusable-schema typed SQL example and an inline-schema typed SQL
  example both support struct-shaped inputs and named row shapes under the same
  strict matching rules for missing fields, extra fields, and incompatible field
  types. (FR-001, FR-002, FR-003)
- SC-005: A schema-aware query can return named row structs instead of only
  tuple-shaped rows when the projected query contract matches the struct
  contract by final output field names. (FR-003, FR-005, FR-009, FR-010)
- SC-006: Users can obtain struct-shaped macro behavior through explicit struct
  selection and also through surrounding-type inference in cases where the
  intended struct shape is unambiguous, and explicit selection wins when both
  routes are present for the same statement. (FR-004, FR-009, FR-010)
- SC-007: The getting-started documentation and at least one book-style example
  show schema-aware typed SQL with named structs, rather than positional shapes,
  when the statement contract can be represented as a named field set; and in
  those reviewed examples, a single shared struct is used when the input and
  output field sets are identical, while separate structs are used only when
  those field sets differ. (FR-011, FR-012)

## Assumptions
- Strict rejection of extra fields applies to both input structs and output row
  structs.
- The specification does not need to lock down exact user-facing syntax for
  explicit struct selection as long as the capability exists and remains
  unambiguous to users.
- The specification does not require every schema-aware statement shape to be
  representable as a struct; tuple or other fallback shapes may still exist
  where the SQL contract is not naturally representable as a named struct.
- Documentation cleanup can be delivered as part of the same feature as long as
  it remains scoped to examples and explanations directly affected by the new
  struct-centric macro behavior.
- Output row matching is defined by final output field names, including aliases,
  rather than by projection order alone.
- When explicit struct selection and surrounding-type inference are both present
  for the same statement, explicit struct selection wins.

## Scope

In Scope:
- Struct-shaped command inputs for schema-aware typed SQL
- Struct-shaped query inputs for schema-aware typed SQL
- Struct-shaped query rows for schema-aware typed SQL
- Support across both reusable-schema and inline-schema typed SQL surfaces
- Strict compile-time-style rejection of missing fields, extra fields, and
  incompatible types/nullability
- Both explicit struct selection and surrounding-type inference where the intent
  is unambiguous
- Example and docs updates directly tied to the new macro behavior

Out of Scope:
- Relaxed or partial field matching that ignores extra struct fields
- Runtime-only mismatch detection as the primary enforcement mechanism
- Redesign of unrelated raw-builder APIs that already support struct-shaped
  values
- Broad expansion of the typed-SQL feature set unrelated to struct support
- Guaranteeing that every possible SQL statement can be expressed through a
  single shared struct shape

## Dependencies

- Existing reusable-schema and inline-schema typed SQL surfaces
- Existing struct codec support used by raw builders
- Existing tuple-shaped statement workflows that must remain valid for contracts
  that are not represented as strict named structs
- Unambiguous precedence between explicit struct selection and surrounding-type
  inference
- Documentation and example surfaces that currently teach schema-aware typed SQL
  usage

## Risks & Mitigations

- Strict field rejection may reduce reuse of “larger” structs across multiple
  statements: Mitigation: make strictness explicit in docs and examples so the
  constraint feels intentional rather than surprising.
- Supporting both explicit selection and inference may create ambiguous cases:
  Mitigation: define a single unambiguous precedence rule and test conflict
  cases directly.
- Query row matching may become confusing when SQL projection names are aliased
  or reordered: Mitigation: define a clear row-matching rule and add compile-fail
  coverage for ambiguous or conflicting output shapes.
- The current macro pipeline may embed tuple-oriented assumptions that make
  struct support broader than it first appears: Mitigation: plan the work as a
  focused extension of the existing macro pipeline with targeted pass/fail
  coverage before large docs changes.
- Public docs currently imply stronger struct support than the implementation
  provides: Mitigation: ship docs updates alongside the behavior change so the
  public story and actual behavior converge together.

## References
- Issue: none
- Work Shaping artifact from the current workflow
- Research: none
