# Feature Specification: Typed SQL Unification

**Branch**: feature/typed-sql-unification  |  **Created**: 2026-04-29  |  **Status**: Revised Draft
**Input Brief**: Evolve babar's schema-aware typed SQL direction so `query!` and `command!` become the primary typed SQL API, with broader type support, live verification, scoped write coverage, and explicit raw escape hatches.

## Overview

`babar` currently gives users several overlapping ways to express SQL: raw `Query` and `Command` values, `simple_query_raw`, explicit-codec macros such as `query!` and `command!`, and the newer schema-aware `typed_query!` path. That overlap makes the product harder to learn and harder to explain. The desired next step is to converge on one primary typed SQL direction built around authored schema modules and SQL that still reads like SQL.

The user-facing goal is not merely to add more SQL syntax support. It is to make babar's main query and command surface feel coherent. A user should be able to define schema facts once, write read and write statements against those facts, optionally verify them against a live database at build time, and rely on `query!` / `command!` as the normal way to use typed SQL. Within authored schema modules, the primary call-site contract should be local `query!` and `command!` wrappers generated from that schema; the public macros remain the broader entrypoints, and `typed_query!` remains only a compatibility alias in this round rather than the primary recommendation. Users who need functionality outside the supported typed subset should still have a deliberate escape hatch, but that escape hatch should no longer be the default story.

This round therefore focuses on unification rather than parallel expansion. The schema-aware typed SQL path should grow beyond its current read-only proof of concept so it can cover a practical first write subset, close important runtime type gaps, and absorb the live-verification role currently filled by the older explicit-codec macros. At the same time, migrations and other control-plane SQL should remain on raw/simple-query execution so typed SQL does not have to become a full replacement for every SQL execution mode.

## Objectives

- Make schema-aware typed SQL the primary typed SQL experience in babar.
- Replace the current explicit-codec `query!` / `command!` story with schema-aware `query!` / `command!` behavior.
- Expand typed SQL support to cover a practical first subset of write statements in addition to read queries.
- Add live verification to the main typed SQL path so authored schema facts can be checked against a real database.
- Close the most visible runtime type-support gap between authored schema declarations and executable typed SQL.
- Preserve explicit raw execution paths for migrations, control-plane SQL, and advanced unsupported statements.
- Begin phasing out `sql!` as a primary public direction.

## User Scenarios & Testing

### User Story P1 – Use schema-aware `query!` as the default read path
Narrative: A babar user wants the main read-query API to understand authored schema facts directly, without having to manually declare parameter and row codecs for ordinary supported queries in the supported first-round read subset.
Independent Test: Define an authored schema module, write a supported single-statement read query with `query!`, and confirm it compiles with inferred typed parameter and row behavior.
Acceptance Scenarios:
1. Given an authored schema module with declared tables and columns, When a user writes a supported row-returning statement with `query!`, Then the statement validates against that schema and exposes the correct typed read behavior.
2. Given a query that references a missing or mismatched schema element, When the user compiles it, Then the query fails with a diagnostic that identifies the invalid table, column, or type expectation.

### User Story P1 – Use schema-aware `command!` as the default write path
Narrative: A babar user wants the main write-command API to cover common insert, update, and delete operations without falling back to explicit codec tuples for ordinary statements.
Independent Test: Write supported insert, update, and delete statements with `command!` and confirm that non-`RETURNING` forms validate and execute as command-style no-row statements, while explicit-`RETURNING` forms validate and expose typed row-returning behavior.
Acceptance Scenarios:
1. Given a supported `INSERT`, `UPDATE`, or `DELETE` statement without `RETURNING`, When a user writes it with `command!`, Then the statement validates against authored schema facts and exposes the correct typed parameter behavior as a command-style no-row statement.
2. Given a supported write statement with explicit `RETURNING` columns, When the user compiles it with `command!`, Then the statement exposes typed row-returning behavior consistent with the declared returned columns by reusing the same query-shaped row contract as other row-returning typed SQL in this round.

### User Story P1 – Verify authored schema facts against a live database
Narrative: A babar user wants authored schema modules to remain the primary source of truth in code, while optional live verification confirms that the referenced schema facts used by a typed statement still match the real database.
Independent Test: Compile a typed statement with live verification enabled against a matching database and against a drifted database, and confirm success in the first case and a clear failure in the second.
Acceptance Scenarios:
1. Given live verification is configured and the authored schema matches the database, When a user compiles a supported typed statement, Then the statement is accepted without requiring a second schema description format.
2. Given live verification is configured and the authored schema disagrees with the database, When a user compiles a supported typed statement, Then compilation fails with diagnostics describing the mismatch.

