# Feature Specification: External Schemas

**Branch**: feature/external-schemas  |  **Created**: 2026-04-29  |  **Status**: Draft
**Input Brief**: Add ergonomic external schema support so `typed_query!` can use authored or generated Rust-visible schema declarations instead of requiring inline schema blocks.

## Overview

`typed_query!` already proves that babar can validate read queries against declared schema information, but the current inline schema block is still a local proof-of-concept authoring model. It works for small examples, yet it becomes repetitive and awkward once a project wants to reuse the same table definitions across queries, modules, examples, or generated schema artifacts. The next step is to let users define schema information once and reference it from typed queries without falling back to stringly or file-path-heavy configuration.

The user-facing goal is not just reuse. It is to make schema declarations feel natural in Rust and easy to read in ordinary application code. Users should be able to express table and field meaning with a type-first model where the core shape remains obvious from the declaration itself, and metadata only appears when it adds meaning. The experience should feel closer to ordinary Rust data modeling than to maintaining a separate mini-language.

This feature therefore focuses on a Rust-visible external schema model for authored and generated declarations. Users should be able to keep schema facts in reusable Rust-facing artifacts, use them across many queries, and preserve babar's SQL-first query surface. The user experience should stay simple enough that external schemas feel like a convenience and ergonomics upgrade, not like a second framework layered on top of `typed_query!`.

## Objectives

- Let users reuse schema definitions across many typed queries without repeating inline schema declarations.
- Keep the primary authoring model Rust-native, with schema meaning conveyed mostly by types and a small amount of additive metadata.
- Support both hand-authored schema declarations and generated Rust-visible schema artifacts in the same conceptual model.
- Preserve babar's SQL-first query authoring style so external schemas complement `typed_query!` rather than replacing it.
- Keep the first version focused on the schema information needed for typed read-query validation, not on full database modeling.
- Present a small set of clear user-facing API directions so planning can choose among ergonomic Rust-native options without reopening scope.

## User Scenarios & Testing

### User Story P1 – Reuse schema declarations across queries
Narrative: A babar user declares schema information once and expects multiple typed queries to reference it without copying table and column definitions into each macro call.
Independent Test: Define one reusable schema source and use it in multiple typed queries that compile successfully against the same declared tables and columns.
Acceptance Scenarios:
1. Given a reusable schema declaration that includes several tables, When a user references it from more than one typed query, Then each query validates against the same shared schema information.
2. Given a schema declaration that changes, When a user recompiles queries that reference it, Then those queries reflect the updated schema facts consistently.

### User Story P1 – Author schemas in a Rust-native way
Narrative: A babar user wants schema declarations to feel natural in Rust, with important field meaning visible in the declaration and without a large amount of detached metadata syntax.
Independent Test: Review a representative schema declaration and confirm that table, column, nullability, and key field meaning are understandable from the declaration structure itself.
Acceptance Scenarios:
1. Given a schema declaration for a table with several fields, When a user reads it, Then the field types and important field semantics are understandable without cross-referencing a second DSL.
2. Given a declaration that needs field-level metadata, When the user adds that metadata, Then it appears as an additive refinement rather than replacing the type-driven shape.

### User Story P1 – Use generated schema artifacts without changing the query model
Narrative: A babar user relies on generated schema information and wants generated artifacts to participate in the same external schema model as hand-authored declarations.
Independent Test: Use a generated Rust-visible schema artifact as the schema source for a typed query and confirm the query is authored the same way as one using a hand-authored schema source.
Acceptance Scenarios:
1. Given a generated Rust-visible schema artifact, When a user references it from a typed query, Then the query authoring flow matches the hand-authored schema flow.
2. Given a project that mixes generated and hand-authored schema declarations, When users read the code, Then the high-level usage pattern remains consistent.

