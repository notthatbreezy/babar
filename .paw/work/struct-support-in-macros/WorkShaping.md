# Struct Support in Macros

## Problem Statement

`babar` already supports struct-shaped command inputs and query rows through the
raw builders plus `#[derive(babar::Codec)]`, but the schema-aware macro path
does not preserve that same ergonomic model in practice. Today, users can write
schema-aware `query!` / `command!` calls that look like they should work with
normal Rust structs, yet the macro output currently lowers to tuple-shaped
parameter and row types on this checkout.

That creates a mismatch between the intended product experience and actual macro
behavior:

- users expect structs to remain the normal Rust data-organization tool
- schema-aware macros should not force tuple-centric application code
- missing fields, extra fields, and type mismatches should fail at compile time
- docs/examples should not imply a struct-friendly macro workflow unless the
  implementation actually supports it

The desired outcome is that schema-aware macros support struct-shaped command
arguments, query input parameters, and query output rows in a way that feels
consistent with the rest of `babar`.

## Core Outcome

Users should be able to use structs naturally with both schema-aware macro
surfaces:

- schema-scoped macros from `schema!`
  - `app_schema::query!`
  - `app_schema::command!`
- top-level macros with inline schema
  - `babar::query!`
  - `babar::command!`

Struct support should apply to:

- command/input parameters
- query/input parameters
- query/output rows

The matching rule should be **very strict**:

- every required SQL field must exist on the struct
- Rust field types must align with the SQL type/nullability contract
- extra struct fields should be rejected

The long-term user experience goal is not merely “structs are possible,” but
“structs are the normal path unless the SQL shape genuinely requires something
else.”

## Work Breakdown

### Core Functionality

1. Enable schema-aware macros to target struct-shaped parameter values for
   commands and queries.
2. Enable schema-aware query macros to target struct-shaped output row values.
3. Support both:
   - explicit struct annotation
   - surrounding-type inference where possible
4. Enforce compile-time validation for:
   - missing required fields
   - extra fields
   - field type mismatches
   - nullability mismatches

### Supporting Functionality

1. Define the user-facing syntax and precedence between explicit annotation and
   surrounding-type inference.
2. Reconcile struct field names with:
   - SQL placeholder names for inputs
   - projected column/output names for rows
3. Update docs/examples to prefer structs where appropriate.
4. Simplify examples so a single struct shape is preferred whenever SQL does not
   require separate create/read models.
5. Add focused compile-fail and pass coverage for struct-aware macro behavior.

## Scope Boundaries

### In Scope

- Struct support for both schema-scoped and top-level inline-schema macro forms
- Input structs for `command!`
- Input structs for `query!`
- Output row structs for `query!`
- Compile-time enforcement of strict field matching
- Docs/example revisions needed to align the public story with the shipped
  behavior

### Explicitly Out

- Relaxed struct matching where extra fields are silently ignored
- Runtime-only validation for struct/SQL mismatches
- Broad redesign of the typed SQL subset beyond what is necessary to support
  struct shapes
- Non-macro surfaces that already work adequately with raw builders
- General-purpose DSL work unrelated to struct selection or matching

## User Interaction / Expected Experience

The shaped feature should let a user write macro-based SQL and keep their
application-facing values in named structs rather than positional tuples.

Expected experience:

1. A user defines a struct for the parameter shape and/or row shape.
2. The user either:
   - names that struct explicitly in the macro flow, or
   - relies on surrounding Rust type inference when possible
3. The macro checks that the struct shape matches the SQL contract.
4. If the shape is wrong, compilation fails with actionable diagnostics.

Examples of intended failures:

- SQL expects `$age`, `$name`, `$dob`, but the input struct only has `age` and
  `name`
- SQL expects `date`, but the field type is not date-compatible
- Query projection yields columns `id, name`, but the output struct has
  `id, display_name, active`
- Struct contains extra fields not represented by the SQL contract

## Edge Cases and Expected Handling

### Missing input fields

If a command/query input struct omits any required placeholder-backed field, the
macro expansion should fail at compile time.

### Extra input fields

Extra fields should be rejected. The user explicitly prefers “very strict”
matching so structs do not silently overfit multiple statements.

### Wrong field types

If a struct field type does not align with the resolved SQL type, compilation
should fail. This includes nullability mismatches (`T` vs `Option<T>`).

### Output field ordering vs names

Current lowering is tuple-oriented and projection-order-based. The downstream
spec/implementation must decide whether row structs are matched by:

- output field names
- output order
- or a hybrid rule with explicit annotation

This is one of the most important design decisions because strict rejection of
extra fields strongly suggests name-aware matching rather than positional-only
matching.

### Optional placeholders