### User Story P2 – Use broader common SQL types without losing typed behavior
Narrative: A babar user wants schema-aware typed SQL to work with the same common SQL types they already expect from babar's existing codec-driven query and command setup.
Independent Test: Use supported typed read and write statements involving the prioritized missing SQL types and confirm typed parameters and returned columns work for those statements.
Acceptance Scenarios:
1. Given a schema uses common non-primitive SQL types such as UUID, temporal values, JSON, or numeric values, When a user writes a supported typed statement using those fields, Then the statement compiles and exposes typed runtime behavior for both parameters and returned columns.
2. Given a type family is not yet supported by typed SQL in this round, When a user attempts to use it in a typed statement, Then the statement fails with a clear unsupported-subset diagnostic.

### User Story P2 – Keep explicit escape hatches for unsupported SQL
Narrative: A babar user accepts that typed SQL is intentionally scoped, but still needs a clear path for migrations, control-plane SQL, or advanced statements outside the supported typed subset.
Independent Test: Review the documented SQL execution surfaces and confirm there is a clear default path for supported typed SQL and a clear fallback path for unsupported cases.
Acceptance Scenarios:
1. Given a migration or control-plane operation, When a user chooses an execution surface, Then `simple_query_raw` remains available and documented for that purpose.
2. Given a user needs an advanced statement outside the supported typed subset, When they cannot use typed `query!` or `command!`, Then raw `Query::raw` or `Command::raw` remain available as advanced escape hatches.

### Edge Cases

- A typed statement is valid against authored schema modules but fails live verification because the real database has drifted.
- A write statement uses `RETURNING` and must expose row-returning typed behavior rather than command-only behavior.
- A user writes predicate-free `UPDATE` or `DELETE`, which is intentionally outside the supported subset for this round and should fail with an unsupported-subset diagnostic rather than silently acting as a broad table rewrite/delete.
- A supported statement uses one of the newly prioritized SQL types in both parameters and returned columns.
- A user attempts unsupported SQL constructs such as CTEs, subqueries, set operations, wildcard projections, `ON CONFLICT`, wildcard `RETURNING`, `INSERT ... SELECT`, or multi-statement batches.
- A query uses runtime-dependent optional behavior and users still expect prepare/streaming ergonomics to remain understandable.
- A user needs advanced SQL not supported by the typed subset and must choose between raw extended-query and simple-query escape hatches.
- A user still has an older explicit-codec `query!` or `command!` call site and needs a clear migration diagnostic rather than an opaque parser failure.

## Requirements

### Functional Requirements

