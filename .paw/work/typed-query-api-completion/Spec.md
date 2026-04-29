# Feature Specification: Typed Query API Completion

**Branch**: feature/typed-query-macros  |  **Created**: 2026-04-28  |  **Status**: Draft
**Input Brief**: Complete babar's typed query API as the new query direction, including a declarative model for optional filters, ordering, and pagination in listing-style queries.

## Overview

`babar` now has the beginnings of a schema-aware typed query surface, but it is still a narrow proof of concept. The next step is to turn that proof of concept into a coherent query API that people can actually adopt as the main way they write typed read queries. The value is not only compile-time checking of selected columns and parameter types, but also giving users a readable, SQL-first way to declare the query behaviors they want to expose in real applications.

The most important user-facing need is list and lookup queries with many possible filters, sort orders, and pagination controls where only some inputs are active for any given request. Users want to declare those possibilities once, keep the SQL recognizable, and let the query surface omit inactive pieces without sending broken or semantically misleading SQL to the server. That omission behavior must be explicit and predictable so users can reason about what query will run when some inputs are absent.

This effort therefore focuses on making the typed query API feel complete for read paths: schema-aware `SELECT`s, explicit optional predicate/group boundaries, optional ordering and pagination clauses, and clear rules for how absent inputs change the emitted query. The result should feel like SQL with a small amount of structure-aware optionality, not like a general templating language or ORM DSL.

## Objectives

- Make the typed query API a viable primary surface for schema-aware read queries.
- Support declarative listing/query endpoints where many filters may be exposed but only some are active at runtime.
- Preserve a SQL-first reading experience, with only small explicit markers for optional behavior.
- Ensure omission behavior is predictable, explicit, and safe for query semantics.
- Keep the first completed version query-focused so the read/query model becomes solid before write support expands the surface area.

## User Scenarios & Testing

### User Story P1 – Author schema-aware read queries
Narrative: A babar user writes a typed read query against declared tables and columns and expects the query to compile only when the selected columns, placeholders, and row shape are valid.
Independent Test: Write a valid read query against declared schema metadata and confirm the emitted query surface is accepted with the correct parameter and row types.
Acceptance Scenarios:
1. Given a query that selects declared columns from declared tables, When the user compiles it, Then the query is accepted and exposes the correct typed parameter and row shapes.
2. Given a query that references an unknown table or column, When the user compiles it, Then the query is rejected with a location-aware diagnostic.

### User Story P1 – Declare optional filters for listing endpoints
Narrative: A babar user defines a listing query with several optional filters and expects only the active filters to contribute to the SQL sent to the server.
Independent Test: Execute the same query shape with different subsets of optional filter inputs and confirm inactive filters are omitted while active filters remain.
Acceptance Scenarios:
1. Given a query with multiple optional filters, When only one optional input is present, Then only that filter participates in the emitted SQL.
2. Given a query with no active optional filters, When the query runs, Then the emitted SQL remains valid and excludes the inactive filter predicates.

### User Story P1 – Control omission boundaries explicitly
Narrative: A babar user writes a compound condition and needs grouped optional logic to disappear only at declared group boundaries so the query meaning does not change unexpectedly.
Independent Test: Define a grouped range predicate with two optional inputs and confirm the grouped predicate is present only when its required inputs are present.
Acceptance Scenarios:
1. Given an explicitly grouped optional predicate that depends on multiple optional inputs, When all required inputs are present, Then the entire group participates in the emitted SQL.
2. Given that same grouped optional predicate, When one required input is absent, Then the entire group is omitted instead of leaving behind a partial condition.

### User Story P2 – Make ordering and pagination optional
Narrative: A babar user builds a general listing query and wants sort order, limit, and offset to be exposed as optional query behaviors without hand-building SQL fragments.
Independent Test: Compile and run a listing query where ordering and pagination inputs are independently present or absent and confirm the emitted SQL includes only active tail clauses.
Acceptance Scenarios:
1. Given a query with optional ordering, When the caller does not provide ordering input, Then no ordering clause is emitted.
2. Given a query with optional pagination inputs, When only limit is present, Then the emitted SQL includes limit behavior without requiring offset.

### User Story P2 – Learn the API from examples and docs
Narrative: A babar user evaluating the new query direction wants examples and documentation that show how ordinary service code uses the typed query API, especially for listing/filtering endpoints.
Independent Test: Follow the public examples and docs to build or adapt a small read endpoint using the typed query surface.
Acceptance Scenarios:
1. Given the public docs and example code, When a user looks for a typed query service example, Then they can find a realistic read-focused example.
2. Given documentation for optional query behavior, When a user reads it, Then they can understand when inactive filters or tail clauses are omitted.

### Edge Cases

- An optional single-filter input is absent while surrounding non-optional predicates remain present.
- A grouped optional predicate references more than one optional input and only some are present.
- An optional ordering clause is absent while optional pagination remains present.
- A query contains no active optional filters at runtime and must still emit valid SQL.
- The same optional input is referenced more than once in a query.
- A query author marks omission boundaries unclearly or in an unsupported context.

## Requirements

### Functional Requirements