### User Story P2 – Learn a small, ergonomic schema API
Narrative: A babar user evaluating external schemas wants the public API to offer a small number of understandable patterns rather than many overlapping declaration mechanisms.
Independent Test: Compare the documented user-facing schema approaches and confirm one or more recommended patterns are clear, scoped, and easy to explain.
Acceptance Scenarios:
1. Given the docs for external schemas, When a user looks for the recommended authoring style, Then the recommended pattern is clear.
2. Given alternate supported authoring patterns, When a user compares them, Then the tradeoffs are described in terms of ergonomics and intended use rather than implementation internals.

### User Story P2 – Keep unsupported complexity out of v1
Narrative: A babar user wants reusable schemas, but does not want the first version to introduce path-driven configuration, heavy code generation requirements, or a full schema-management framework.
Independent Test: Review the v1 scope and confirm unsupported complexity is explicitly excluded.
Acceptance Scenarios:
1. Given the first version of external schemas, When a user reads the scope, Then file-path-based schema loading is clearly out of scope.
2. Given the first version of external schemas, When a user evaluates whether it is a full database-modeling system, Then it is clear that advanced schema-management concerns remain out of scope.

### Edge Cases

- A user references a reusable schema source that does not contain the table or column named in the query.
- A generated schema artifact and a hand-authored declaration model the same table differently.
- A field needs an important semantic marker such as primary-key identity while most fields need only type/nullability information.
- A schema declaration grows large enough that excessive per-field metadata would harm readability.
- A user wants non-Rust external files as schema sources, but the v1 model intentionally stays Rust-visible only.
- The current typed query lowering/runtime support cannot yet represent every schema type that a declaration format could theoretically describe.

## Requirements

### Functional Requirements

- FR-001: `typed_query!` must support reusable external schema sources so users can validate queries without repeating inline schema blocks for each query. (Stories: P1)
- FR-002: The first version of external schemas must target Rust-visible authored declarations and Rust-visible generated declarations as the supported source model. (Stories: P1)
- FR-003: The user-facing schema declaration model must remain type-first, with additive metadata used only to refine field semantics that are not already obvious from the declaration's type shape. (Stories: P1)
- FR-004: The external schema experience must preserve the existing SQL-first typed query model so users continue authoring queries as SQL rather than through a second query builder surface. (Stories: P1)
- FR-005: Users must be able to express the schema facts required for typed read-query validation through the external schema model, including table identity, field identity, field types, and nullability. (Stories: P1)
- FR-006: The external schema model must provide a way to express a small set of higher-value field semantics, such as primary-key identity or similarly important attributes, without making those annotations mandatory for ordinary fields. (Stories: P1)
- FR-007: Hand-authored and generated Rust-visible schema declarations must be usable through a consistent mental model so the query authoring experience does not fork by source origin. (Stories: P1)
- FR-008: The public docs/spec must describe a small set of user-facing API options for external schema authoring and identify the recommended direction in ergonomics terms. (Stories: P2)
- FR-009: The v1 scope must explicitly exclude file-path-based schema inputs and other non-Rust external source models from the primary supported user flow. (Stories: P2)
- FR-010: The v1 scope must explicitly exclude advanced schema-management concerns that are not required for typed read-query validation. (Stories: P2)

### Key Entities

- External Schema Source: A reusable Rust-visible declaration that typed queries can reference for schema facts.
- Authored Schema Declaration: A hand-written external schema source maintained by library users.
- Generated Schema Declaration: A Rust-visible artifact produced by another tool but consumed through the same external schema model as authored declarations.
- Field Semantic Marker: A small, explicit refinement that communicates an important field meaning not conveyed by the field's type shape alone.
- Recommended Schema Pattern: The primary user-facing declaration style babar documents for external schemas.

### Cross-Cutting / Non-Functional

- The primary authoring pattern must optimize for readability in normal Rust code, not just macro flexibility.
- Important field meaning should be visually local to the field declaration and should not require users to learn a large secondary DSL.
- The recommended pattern should remain understandable when declarations are authored by hand and when they are generated.
- The v1 experience should avoid surprising indirection, hidden file loading, or path-sensitive behavior in the main user flow.

## User-Facing API Direction Options

The spec should carry these options forward for planning and selection:

1. **Schema module pattern**: users define reusable table and field declarations in a dedicated Rust-visible schema module, then reference that schema source from typed queries. This is the most explicit and modular option.
2. **Type-driven table declaration pattern**: users define Rust-visible table/field declarations where type wrappers or marker types communicate important semantics such as key identity. This best matches the type-first ergonomics goal.
3. **Hybrid pattern**: users define a Rust-visible schema module, but field declarations can use reusable semantic wrappers so the common case stays terse while richer semantics remain available when needed.

The recommended direction for v1 is the **hybrid pattern**, because it keeps the schema source explicit and reusable while still allowing the field model itself to feel Rust-native and type-driven.

## Success Criteria

- SC-001: Users can define a reusable Rust-visible schema source once and reference it from multiple typed queries without duplicating inline schema blocks. (FR-001, FR-002)
- SC-002: The documented primary authoring style reads as a Rust-native declaration model where core schema meaning is visible from types and only a small amount of additive metadata is needed. (FR-003, FR-006, FR-008)
- SC-003: Users can employ hand-authored and generated Rust-visible schema declarations through the same high-level query authoring flow. (FR-002, FR-007)
- SC-004: The external schema feature preserves babar's SQL-first query experience rather than introducing a separate builder-centric query model. (FR-004)
- SC-005: The v1 scope clearly excludes file-based external schema loading and advanced schema-management features not required for typed read-query validation. (FR-009, FR-010)

## Assumptions

- The first version of external schemas is still anchored to the current typed read-query scope and does not expand the feature into full write-query or ORM-style modeling.
- Important field semantics should remain intentionally narrow in v1 and focus on the attributes that most improve readability and ergonomics.
- Generated schema support means generated Rust-visible artifacts are accepted in the same conceptual model, not that babar must ship a schema generator in this milestone.
- The external schema feature may initially inherit some of today's typed-query type/lowering limits even if the declaration model itself could describe more.
- A small set of recommended declaration patterns is preferable to a fully open-ended schema authoring framework.

## Scope

In Scope:
- Reusable external schema support for typed read queries
- Rust-visible authored schema declarations
- Rust-visible generated schema declarations
- A type-first declaration style with additive semantic markers where needed
- Documentation of a small set of user-facing API options and a recommended direction
- Preservation of SQL-first typed query authoring

Out of Scope:
- File-path-based or other non-Rust external schema inputs in v1
- Live database introspection as the primary user flow for this milestone
- A full schema management, migration, or ORM modeling framework
- Requiring babar itself to generate schema artifacts in this milestone
- Expanding the feature beyond the schema needs of typed read-query validation

## Dependencies

- The existing typed query validation model and supported read-query scope
- A reusable Rust-visible representation of the schema facts typed queries need
- Documentation/examples that teach the recommended external schema pattern clearly

## Risks & Mitigations

- Users may expect “external schema” to include files, database snapshots, or live introspection immediately: Mitigation: state clearly that v1 is Rust-visible only and explain why that improves ergonomics and predictability.
- A too-flexible declaration model could become another DSL instead of feeling like Rust: Mitigation: keep the model type-first and prefer additive semantic markers over large metadata blocks.
- Generated and hand-authored schemas could drift into separate mental models: Mitigation: require one consistent consumption model for both source origins.
- Trying to represent too many field semantics at once could make declarations noisy: Mitigation: limit v1 to the highest-value semantics and keep advanced attributes out of scope.
- Users may assume external schemas remove all current typed-query limitations: Mitigation: document that external schemas improve reuse and ergonomics first, while broader type/runtime expansion remains separate work.

## References

- Issue: none
- Internal research: current typed_query schema model and pipeline review
- Pydantic fields: https://docs.pydantic.dev/latest/concepts/fields/
- Pydantic aliases: https://docs.pydantic.dev/latest/concepts/alias/
- Pydantic validators: https://docs.pydantic.dev/latest/concepts/validators/
- Pydantic types/customization: https://docs.pydantic.dev/latest/concepts/types/