- FR-001: `query!` must become the primary schema-aware typed read-query surface for supported row-returning statements in the first-round read subset: single-statement `SELECT` queries with explicit projections and the existing `FROM`, `JOIN`, `WHERE`, `ORDER BY`, `LIMIT`, and `OFFSET` coverage. (Stories: P1)
- FR-002: `command!` must become the primary schema-aware typed write surface for supported write statements in this round, with statements that do not use `RETURNING` remaining command-style no-row statements and statements with explicit `RETURNING` lowering to query-shaped row-returning behavior rather than a new row-returning command abstraction. (Stories: P1)
- FR-003: The primary typed SQL model must use authored schema modules as the main schema source for supported typed statements, and after repointing those modules must expose local `query!` and `command!` wrappers as the primary authored-schema call-site contract. (Stories: P1)
- FR-004: Inline schema declarations may remain available for small examples and tests, but they must not be the primary recommended user flow. (Stories: P1)
- FR-005: The main typed SQL path must support optional live verification that checks the referenced authored schema facts, referenced tables and columns, parameters, and returned or `RETURNING` columns for a typed statement against a live database. (Stories: P1)
- FR-006: The supported write subset for this round must include `INSERT ... VALUES ...`, `INSERT ... RETURNING <explicit columns>`, `UPDATE ... SET ... WHERE ...`, `UPDATE ... RETURNING <explicit columns> WHERE ...`, `DELETE ... WHERE ...`, and `DELETE ... RETURNING <explicit columns> WHERE ...`. Predicate-free `UPDATE` and predicate-free `DELETE` are out of scope in this round. (Stories: P1)
- FR-007: The typed SQL path for this round must explicitly reject unsupported constructs including `ON CONFLICT`, `INSERT ... SELECT`, `WITH` / CTEs, subqueries, `UPDATE ... FROM`, `DELETE ... USING`, predicate-free `UPDATE`, predicate-free `DELETE`, wildcard `RETURNING *`, and multi-statement batches. (Stories: P1,P2)
- FR-008: The typed SQL runtime must support the prioritized common SQL types `uuid`, `date`, `time`, `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric` for both parameters and returned columns when used in otherwise supported statements. (Stories: P2)
- FR-009: Unsupported SQL types or unsupported SQL constructs must fail with diagnostics that identify the unsupported part of the statement and preserve a clear fallback path. (Stories: P1,P2)
- FR-010: Raw `Query::raw` and `Command::raw` must remain available as advanced escape hatches for unsupported extended-protocol SQL. (Stories: P2)
- FR-011: `simple_query_raw` must remain available for migrations and other control-plane or script-style SQL that does not belong on the main typed SQL path. (Stories: P2)
- FR-012: Public documentation must reposition typed `query!` / `command!` as the default path, explain the supported typed SQL subset, explain live verification behavior, and explain when to use raw escape hatches. (Stories: P1,P2)
- FR-013: `sql!` must no longer be positioned as a primary long-term API direction and should begin phasing out in this round. (Stories: P2)
- FR-014: The typed SQL experience for supported statements must remain compatible with babar's ordinary execution model so users can still use normal query, execute, streaming, and preparation workflows where the statement shape permits it. (Stories: P1,P2)
- FR-015: Repointing public `query!` / `command!` must include a deliberate migration contract: old explicit-codec macro forms fail with targeted migration diagnostics in this round; the public macros remain the broader entrypoints; and `typed_query!` remains available only as a compatibility alias rather than the primary recommended path. (Stories: P1,P2)

### Key Entities

- Typed Read Statement: A supported row-returning SQL statement authored with `query!` against declared schema facts.
- Typed Write Statement: A supported write SQL statement authored with `command!`; without `RETURNING` it is a command-style no-row statement, and with explicit `RETURNING` it follows the same query-shaped row-returning contract as other row-returning typed SQL in this round.
- Authored Schema Module: A reusable in-code schema declaration that defines table, column, type, and semantic facts for typed SQL.
- Live Verification: Optional compile-time checking that compares the referenced authored schema expectations and statement expectations for one typed statement against a real database.
- Raw Extended-Protocol Escape Hatch: `Query::raw` or `Command::raw` used when a statement is outside the supported typed subset but still belongs on the extended-query path.
- Raw Control-Plane Escape Hatch: `simple_query_raw` used for migrations, multi-statement scripts, and control-plane SQL.

### Cross-Cutting / Non-Functional

- The default typed SQL path must remain SQL-first and readable rather than becoming a builder-centric abstraction.
- The supported typed SQL subset must be documented explicitly enough that unsupported-but-valid PostgreSQL features do not feel accidental.
- Verification failures must distinguish between authored-schema drift and statement-shape incompatibility clearly enough for users to correct the problem.
- Escape hatches must remain deliberate and understandable rather than reintroducing ambiguity about which API is the main path.

## Success Criteria

- SC-001: Users can author supported read statements with `query!` against authored schema modules without manually supplying explicit codec tuples for ordinary supported cases. (FR-001, FR-003)
- SC-002: Users can author supported insert, update, and delete statements with `command!` without falling back to the old explicit-codec macro model, with non-`RETURNING` forms staying command-style/no-row and explicit-`RETURNING` forms exposing query-shaped row-returning behavior. (FR-002, FR-006)
- SC-003: With live verification enabled, supported typed statements succeed when authored schema facts match the database and fail clearly when table, column, or type expectations drift. (FR-005)
- SC-004: Supported typed statements can use the prioritized SQL types `uuid`, `date`, `time`, `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric` for both parameters and returned columns in this round. (FR-008)
- SC-005: Unsupported SQL constructs and unsupported SQL types fail with clear diagnostics and documented fallback guidance rather than silent degradation. (FR-007, FR-009)
- SC-006: Public docs present schema-aware `query!` / `command!` as the default path, describe authored-schema local `query!` and `command!` wrappers as the primary authored-schema call-site contract, reserve `simple_query_raw` for migrations/control-plane work, describe raw `Query::raw` / `Command::raw` as advanced escape hatches, and explain the migration path away from the old explicit-codec macro forms while keeping `typed_query!` only as a compatibility alias. (FR-003, FR-010, FR-011, FR-012, FR-015)
- SC-007: Supported typed statements continue to fit babar's ordinary execution workflows, with clear expectations for which shape-dependent cases can and cannot be prepared or streamed in the first round. (FR-014)
- SC-008: Public docs, examples, and migration guidance no longer present `sql!` as a primary long-term API direction, and instead position it as a secondary or compatibility surface relative to schema-aware `query!` / `command!`. (FR-013)