- FR-001: The typed query API must support schema-aware `SELECT` queries as the primary completed scope for this initiative. (Stories: P1)
- FR-002: The typed query API must validate selected columns, declared relations, and bound inputs against declared schema information before the query is accepted. (Stories: P1)
- FR-003: The API must let query authors declare optional single-filter inputs for read queries so inactive filters are omitted from emitted SQL. (Stories: P1)
- FR-004: The API must let query authors declare explicit optional predicate/group boundaries so compound conditions are omitted only at clear author-controlled boundaries. (Stories: P1)
- FR-005: The API must support optional ordering behavior for read queries so ordering can be exposed without requiring it for every execution. (Stories: P2)
- FR-006: The API must support optional limit and offset behavior for read queries so pagination controls can be exposed independently. (Stories: P2)
- FR-007: The API must emit valid SQL for every supported combination of active and inactive optional inputs within a declared query. (Stories: P1,P2)
- FR-008: The API must reject unsupported optional-placement patterns with diagnostics that identify the problematic query region and explain the omission rule being violated. (Stories: P1)
- FR-009: The API must preserve a SQL-first authoring style, using only a small explicit mechanism for optional behavior rather than requiring an out-of-band query builder. (Stories: P1,P2)
- FR-010: Public docs and examples must explain the query-only scope, optional filter/tail-clause behavior, and the intended use for service-style listing endpoints. (Stories: P2)

### Key Entities

- Typed Query: A schema-aware read query whose selected columns, inputs, and emitted SQL behavior are validated before use.
- Optional Filter Input: A declared read-query input whose absence removes an owned predicate from the emitted SQL.
- Optional Group Boundary: An explicit author-controlled grouping that causes an entire predicate/group fragment to be omitted when its required optional inputs are not all present.
- Optional Tail Clause: An optional ordering or pagination behavior that is emitted only when its corresponding input is active.
- Active Input Set: The subset of optional inputs provided for a given execution of a declared query.

### Cross-Cutting / Non-Functional

- Omission behavior must be predictable from the query text and not depend on hidden heuristics.
- Diagnostics for unsupported optional behavior must be understandable without reading implementation internals.
- The completed read-query API must remain approachable for ordinary service code and not require users to adopt a second non-SQL builder model.

## Success Criteria

- SC-001: Users can author schema-aware read queries that expose typed parameter and row shapes without manually supplying row codecs for supported query patterns. (FR-001, FR-002)
- SC-002: For supported queries with optional filters, every tested combination of active and inactive inputs emits valid SQL that matches the declared omission rules. (FR-003, FR-004, FR-007)
- SC-003: Grouped optional predicates are omitted as whole units when required grouped inputs are missing, with no partial-condition rewrites in supported cases. (FR-004, FR-007)
- SC-004: Users can declare optional ordering, limit, and offset behavior for listing queries and activate those behaviors independently at execution time. (FR-005, FR-006)
- SC-005: Unsupported optional-placement patterns fail with location-aware diagnostics that explain what omission boundary is required. (FR-008)
- SC-006: Public examples and documentation show at least one realistic service-style typed query example and explain the read-only scope and optional-behavior rules. (FR-009, FR-010)

## Assumptions

- The first completed version remains query-only and does not attempt to finish write-oriented typed query support in the same initiative.
- Optional behavior will remain intentionally narrow and structure-aware rather than becoming a general-purpose SQL templating system.
- A small explicit optional marker is acceptable to users so long as the surrounding query remains recognizably SQL.
- Inline schema declaration may remain part of the user story for now even if reusable authored/generated schema sources are a later goal.
- Optional tail-clause behavior is primarily aimed at service-style listing endpoints rather than arbitrary advanced SQL constructs.

## Scope

In Scope:
- Completing the typed query API for schema-aware read queries
- Optional single-filter behavior for read queries
- Explicit optional grouped predicates
- Optional ordering, limit, and offset behavior
- Rules for emitted SQL under different active-input combinations
- Diagnostics, examples, and docs for the completed read-query surface

Out of Scope:
- `INSERT`, `UPDATE`, `DELETE`, or other write-command support in this initiative
- A general-purpose SQL templating or rewrite engine
- Full SQL grammar coverage beyond the supported typed read-query subset
- Generated schema modules or full schema codegen as a required part of this milestone
- Fluent query-builder APIs that replace SQL-first authoring

## Dependencies

- Existing typed query parsing, normalization, resolution, and lowering pipeline
- Public typed query surface and current inline schema mechanism
- Documentation and example surfaces used to teach babar query APIs

## Risks & Mitigations

- Omission rules may become confusing if too many cases are supported at once: Mitigation: keep v1 narrow, require explicit optional boundaries, and reject unsupported placements.
- Users may expect arbitrary SQL rewriting once optional syntax exists: Mitigation: document the supported omission model clearly and treat broader rewriting as out of scope.
- Optional ordering and pagination may interact awkwardly with grouped predicate rules: Mitigation: define tail-clause omission behavior separately from boolean predicate omission.
- Query-only completion may leave some users wanting write support immediately: Mitigation: state explicitly that write support is deferred so the read model can stabilize first.
- Inline schema declarations may feel repetitive for large services: Mitigation: document this as a temporary constraint and leave reusable schema sources to later work.

## References

- Issue: none
- Research: none