Optional typed-SQL placeholders already exist. If struct inputs are supported
here, optional placeholders likely need clear interaction with `Option<T>`
fields and strict missing-field rules.

### Duplicate / aliased projections

The implementation must define how aliased projections map to struct fields and
how duplicate or ambiguous output names are rejected.

### Large structs / arity limits

Existing tuple infrastructure appears to inherit tuple-arity constraints. If
struct support still routes through tuple-based internals, field-count limits may
remain relevant.

## Rough Architecture

The likely implementation path is an extension of the current typed-SQL macro
pipeline rather than a separate macro system.

### Current flow

1. Public macro entry points parse inline or authored schema input.
2. The resolver builds checked parameter/projection metadata.
3. Lowering emits tuple-based runtime codecs and statement builders.

### Likely future flow

1. Public macro input accepts explicit struct annotation where provided.
2. Macro expansion also consults surrounding type information where feasible.
3. Resolution/lowering reconciles:
   - placeholder names/types/nullability ↔ input struct fields
   - projection names/types/nullability ↔ output struct fields
4. Code generation emits a struct-aware statement surface while continuing to
   reuse existing codec infrastructure where possible.

### Reuse opportunities

The existing `#[derive(Codec)]` infrastructure already supports struct-shaped
encoding/decoding for raw builders, which suggests the macro path should reuse
that machinery rather than invent a new runtime representation.

## Codebase Fit

Research suggests the best fit is to modify the existing typed-SQL macro
pipeline, not bolt on a separate path.

Key fit observations:

- schema-aware macros already collect the type/name/nullability information
  needed for strict struct checking
- raw builders already support struct-shaped `Command<A>` and `Query<A, B>`
  using codec derive
- current docs/examples already lean toward a struct-centric story, so aligning
  implementation with that story removes an existing product inconsistency

Likely impact areas:

- typed SQL public macro input / front-door parsing
- typed SQL resolver/lowering
- compile-fail UI tests
- docs and tutorial examples

## Critical Analysis

### Why this work is valuable

This is a high-value ergonomics and correctness improvement because it removes a
surprising gap between:

- how `babar` encourages users to model data with structs
- how the schema-aware macro path currently behaves

It also reduces cognitive friction for new users: tuple-based lowering is
technically workable, but it undermines the goal of letting application code
stay explicit and domain-shaped.

### Why modify existing behavior instead of documenting tuples

Simply documenting tuple behavior would keep the macro path inconsistent with the
rest of the library and with user expectations. Since struct support already
exists elsewhere in the system, the better fit is to extend the schema-aware
path to match the product model.

### Tradeoffs

- Strict rejection of extra fields improves correctness, but reduces reuse of a
  single “wider” struct across multiple statements
- Supporting both explicit annotation and type inference improves ergonomics, but
  introduces design complexity
- Name-aware matching is likely the most intuitive model, but may require more
  resolver/lowering changes than positional reuse of current tuple machinery

## Risks and Gotchas

- The current macro pipeline appears tuple-oriented internally; retrofitting
  struct support may expose assumptions about parameter/projection order
- Surrounding-type inference may be harder to support reliably than explicit
  annotation depending on macro expansion constraints
- Strict “no extra fields” validation may conflict with some existing examples or
  desired reuse patterns
- Optional placeholders and nullable types need especially careful design so
  diagnostics remain understandable
- Docs currently imply stronger struct support than the implementation actually
  provides; partial implementation would worsen confusion if not updated

## Open Questions for Spec / Planning

1. What exact explicit annotation syntax should the macros support?
2. When both explicit annotation and surrounding-type inference are available,
   which wins?
3. Should output rows be matched by field name, projection order, or both?
4. How should aliased SQL projections map to struct fields?
5. Should extra-field rejection apply equally to:
   - input structs
   - output structs
6. How should optional placeholders interact with `Option<T>` struct fields?
7. Can the implementation reuse derived codec machinery directly, or does the
   typed-SQL lowering need a new intermediate representation?
8. Should docs/tutorial updates be part of the same implementation phase or a
   dedicated follow-up phase?

## Session Notes

- User wants structs to be the normal ergonomic path for schema-aware macros.
- User explicitly wants compile-time rejection for missing fields, wrong types,
  and extra fields.
- User wants support on both macro surfaces:
  - schema-scoped macros
  - top-level inline-schema macros
- User wants support for both input parameters and output rows.
- User wants both explicit struct annotation and surrounding-type inference where
  possible.
- User wants examples/docs simplified to prefer a single struct shape unless SQL
  genuinely requires separate write/read models.
- Research indicates raw builders already support struct-shaped values through
  codec derive, while schema-aware macros currently lower to tuple-shaped
  inputs/rows.