## Assumptions

- The public macro names `query!` and `command!` are part of the desired user-facing destination even if implementation details or migration mechanics may need staged handling underneath.
- The first-round read subset intentionally preserves the current typed SQL read boundary rather than expanding immediately to the full PostgreSQL `SELECT` surface.
- Authored schema modules remain the primary schema model for typed SQL, while inline schema declarations stay available mainly for local examples and tests.
- After repointing, authored schema modules should be the most ergonomic place to call typed SQL through local `query!` / `command!` wrappers, while the public macros remain available for broader use.
- Migrations and typed DDL are intentionally separate concerns from this round's typed SQL unification work.
- Long-term type support should move toward parity with babar's broader codec-backed query and command ecosystem, even if this round prioritizes a specific missing subset first.
- Raw execution APIs remain available, but they are fallback surfaces rather than the default recommendation for ordinary supported application queries and commands.

## Scope

In Scope:
- Repositioning `query!` and `command!` around schema-aware typed SQL
- Authored-schema local `query!` and `command!` wrappers as the primary authored-schema UX after repointing
- Live verification for the main typed SQL path
- Typed read statements in the supported first-round read subset: single-statement `SELECT` queries with explicit projections plus the current `FROM`, `JOIN`, `WHERE`, `ORDER BY`, `LIMIT`, and `OFFSET` coverage
- Typed write statements in the agreed initial DML subset
- Support for the prioritized missing common SQL types in both parameters and returned columns
- Documentation and guidance for typed SQL defaults and raw escape hatches
- Beginning the phase-out of `sql!` as a primary API direction
- A deliberate migration contract for repointed `query!` / `command!`, including targeted diagnostics for old explicit-codec macro forms and compatibility positioning for `typed_query!`

Out of Scope:
- Typed DDL in this round
- `ON CONFLICT`
- `INSERT ... SELECT`
- `WITH` / CTEs
- Subqueries
- Set operations
- `UPDATE ... FROM`
- `DELETE ... USING`
- Predicate-free `UPDATE`
- Predicate-free `DELETE`
- Wildcard `SELECT *` projection support as part of the first-round typed read subset
- Wildcard `RETURNING *`
- Multi-statement batches
- Removing all raw execution APIs
- Treating `simple_query_raw` as the default query/command path

## Dependencies

- Existing schema-aware typed SQL parsing, normalization, resolution, and lowering work
- Existing authored schema module support
- Existing live verification/probe concepts that can be adapted into the typed SQL path
- Documentation surfaces that currently explain `query!`, `command!`, `sql!`, and typed SQL separately

## Risks & Mitigations

- Replacing the old macro behavior in place may create adoption friction: Mitigation: document the new default clearly, make authored-schema local `query!` / `command!` wrappers the primary authored-schema UX, keep explicit raw escape hatches, retain `typed_query!` only as a compatibility alias during this round, and require targeted migration diagnostics for old explicit-codec macro forms.
- Verification may become harder to reason about when authored schema facts and live database probing can disagree: Mitigation: require diagnostics that distinguish schema drift from unsupported statement shape.
- Expanding statement and type coverage in the same round may broaden implementation scope significantly: Mitigation: keep the SQL subset explicit and prioritize the agreed missing type set first.
- Unsupported PostgreSQL features may feel like regressions once `query!` / `command!` names are repointed: Mitigation: document the supported typed subset and preserve raw extended-query and simple-query escape hatches.
- Prepare/streaming expectations may become confusing for runtime-shape-dependent statements: Mitigation: make preparable versus non-preparable typed SQL behavior explicit in docs and planning, rather than implying uniform support.

## References

- Issue: none
- Work shaping: `.paw/work/typed-sql-unification/WorkShaping.md`
- Research: `.paw/work/typed-sql-unification/CodeResearch.md`
