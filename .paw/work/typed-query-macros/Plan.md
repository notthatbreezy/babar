# Plan: Typed Query Macros

## Problem

`babar` currently exposes SQL-first macros built around string literals plus
codec declarations:

- `sql!(...)` rewrites named placeholders into `Fragment<A>`
- `query!(...)` and `command!(...)` validate declared parameter/row codecs
  against a live database when configuration is present

That gives good runtime ergonomics and some live verification, but it does not
provide a **schema-aware, fully type-checked query language** at compile time.

The new goal is to design the **new primary querying direction for `babar`** as
a greenfield system that uses a PostgreSQL parser and schema symbols so callers
can write literal query syntax that is type-checked against:

- known tables / columns
- required parameters
- projected row shape
- basic query composition rules

Backwards compatibility is explicitly **not** a design constraint for this work.
The current architecture is useful context, but it should not constrain the new
design if cleaner foundations are available.

## Product Direction

Build the canonical typed-query stack around a **pure-Rust PostgreSQL parser
backend**, hidden behind the normalized IR boundary.

The parser decision is now:

- no `pg_parse`
- no FFI dependency on `libpg_query`
- keep the backend swappable behind normalization, but optimize the rest of the
  architecture for one strong pure-Rust implementation at a time

This choice is a deliberate trade:

- **Pro:** strongest long-term developer experience, simpler build story, easier
  cross-platform maintenance, and cleaner proc-macro integration
- **Con:** a pure-Rust parser may initially lag PostgreSQL grammar completeness,
  so the normalized v1 subset must remain intentionally narrow

This is not an experimental sidecar. Planning should assume the resulting
resolver/type-checker stack can become the long-term query story for `babar`.

## Constraints

- Treat this as greenfield: existing macro syntax, runtime query wrappers, and
  API compatibility do not need to be preserved.
- Favor **developer ergonomics** and **helpful compile errors** over perfect SQL
  coverage in the first iteration.
- Query composition must be part of the design, not bolted on later.
- It is acceptable to require **fully qualified names** such as `users.id`,
  `users.name`, etc. to simplify name resolution and diagnostics.
- Stay Postgres-specific and explicit rather than drifting into a generic ORM.
- Do not assume current `Fragment` / `Query` / `Command` shapes are the final
  target abstractions; replacing them is allowed if it materially improves the
  typed-query system.

## Most Promising Stack

### Parsing

- a pure-Rust PostgreSQL parser backend behind `parse_backend` + `normalize`
- proc-macro front-end using `proc_macro2`, `syn`, and `quote`

### Schema / symbol source of truth

Introduce a generated or handwritten Rust schema module that defines:

- tables
- columns
- column SQL types / nullability
- optional join / relation metadata later

The first pass should prefer **generated Rust symbols** over runtime-only
database introspection so the macro can resolve names without a live database.

### Type-checking layers

1. **Parser layer** — convert macro input into a parsed SQL tree
2. **Resolver layer** — resolve tables / columns against schema symbols
3. **Type-check layer** — validate expressions, predicate compatibility,
   placeholders, and projected row shapes
4. **Lowering layer** — generate either existing or replacement typed runtime
   statement values

### Macro ergonomics

Two possible macro surfaces:

1. **Literal token-style SQL macro**
   ```rust
   typed_sql!(SELECT users.id, users.name FROM users WHERE users.id = $id)
   ```
2. **Slightly more structured SQL-like macro**
   ```rust
   typed_sql!(
       SELECT users.id, users.name
       FROM users
       WHERE users.id = $id
   )
   ```

The plan should optimize for the second: it is still literal-feeling, but much
easier to produce good diagnostics and maintain stable parsing.

## Core Architectural Decision

Use **schema symbols plus qualified range bindings** as the baseline compromise.

That means:

- `users.id` is valid
- `u.id` is valid when `users AS u` introduced `u`
- bare `id` is rejected unless later explicitly supported
- aliases are explicit and required when the user wants a shorter binding name
- joins are in v1, but only with explicit `JOIN ... ON ...` predicates

This reduces ambiguity and makes both parsing and error reporting much simpler.

## Proposed Scope for v1

### In scope

- `SELECT ... FROM ... JOIN ... WHERE ... ORDER BY ... LIMIT ... OFFSET ...`
- explicit qualified columns through table or alias bindings
- explicit aliases only (`FROM users AS u`, never implicit output rewrites)
- placeholders / bound parameters
- typed projections into tuple/decoder-compatible shapes
- join predicate checking and outer-join nullability propagation
- direct composition with typed predicates or a typed query fragment layer

### Deferred

- `*` and `table.*`
- CTEs
- subqueries
- implicit aliases
- window functions
- aggregates
- casts, functions, and polymorphic/operator-resolution edge cases beyond a
  narrow supported set
- `USING`, `NATURAL JOIN`, join graph inference, and automatic relation
  discovery
- every PostgreSQL expression form

## Query Composition Strategy

Composition should not depend solely on string concatenation.

The most promising direction:

- parsed literal query macros for full statements
- typed predicate / clause values for reusable query pieces
- a composition layer that works at a semantic level rather than just text

Likely shape:

- full typed statement macro for end-to-end queries
- reusable predicate / ordering / clause symbols generated from schema-aware
  types
- lowering to a single checked internal statement representation

That preserves ergonomic literal queries while still enabling reusable filters
and safe query-tail composition.

## Error Reporting Principles

The system must feel helpful, not hostile.

Priorities:

- point at the exact token/span that failed
- say what the macro expected vs found
- reference schema symbols in diagnostics
- suggest qualified alternatives when names are ambiguous or missing
- explain unsupported SQL constructs clearly rather than silently degrading

Examples of desired errors:

- `unknown column users.nmae; did you mean users.name?`
- `column users.id has type int4 but placeholder $id was inferred/declared as text`
- `projection (users.id, users.name) does not match requested row type (i32,)`
- `bare column id is not allowed; use a fully qualified column such as users.id`

## Integration Strategy

### Phase 1: Architectural spike

- integrate a pure-Rust parser backend into the macros crate or a helper crate
- prove normalized-IR extraction for the narrow v1 subset
- verify compile-time build ergonomics and binary/build impact

### Phase 2: Schema symbol system

- define how schema metadata is authored or generated
- establish Rust symbol naming conventions
- make full qualification the default rule

### Phase 3: Resolver / type checker

- resolve tables and columns
- type-check predicates and placeholders
- validate projected row shape

### Phase 4: Lowering / runtime surface

- lower checked statements into existing runtime values or new greenfield typed
  statement/runtime types
- decide whether existing `sql!` / `query!` / `command!` stay as low-level APIs,
  become compatibility layers, or are superseded entirely

### Phase 5: Composition and ergonomics

- define reusable typed predicates / orderings
- add fun diagnostics and docs
- benchmark macro cost and dev-loop impact

## Risks

- a pure-Rust parser may not cover enough PostgreSQL syntax outside the chosen
  v1 subset
- parser AST fidelity may still leave significant normalization work to do
- build times may grow materially once schema codegen and checking are added
- compile errors could become noisy if span mapping is poor
- full SQL support is a trap; v1 must stay intentionally narrow

## Recommendation

Proceed with a **narrow, schema-aware, qualified v1**:

- parser: pure-Rust backend hidden behind normalized IR
- qualification rule: always require `binding.column`
- row typing: explicit and checked
- composition: typed predicates / clauses plus typed statement macro
- compatibility: greenfield, no effort spent preserving current macro syntax or
  the current runtime query API if a better design emerges

This gives the best chance of landing a useful, fun, productive system without
immediately getting buried in SQL completeness problems.

## Parser-Facing Abstraction (design-parser-abstraction)

### Stage boundary

Use a **Babar-owned normalized parse IR** as the only input to resolver,
type-check, and lowering passes.

```text
macro tokens
  -> SqlSource (canonical SQL + source map + placeholder/hook tables)
  -> parser backend AST (pure-Rust backend today, replaceable later)
  -> ParsedSelect v1   <-- stable internal boundary
  -> ResolvedSelect
  -> CheckedSelect
  -> LoweredQuery
```

- the chosen pure-Rust parser backend or any future backend is hidden behind
  `parse_backend` +
  `normalize`.
- `Resolved*` and later layers must **never depend on backend node types**.
- Do **not** introduce a public cross-crate `ParserBackend` trait yet; keep the
  abstraction as a crate-private module boundary until a second backend exists.

### Source and diagnostic model

The macro front-end should build a canonical SQL string plus a source map before
calling the parser:

```rust
pub struct SqlSource {
    pub canonical_sql: String,
    pub source_map: SourceMap,
    pub placeholders: PlaceholderTable,
    pub hooks: HookTable,
}

pub struct SourceSpan {
    pub start: u32, // byte offsets in canonical_sql
    pub end: u32,
}

pub struct Spanned<T> {
    pub span: SourceSpan,
    pub value: T,
}
```

- The parser-facing world uses **byte offsets into `canonical_sql`**.
- `SourceMap` translates `SourceSpan` back into proc-macro diagnostics.
- Every normalized node carries a `SourceSpan`; diagnostics should never need to
  inspect backend AST to recover a location.
- Keep parser errors and normalization errors in the same coordinate system.

### Node identity

Assign stable dense identities during normalization:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AstId(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ScopeId(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PlaceholderId(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct HookId(u32);
```

Use `AstId` for:

- attaching semantic facts in later passes without pointer identity
- cross-referencing diagnostics (`projection #2`, `ORDER BY item #1`, etc.)
- mapping backend parse nodes to Babar nodes during normalization

### Subset-normalized IR (post-parse, pre-resolution)

Normalization should already reject unsupported SQL and canonicalize the v1
subset into a small owned IR:

```rust
pub struct ParsedSelect {
    pub id: AstId,
    pub span: SourceSpan,
    pub projections: Vec<ParsedProjection>,
    pub from: ParsedFrom,
    pub joins: Vec<ParsedJoin>,
    pub filter: Option<ParsedExpr>,
    pub order_by: Vec<ParsedOrderBy>,
    pub limit: Option<ParsedLimit>,
    pub offset: Option<ParsedOffset>,
}

pub struct ParsedFrom {
    pub id: AstId,
    pub span: SourceSpan,
    pub table_name: ObjectNameSyntax,
    pub binding_name: BindingNameSyntax,
}

pub struct ParsedJoin {
    pub id: AstId,
    pub span: SourceSpan,
    pub kind: JoinKind,
    pub right: ParsedFrom,
    pub on: ParsedExpr,
}

pub struct ParsedProjection {
    pub id: AstId,
    pub span: SourceSpan,
    pub expr: ParsedExpr,
    pub output_name: OutputNameSyntax,
}

pub struct ParsedOrderBy {
    pub id: AstId,
    pub span: SourceSpan,
    pub expr: ParsedExpr,
    pub direction: OrderDirection,
    pub nulls: Option<NullsOrder>,
}

pub enum ParsedExpr {
    Column(ColumnRefSyntax),
    Placeholder(PlaceholderRef),
    Literal(Literal),
    Unary {
        id: AstId,
        span: SourceSpan,
        op: UnaryOp,
        expr: Box<ParsedExpr>,
    },
    Binary {
        id: AstId,
        span: SourceSpan,
        op: BinaryOp,
        left: Box<ParsedExpr>,
        right: Box<ParsedExpr>,
    },
    IsNull {
        id: AstId,
        span: SourceSpan,
        negated: bool,
        expr: Box<ParsedExpr>,
    },
    BoolChain {
        id: AstId,
        span: SourceSpan,
        op: BoolOp,
        terms: Vec<ParsedExpr>,
    },
    Hook(ExprHookRef),
}
```

Recommendations:

- Normalize `FROM users` into `table_name = users`, `binding_name = users`.
- Normalize `FROM users AS u` into `table_name = users`, `binding_name = u`.
- Flatten associative boolean chains (`a AND b AND c`) into `BoolChain`.
- Keep joins/order/limit/offset as top-level fields rather than a generic AST so
  later composition can target clause slots directly.
- Reject `*`, bare columns, subqueries, CTEs, aggregates, windows, and any
  unsupported expression form **during normalization**, not in resolver.

### Qualification handling

Treat qualification as a syntactic binding problem first, schema problem second.

```rust
pub struct ColumnRefSyntax {
    pub id: AstId,
    pub span: SourceSpan,
    pub qualifier: BindingNameSyntax,
    pub column: IdentSyntax,
}
```

- `qualifier` refers to the active range binding (`users` or `u`), not directly
  to a schema table symbol.
- Resolver maps `BindingNameSyntax -> ScopeEntry -> schema table`.
- This keeps alias support cheap without coupling the parser IR to schema IDs.
- Bare `id` never reaches resolver in v1; normalization emits a targeted error.

### Placeholders

Keep **named placeholders as the user-facing model** and treat parser ordinals as
an internal transport detail.

```rust
pub struct PlaceholderSpec {
    pub id: PlaceholderId,
    pub name: IdentSyntax,
    pub first_span: SourceSpan,
}

pub struct PlaceholderRef {
    pub id: AstId,
    pub span: SourceSpan,
    pub placeholder: PlaceholderId,
}
```

Recommended flow:

1. Scan macro input for `$name`.
2. Assign each distinct name a `PlaceholderId` and parser ordinal.
3. Rewrite canonical SQL to `$1`, `$2`, ... for the backend parser.
4. During normalization, convert parser `ParamRef(number)` back into
   `PlaceholderId`.

Consequences:

- repeated `$id` uses share one logical placeholder
- resolver/type-checker reason about logical names, not backend ordinals
- lowering still has stable positional metadata for runtime parameter encoding

### Projections

Make projections explicit and stable before resolution:

- preserve user order exactly
- give every output slot a single `output_name`
- infer `output_name` from the column name only for direct `table.column`
  projections
- require `AS alias` for any non-column projection form in v1

That yields a clean bridge into row-shape checking:

```rust
pub struct ResolvedProjection {
    pub id: AstId,
    pub output_name: OutputName,
    pub expr: ResolvedExpr,
    pub sql_type: SqlType,
    pub nullability: Nullability,
}
```

### Composition hooks

Do not make composition depend on reparsing concatenated SQL. Give the normalized
IR explicit hook nodes in clause positions we intend to support:

```rust
pub struct ExprHookRef {
    pub id: HookId,
    pub span: SourceSpan,
    pub kind: ExprHookKind, // Predicate initially
}
```

Recommended initial hook surface:

- `WHERE` predicate hooks
- `ORDER BY` item-list hooks

Do **not** support arbitrary statement-shape hooks in v1. Restricting hook kinds
keeps the resolver/type-checker simple and lets later reusable APIs target
semantic slots such as:

- `TypedPredicate<Scope>`
- `TypedOrder<Scope>`

### Resolver / type-check contracts

Once normalization succeeds, later stages should work against progressively more
semantic shapes:

```rust
pub struct ResolvedSelect {
    pub parsed: ParsedSelect,
    pub scopes: ScopeGraph,
    pub tables: Vec<ResolvedTableRef>,
    pub placeholders: Vec<ResolvedPlaceholder>,
}

pub struct CheckedSelect {
    pub resolved: ResolvedSelect,
    pub projections: Vec<CheckedProjection>,
    pub predicates: Vec<CheckedPredicate>,
    pub parameters: Vec<CheckedParameter>,
}
```

- **resolver owns names**: bindings, tables, columns, projection labels
- **type-check owns compatibility**: operator typing, nullability, placeholder
  constraints, projection row shape
- **lowering owns runtime layout**: final SQL text, placeholder order, output
  metadata, reusable prepared-statement keying

### Concrete recommendation

The cleanest long-term boundary is:

1. **parser-specific adapter**
2. **Babar-owned `ParsedSelect` subset IR**
3. **resolver/type-check/lowering layered only on that IR**

This keeps the parser choice replaceable, keeps the rest of the stack pure Rust,
and gives the typed-query pipeline a small, stable shape to build against.

## Resolver and Type-Checker Architecture (resolver-typechecker)

### Inputs from completed prior work

- **typed-sql-subset** fixes the supported v1 statement shape:
  `SELECT/FROM/JOIN/WHERE/ORDER BY/LIMIT/OFFSET`, qualified columns, explicit
  aliases only, no `*`, no subqueries/CTEs/aggregates/window functions.
- **schema-symbol-system** provides the schema catalog used by semantic passes:
  generated table symbols, qualified columns, SQL type metadata, and base
  nullability.
- **design-parser-abstraction** fixes the stage boundary at `ParsedSelect` and
  requires all later passes to depend only on normalized IR plus source spans.
- **resolve-pg-parse-toolchain** changes the parser direction: resolver and
  type-checker must stay backend-agnostic and assume a pure-Rust parser adapter.

### Semantic inputs and outputs

```rust
pub struct SchemaCatalog {
    pub tables_by_name: FxHashMap<ObjectName, TableId>,
    pub tables: Vec<TableSymbol>,
    pub columns: Vec<ColumnSymbol>,
}

pub struct TableSymbol {
    pub id: TableId,
    pub sql_name: ObjectName,
    pub columns_by_name: FxHashMap<Ident, ColumnId>,
}

pub struct ColumnSymbol {
    pub id: ColumnId,
    pub table: TableId,
    pub sql_name: Ident,
    pub sql_type: SqlType,
    pub nullable: bool,
}

pub struct ResolvedSelect {
    pub parsed: ParsedSelect,
    pub scope_graph: ScopeGraph,
    pub bindings: Vec<ResolvedBinding>,
    pub projections: Vec<ResolvedProjectionExpr>,
    pub filter: Option<ResolvedExpr>,
    pub joins: Vec<ResolvedJoin>,
    pub order_by: Vec<ResolvedOrderExpr>,
    pub limit: Option<ResolvedExpr>,
    pub offset: Option<ResolvedExpr>,
    pub placeholders: Vec<PlaceholderState>,
}

pub struct CheckedSelect {
    pub resolved: ResolvedSelect,
    pub row_env: RowEnv,
    pub projections: Vec<CheckedProjection>,
    pub parameters: Vec<CheckedParameter>,
    pub filter: Option<CheckedPredicate>,
    pub joins: Vec<CheckedJoin>,
    pub order_by: Vec<CheckedOrderExpr>,
    pub limit: Option<CheckedValueExpr>,
    pub offset: Option<CheckedValueExpr>,
}
```

Contracts:

- `ResolvedSelect` contains **all successful name binding** but may still contain
  unknown placeholder types.
- `CheckedSelect` contains **fully solved expression types**, clause legality,
  placeholder parameter metadata, and final projection row shape.
- lowering must not have to re-resolve names or guess types.

### Core semantic data structures

```rust
pub struct BindingId(u32);

pub struct ResolvedBinding {
    pub id: BindingId,
    pub scope: ScopeId,
    pub binding_name: BindingName,
    pub table: TableId,
    pub introduced_by: AstId,
}

pub struct BindingFact {
    pub binding: BindingId,
    pub table: TableId,
    pub nullability: Nullability,
}

pub struct ExprType {
    pub sql_type: SqlType,
    pub nullability: Nullability,
    pub kind: ExprKind, // Value | Predicate
}

pub enum Nullability {
    NonNull,
    Nullable(NullabilityCauseSet),
}

pub enum NullabilityCause {
    SchemaColumn,
    OuterJoin(BindingId),
    Expression,
}

pub struct PlaceholderState {
    pub id: PlaceholderId,
    pub name: IdentSyntax,
    pub inferred_type: Option<SqlType>,
    pub nullability: Nullability,
    pub uses: Vec<PlaceholderUse>,
    pub pending: Vec<PendingConstraint>,
}
```

The important split is:

- `ResolvedBinding` answers **what object does this name refer to?**
- `BindingFact` answers **what nullability does that binding currently expose in
  this clause environment?**
- `ExprType` answers **what does this expression evaluate to?**

### Pass pipeline

The resolver/type-checker should run as five explicit passes over
`ParsedSelect`.

#### Pass 1: range binding collection

Input: `ParsedFrom`, `ParsedJoin.right`, `SchemaCatalog`

Output: `ScopeGraph`, `ResolvedBinding`

Algorithm:

1. Resolve each `table_name` against `SchemaCatalog.tables_by_name`.
2. Create one `BindingId` per `FROM` item / joined relation using
   `binding_name`.
3. Reject duplicate binding names in the same `SELECT` scope.
4. Record `introduced_by: AstId` for diagnostics (`binding u already defined by
   FROM users AS u`).

v1 scope rule is intentionally simple:

- one statement scope per `ParsedSelect`
- no nested scopes because subqueries/CTEs are deferred
- aliases shadow nothing because bare columns are rejected

#### Pass 2: column and hook resolution

Input: `ParsedExpr`, `ScopeGraph`, `ResolvedBinding`, `SchemaCatalog`, `HookTable`

Output: `ResolvedExpr`

Algorithm:

1. For `ColumnRefSyntax { qualifier, column }`:
   - resolve `qualifier -> BindingId`
   - look up `column` in that binding's table symbol
   - produce `ResolvedColumnRef { binding, column: ColumnId, span }`
2. For placeholders:
   - intern/lookup `PlaceholderState` by logical `PlaceholderId`
   - append a `PlaceholderUse`
3. For hooks:
   - look up the hook payload by `HookId`
   - verify hook kind matches the clause slot (`Predicate` for `WHERE` / `ON`,
     `OrderList` for `ORDER BY`)
   - verify the hook's required bindings can be mapped into the current scope
4. For projection output names:
   - direct column projections inherit the column name
   - non-column projections must already carry explicit `AS alias` from
     normalization

Resolver does **not** decide whether `users.id = $id` is legal; it only ensures
that `users.id` and `$id` are well-formed semantic references.

#### Pass 3: join-shape planning

Input: `ResolvedSelect.bindings`, `ResolvedJoin`

Output: left-to-right `RowEnv` snapshots

Algorithm:

1. Start with the base `FROM` binding as `NonNull` except for schema-nullable
   columns.
2. Fold joins left-to-right.
3. For each join, build two environments:
   - `on_env`: the left input plus the newly introduced right binding, before
     null-extension
   - `output_env`: the post-join environment exposed to later clauses
4. Apply join null-extension to `output_env`:
   - `INNER`: no new binding-level nullability
   - `LEFT`: mark all columns from the right binding as
     `Nullable(OuterJoin(right))`
   - `RIGHT`: mark all columns from the accumulated left side as
     `Nullable(OuterJoin(left_binding))`
   - `FULL`: mark both sides nullable

This separation matters because `ON` is checked against `on_env`, while `WHERE`,
projections, and later joins see `output_env`.

#### Pass 4: expression typing and constraint collection

Input: resolved clause expressions plus the appropriate `RowEnv`

Output: `CheckedExpr` plus placeholder constraints

Each expression visit returns an `ExprType` and may update the shared
`InferenceContext`.

v1 expression rules:

- `Column` => type/nullability from schema column + binding-level nullability in
  the current `RowEnv`
- `Literal` => exact literal type chosen during normalization
- `Placeholder` => unresolved until constrained by surrounding context
- comparison operators (`=`, `<>`, `<`, `<=`, `>`, `>=`) require comparable
  operand types and produce `Predicate`
- boolean chains (`AND`, `OR`, `NOT`) require predicate operands
- `IS NULL` / `IS NOT NULL` accept any scalar operand and always produce
  `bool NOT NULL`
- `LIMIT` / `OFFSET` require `int8`-compatible expressions

Deferred from v1:

- implicit casts and general PostgreSQL coercion search
- function/operator overloading
- arithmetic and text concatenation inference beyond whatever normalization
  explicitly lowers into typed literal forms
- aggregate/window semantics

#### Pass 5: placeholder solving and row-shape finalization

Input: `InferenceContext`, checked projections

Output: `CheckedParameter`, final `CheckedProjection`

Placeholder solving algorithm:

1. Each placeholder starts with `inferred_type = None`.
2. When a checked context demands a concrete type (for example `users.id = $id`
   or `LIMIT $limit`), call `expect_type(placeholder, expected_type, span)`.
3. If a comparison relates two placeholders or another still-unsolved operand,
   store a `PendingConstraint`.
4. After all clauses are visited, iterate pending constraints to a fixpoint:
   - if one side became concrete, push that type to the other side
   - if both sides are concrete, verify compatibility
5. Error if any placeholder remains unsolved or receives incompatible demands.

Projection finalization:

- preserve projection order
- compute `ExprType` for each projection expression
- build `CheckedProjection { ordinal, output_name, sql_type, nullability }`
- reject duplicate output names only for row targets that require unique field
  names; tuple/positional decoding can keep duplicates

### Clause-by-clause contracts

#### Table and alias scope

- `FROM users` introduces binding `users`
- `FROM users AS u` introduces binding `u`; `users.id` is no longer valid in that
  scope unless `users` was separately bound
- each binding name must be unique within the statement
- every qualified column reference must resolve through a visible binding, never
  directly through a table symbol lookup

#### Qualified column resolution

- only `binding.column` reaches resolver in v1
- missing binding => `unknown table/alias binding`
- missing column => `unknown column on bound table`
- ambiguity does not exist in v1 because bare columns are normalized out as
  errors

#### Placeholder typing

- each logical placeholder name corresponds to exactly one parameter slot
- all uses of the same placeholder must solve to one SQL type
- a placeholder can gain nullability from outer-join comparisons, but its
  declared parameter type remains the solved scalar SQL type
- placeholders with no constraining context are rejected in v1 instead of
  defaulting to `text` or `unknown`

#### Predicate typing

- `WHERE` and `JOIN ... ON ...` must resolve to `ExprKind::Predicate`
- nullable predicates are allowed because PostgreSQL uses three-valued logic
- non-predicate scalar expressions in predicate positions are rejected
- `IS NULL` is the only null-test operator in v1; `IS DISTINCT FROM` is deferred

#### Join compatibility

v1 join checking means:

- joined relation exists and binds uniquely
- `ON` expression resolves and type-checks as a predicate
- operand compatibility inside `ON` comparisons is checked using the same rules
  as `WHERE`
- outer joins update downstream binding nullability

Deferred:

- foreign-key-aware join suggestions
- `USING`, `NATURAL JOIN`, lateral references, and relation inference

#### Projection typing

- every projection gets a stable ordinal and output name
- direct qualified-column projections inherit the column name
- computed expressions require `AS alias`
- row-shape metadata is fully known after type-check even before lowering

#### Nullability propagation

Nullability should be computed as:

- base column nullability comes from schema metadata
- binding-level outer-join nullability widens every column under that binding
- comparison operators return nullable `bool` if either side may be null
- boolean composition returns nullable `bool` if any term may be null
- `IS NULL` / `IS NOT NULL` always return `NOT NULL`
- projection nullability is exactly the checked expression nullability

This keeps diagnostics precise:

- `posts.title is nullable because posts came from LEFT JOIN`
- `users.deleted_at is nullable because schema marks it nullable`

#### Composition interactions

Composition must splice **semantic fragments**, not raw SQL strings.

Recommended contract:

```rust
pub struct PredicateFragment {
    pub required_bindings: Vec<BindingRequirement>,
    pub checked_expr: CheckedExpr,
    pub parameters: Vec<CheckedParameter>,
}
```

Rules:

- `WHERE {predicate}` and `JOIN ... ON {predicate}` hooks accept only
  `PredicateFragment`
- `ORDER BY {order}` hooks accept only `OrderFragment`
- hook resolution remaps fragment-local bindings onto statement-local
  `BindingId`s, then imports the fragment's already checked expression tree
- fragment parameters merge into the outer query by logical placeholder name; a
  collision is legal only when the solved SQL type matches

This gives composition strong guarantees:

- fragments cannot reference out-of-scope bindings
- fragments carry their own checked parameter contracts
- the statement type-checker only needs remapping + compatibility checks, not
  reparsing

### What v1 type-checks vs what is deferred

Type-checked in v1:

- table existence
- explicit alias/binding legality
- qualified column existence
- placeholder inference from comparison/projection/limit-offset contexts
- predicate/operator compatibility for the normalized subset
- join `ON` predicate legality
- projection output type + nullability
- outer-join nullability propagation
- composition fragment scope + parameter compatibility

Deferred:

- implicit casts and full PostgreSQL coercion behavior
- functions, aggregates, windows, and polymorphic operators
- subquery scope/correlation
- alias references by ordinal or unqualified output name in arbitrary clauses
- relation inference / automatic join discovery
- any parser-specific AST tricks beyond normalization

## Lowering and Runtime API Design (lowering-runtime-api)

### Assessment of the current runtime surface

Today's runtime query layer is useful context, but it is not the right canonical
target for the schema-aware query system:

- `Fragment<A>` is fundamentally a **string/placeholder builder** with runtime
  encoder composition
- `Query<A, B>` and `Command<A>` split the API by "returns rows vs not" but both
  are still thin wrappers around `Fragment`
- `PreparedQuery<A, B>` and `PreparedCommand<A>` duplicate the same split again
- `Session`, `Transaction`, and `Savepoint` repeat nearly identical query APIs

That model fits v0.x, but it is the wrong center of gravity for the future typed
query stack because the new system already knows statement kind, parameter
layout, row layout, and source origin **at compile time**.

### Recommendation

Replace `Fragment` / `Query` / `Command` as the **canonical public query model**
with a single typed statement model. Keep the old types only as:

- short-term compatibility shims while existing code migrates, or
- lower-level dynamic/manual APIs for callers intentionally opting out of the
  typed query pipeline

They should not remain the primary architecture.

### Lowering target

Lowering should not emit ad-hoc runtime strings plus trait objects. It should
emit a **static statement descriptor** plus generated bind/decode code.

```rust
pub struct StatementPlan {
    pub sql: &'static str,
    pub origin: Option<Origin>,
    pub kind: StatementKind,
    pub fingerprint: StatementFingerprint,
    pub params: &'static [ParamSpec],
    pub columns: &'static [ColumnSpec],
}

pub enum StatementKind {
    Query,
    Command,
}

pub struct ParamSpec {
    pub logical_name: &'static str,
    pub position: u16,
    pub sql_type: SqlType,
    pub nullable: bool,
}

pub struct ColumnSpec {
    pub label: &'static str,
    pub sql_type: SqlType,
    pub nullable: bool,
}
```

The macro then generates a zero-sized statement type that points at this plan:

```rust
pub trait Statement {
    type Params;
    const PLAN: &'static StatementPlan;

    fn encode_params(
        params: &Self::Params,
        out: &mut EncodedParams,
    ) -> Result<()>;
}

pub trait QueryStatement: Statement {
    type Row;

    fn decode_row(row: &[Option<Bytes>]) -> Result<Self::Row>;
}

pub trait CommandStatement: Statement {}
```

This keeps the runtime representation:

- `'static`
- allocation-free at statement-definition time
- free of `Arc<dyn Encoder<_>>` / `Arc<dyn Decoder<_>>` in the canonical path
- directly keyable for prepare-cache reuse

### Lowering pipeline

The clean lowering path from checked IR is:

```text
CheckedSelect
  + imported semantic fragments (predicate/order/etc.)
  -> FinalStatementLayout
  -> generated StatementPlan + generated Statement impl
```

Concretely:

1. **Import composition fragments before final lowering**
   - `PredicateFragment`, `OrderFragment`, etc. stay semantic-only
   - outer lowering remaps binding ids, checks compatibility, and merges their
     checked parameter sets
2. **Assign final parameter slots**
   - merge placeholders by logical name
   - preserve a deterministic left-to-right slot order in the fully composed
     statement
   - record both logical placeholder name and final positional index
3. **Render canonical SQL**
   - output PostgreSQL SQL text with only positional `$1`, `$2`, ... placeholders
   - no runtime placeholder renumbering in the typed path
4. **Freeze row metadata**
   - every projected column gets stable label, SQL type, and nullability
5. **Generate bind/decode code**
   - param encoding code reads `&Params`
   - row decoding code constructs `Row`
   - both are generated from the checked/lowered layout, not rebuilt through
     runtime fragment composition

This means lowering is the only stage that decides runtime placeholder order and
the only stage that turns semantic fragments into executable SQL.

### Canonical caller-facing runtime API

The long-term caller API should be statement-centric and executor-centric:

```rust
pub trait Executor {
    async fn execute<S>(&self, stmt: S, params: &S::Params) -> Result<u64>
    where
        S: CommandStatement;

    async fn fetch_all<S>(&self, stmt: S, params: &S::Params) -> Result<Vec<S::Row>>
    where
        S: QueryStatement;

    async fn fetch_optional<S>(&self, stmt: S, params: &S::Params) -> Result<Option<S::Row>>
    where
        S: QueryStatement;

    async fn fetch_one<S>(&self, stmt: S, params: &S::Params) -> Result<S::Row>
    where
        S: QueryStatement;

    async fn stream<S>(&self, stmt: S, params: &S::Params) -> Result<RowStream<S::Row>>
    where
        S: QueryStatement;

    async fn prepare<S>(&self, stmt: S) -> Result<Prepared<'_, S>>
    where
        S: Statement;
}
```

`Session`, `Transaction<'_>`, and `Savepoint<'_>` should all expose this same
surface, ideally through one sealed/internal `Executor` trait rather than three
copies of the same inherent methods.

### Ownership and lifetime model

- **statement definitions are `'static`**
  - macro-generated statement types are ZST markers or constant values
  - `StatementPlan` lives for the entire program
- **parameter values are borrowed**
  - execution/prepare APIs take `&S::Params`
  - generated binders read from shared references and encode immediately
  - this avoids forcing moves/clones for repeated execution
- **decoded rows are owned**
  - `S::Row` is constructed per row and returned by value
- **prepared statements are executor-bound**
  - `Prepared<'e, S>` is tied to the `Session` / transaction/savepoint scope that
    created it
  - it should not be `Clone`
  - dropping it should release the checked-out prepared handle back to the
    connection-local cache

Suggested prepared shape:

```rust
pub struct Prepared<'e, S: Statement> {
    session: &'e Session,
    statement_name: SmolStr,
    _statement: PhantomData<S>,
}
```

The implementation may still store a sender/cache handle internally rather than a
literal `&Session`, but the API contract should communicate that a prepared
statement is bound to one executor lifetime.

### Raw vs prepared execution

The statement model should support two execution modes from the same lowered
plan:

1. **default execution** — unnamed extended-protocol execution each call
2. **prepared execution** — explicit `prepare` yielding `Prepared<'_, S>`

Preparation should:

- resolve any dynamic PostgreSQL type OIDs needed by the lowered plan
- send `Parse` once per connection-local cache entry
- validate returned row description against `StatementPlan.columns`
- cache by canonical statement identity

The prepare cache key should be based on the lowered statement identity, not on
runtime string builders. Recommended key material:

- statement fingerprint
- canonical SQL text
- resolved parameter type OIDs

For callers, prepared use stays simple:

```rust
let prepared = session.prepare(GetUserById).await?;
let row = prepared.fetch_one(&GetUserByIdParams { id: 42 }).await?;
```

### Composition interaction

Composition should terminate **before runtime**.

- `PredicateFragment` / `OrderFragment` / future clause fragments are
  compile-time semantic values, not executable runtime statements
- they lower into checked expression/clause trees plus parameter contracts
- the outer statement imports them, resolves final placeholder layout, and emits
  one final `StatementPlan`

Implication: the canonical runtime API does **not** need a public `Fragment`-like
builder for typed composition. If Babar wants a low-level dynamic SQL builder, it
should live separately as an explicit escape hatch, not as the heart of the typed
query story.

### Fate of current abstractions

- **`Fragment`**: remove from the canonical path; keep only as a low-level
  dynamic/manual API if still useful, or make it crate-private once legacy macros
  are gone
- **`Query` / `Command`**: superseded by the single statement model; at most keep
  as temporary wrappers around generated `StatementPlan`
- **`PreparedQuery` / `PreparedCommand`**: replace with one `Prepared<'_, S>`
- **`sql!` / `query!` / `command!`**: either deprecate or reposition as explicit
  low-level compatibility layers; the new typed query macros should target the
  new statement model directly
- **`simple_query_raw`**: keep as an intentionally untyped escape hatch for
  migrations, admin SQL, and tooling

### Architectural conclusion

The future Babar query stack should compile:

- parsed SQL
- schema resolution
- type-checked placeholders/projections
- semantic composition fragments

into a **single static typed statement descriptor** with generated bind/decode
code and one unified executor API. The old `Fragment`/`Query`/`Command` family is
best treated as legacy scaffolding, not the long-term runtime center.

## Parser Conformance and Testing Strategy (parser-conformance-strategy)

Trust in the pure-Rust parser should come from **layered conformance**, not from
one giant golden file dump. The strategy should deliberately separate:

1. **parser correctness** — does raw SQL parse or fail where PostgreSQL does?
2. **normalized-IR correctness** — does Babar lower supported SQL into the same
   `ParsedSelect` shape regardless of backend?
3. **semantic correctness** — does resolver/type-checking reject unsupported or
   ill-typed queries with helpful diagnostics?
4. **macro correctness** — do end users see accurate spans, errors, and runtime
   behavior?

### External assets we can realistically use

- **`pg_query.rs` / `libpg_query`** should be the **primary parser oracle** in
  tests. They are the most up-to-date Rust/C wrappers around PostgreSQL's own
  parser and already ship substantial `parse`, `normalize`, fingerprint, and
  parallel-safety tests. Babar should mirror their SQL inputs **1:1** where
  practical.
- **`pg_parse`** remains useful as a **secondary corpus source** because it has a
  Rust-friendly test layout (`parse_tests.rs`, `str_tests.rs`,
  `normalize_tests.rs`, `tests/data/sql/*.sql`, `tests/data/tree/*.json`). It
  is especially helpful for continuity with the previous parser direction, but
  should not be the main oracle once we are pure-Rust.
- **PostgreSQL regression tests** (`src/test/regress/sql/*.sql`,
  `src/test/regress/expected/*.out`, especially files like `select.sql` and
  `errors.sql`) are the best long-lived source of real PostgreSQL edge cases.
  They are excellent **corpus inputs**, but live Postgres is not a pure parser
  oracle because the server also performs parse analysis and catalog lookup.
- **`sqlparser-rs`** is useful as a **corpus and round-trip inspiration source**
  (large PostgreSQL dialect suite plus TPCH queries), but not as the authority
  for PostgreSQL compatibility because it intentionally performs syntax parsing
  without PostgreSQL semantic analysis and explicitly documents that it accepts
  queries engines may reject.

### Core testing rule

Do **not** make the pure-Rust parser match `pg_query.rs`'s Rust AST or
`libpg_query`'s JSON tree byte-for-byte. Instead:

- compare **parse success/failure** against the oracle parser,
- compare **cursor/error positions** and coarse error category,
- and, for the supported v1 subset, normalize both parser outputs into
  Babar-owned `ParsedSelect` / `ParsedExpr` IR and compare **that**.

That keeps the oracle powerful without coupling Babar's implementation to a
foreign AST shape.

### Required test layers

#### 1. Golden parser cases

Maintain a curated checked-in suite for the v1 subset and known bug shapes:

- single-table `SELECT ... FROM ... WHERE ...`
- joins and alias binding syntax
- placeholder forms and repeated placeholders
- literals, boolean chains, unary/binary operators, `IS NULL`
- `ORDER BY`, `LIMIT`, `OFFSET`
- identifier quoting, comments, whitespace, and keyword edge cases

Each case should record:

- SQL source
- expected parse result (`ok` / `syntax_error`)
- expected top-level statement kind(s)
- expected key spans/cursor positions when relevant

These are the fast, human-readable regression tests we update intentionally.

#### 2. Differential parser tests against PostgreSQL-derived oracles

Add a dev/test-only oracle path using `pg_query.rs` (or a tiny helper over
`libpg_query` if needed). For every corpus item:

- parse with the pure-Rust parser
- parse with the oracle parser
- assert both either succeed or fail
- when both fail, compare error cursor position and broad category
- when both succeed, compare statement count and statement kind sequence

For the supported v1 query subset, also run **dual normalization**:

- `pure_rust_ast -> ParsedSelect`
- `pg_query_ast -> ParsedSelect`

and require exact IR equality.

This is the highest-confidence test in the system because it proves Babar's
own boundary is backend-independent.

#### 3. Corpus tests

Build a checked-in corpus directory with explicit provenance labels:

- `postgres-regress/` — harvested query files and negative cases from upstream
  PostgreSQL regression tests
- `pg-query/` — mirrored `pg_query.rs` / `libpg_query` parse and normalize cases
- `pg-parse/` — mirrored `pg_parse` cases
- `sqlparser-postgres/` — selected PostgreSQL-dialect and TPCH queries from
  `sqlparser-rs`
- `babar-real/` — queries from docs, examples, integration tests, and future
  typed-query fixtures

Every corpus item should be classified as one of:

- `parse-ok-supported`
- `parse-ok-unsupported`
- `syntax-error`

This prevents accidental conflation of "the parser failed" with "Babar
intentionally rejects this during normalization."

#### 4. Negative and unsupported-feature tests

Maintain separate tests for:

- raw syntax errors (unterminated literals, bad keywords, malformed operators)
- unsupported but syntactically valid PostgreSQL (`WITH`, subqueries, `*`,
  aggregates, windows, etc. until implemented)
- v1-specific policy errors such as bare column names where qualification is
  required
- placeholder misuse, alias binding errors, duplicate output names, and invalid
  hook placement

These should assert the **stage** that rejects the query (`parse`,
`normalize`, `resolve`, `typecheck`) and the exact highlighted span.

#### 5. Span and diagnostic tests

Source-map fidelity is a first-class requirement. Add tests that assert:

- parser cursor positions map back to the correct proc-macro token span
- multi-line SQL and comments preserve useful highlight locations
- placeholder rewriting (`$name -> $1`) still reports diagnostics at the
  original `$name`
- normalization/typecheck errors point at the smallest useful source span

Use `.stderr`-style golden diagnostics for proc-macro/UI coverage, following the
existing `trybuild` pattern already used in `crates/core/tests/ui/`.

#### 6. Normalized IR tests

The Babar-owned normalized IR is the architectural seam and needs its own
snapshots. For supported inputs, serialize `ParsedSelect` in a stable text form
(JSON/RON/debug snapshot) and assert:

- binding names and aliases are canonicalized correctly
- repeated placeholders share one logical placeholder id
- boolean chains are flattened deterministically
- output names are inferred only where allowed
- unsupported syntax never leaks past normalization

These tests should be small, deterministic, and independent of the runtime.

#### 7. Resolver / type-check tests

Above normalization, maintain semantic tests that use a fixed schema fixture and
assert:

- table/column resolution
- nullability propagation
- operator compatibility
- placeholder type constraints across repeated uses
- projection shape checking
- hook/type composition contracts

These tests should use hand-authored schema fixtures so failures isolate the
checker rather than the parser.

#### 8. End-to-end macro tests

Keep a dedicated end-to-end suite that compiles actual typed-query macros and
verifies:

- pass cases generate the expected runtime shape
- fail cases emit the expected compile diagnostics
- live verification against PostgreSQL still works once schema-aware checks are
  enabled
- runtime execution against a temp PostgreSQL instance returns the expected rows

The existing `trybuild` + Docker-backed integration pattern in `crates/core`
should be reused rather than inventing a second macro test harness.

#### 9. Fuzz and generated-query testing

After the deterministic corpus is stable, add scheduled fuzzing / generated
query checks:

- grammar-aware generation for the supported subset
- minimization of parser crashes / infinite loops / exponential blowups
- differential execution against `pg_query.rs` for parse-ok vs parse-error
  outcomes

This should be a scheduled or manually-invoked confidence job, not the first
required PR gate.

### CI expectations

#### Required on every PR

- fast parser unit/golden/negative/span tests
- normalized-IR snapshot tests
- resolver/type-check tests
- macro UI / `.stderr` tests
- targeted differential tests against `pg_query.rs` for the curated supported
  corpus
- existing workspace verification (`cargo test --all-features`) once the new
  parser path is integrated into the main crates

#### Required on PRs touching parser, normalization, or diagnostics

- full differential corpus run against `pg_query.rs`
- end-to-end typed macro tests
- Docker-backed PostgreSQL integration tests covering schema-aware queries and
  diagnostics

#### Scheduled / nightly

- expanded corpus sweep across mirrored PostgreSQL / `pg_query.rs` /
  `pg_parse` / `sqlparser-rs` cases
- multi-version PostgreSQL behavior checks for end-to-end macro flows
- fuzz / generated-query differential jobs
- drift check that reports when upstream `pg_query.rs` / `libpg_query` corpus
  changes should be imported

### Practical recommendation

The minimum viable confidence bar for trusting the pure-Rust parser is:

1. a **checked-in curated corpus**,
2. a **`pg_query.rs` differential suite**,
3. **normalized-IR snapshots**,
4. **trybuild diagnostic goldens**, and
5. **live PostgreSQL macro/integration coverage**.

If any one of those is missing, the new query direction will be too easy to
break at the edges. If all five are present, Babar can move forward with a
pure-Rust parser while still staying anchored to real PostgreSQL behavior.

## Diagnostics UX (diagnostics-ux)

### Design goal

The typed-query macro should fail like a very sharp pair-programmer:

- **precise** — highlight the smallest useful span
- **actionable** — say what was expected, what was found, and what to do next
- **stage-aware internally, stage-neutral externally** — tests/assertions track
  whether rejection happened in parse/normalize/resolve/typecheck, but end-user
  messages should talk about the SQL problem rather than implementation phases
- **no silent fallback** — typed macros never degrade into unchecked SQL; if the
  query is outside the supported subset, the diagnostic should say so explicitly
- **friendly** — primary messages stay short and factual, while `help:` / `note:`
  text provides the extra coaching

### Diagnostic envelope

Every emitted diagnostic should be built from the same logical shape:

1. **primary message** — one sentence naming the problem
2. **primary span** — exact token, keyword, placeholder, or hook that should be
   looked at first
3. **optional context** — expected vs found, solved SQL type, required binding,
   supported subset rule, etc.
4. **optional help** — concrete rewrite or nearest valid symbol
5. **optional related location** — only when the second location materially
   explains the error

On stable Rust, proc macros cannot rely on rich structured rustc suggestions as
their main UX. v1 should therefore assume:

- one high-quality primary spanned error is the baseline
- `help:` / `note:` content is embedded in the rendered message text
- additional spans are used sparingly, because multiple `compile_error!`
  emissions read as multiple errors rather than a single polished multi-span
  diagnostic

If a richer diagnostic API becomes viable later, it can improve rendering, but
the semantic diagnostic model should not depend on unstable compiler features.

### Concrete diagnostic categories

| Category | Produced by | Primary span | Required content |
| --- | --- | --- | --- |
| Parse error | parser backend | offending token or parser cursor | what token/construct was expected |
| Unsupported syntax | normalization | clause keyword / unsupported operator / wildcard token | explicitly say the construct is outside the supported subset |
| Qualification policy error | normalization | bare identifier | explain the fully-qualified rule and suggest valid bindings |
| Binding resolution error | resolver | missing alias/table segment | show the missing binding and visible bindings when useful |
| Column resolution error | resolver | missing column segment | show bound relation and nearest schema matches |
| Placeholder contract error | typecheck | placeholder occurrence | solved/expected type or why inference failed |
| Expression type error | typecheck | operator or offending operand | expected expression kind vs actual SQL type |
| Projection/row-shape error | typecheck / lowering boundary | projection item or row type argument | actual row shape vs requested row shape |
| Composition contract error | hook import / typecheck | hook insertion site | required fragment kind/bindings/parameter contract |
| Internal bug guardrail | any stage | macro invocation span | apologize briefly and ask user to file an issue; never dump raw backend internals |

### Span strategy

Build diagnostics entirely on top of `SqlSource { canonical_sql, source_map, .. }`
and `SourceSpan`; later passes should never recover locations from parser-owned
AST nodes.

#### Span mapping rules

1. **Canonical coordinates first**  
   All parse, normalize, resolve, and typecheck errors are reported in canonical
   SQL byte offsets.

2. **SourceMap is segment-based**  
   `SourceMap` should record the mapping from canonical SQL ranges back to:
   - the originating string literal token span,
   - placeholder token span (`$id`, not rewritten `$1`), and
   - hook expression span (`{predicate}` insertion site).

3. **Use the smallest useful span**  
   - unknown binding: highlight `u` in `u.id`
   - unknown column: highlight `nmae` in `users.nmae`
   - unsupported `WITH`: highlight the `WITH` keyword
   - type mismatch in `users.id = $name`: highlight `$name`
   - wrong hook kind in `WHERE {order_by}`: highlight `{order_by}`

4. **Zero-width parser cursors become anchored spans**  
   Parser failures often produce a cursor position between bytes. Map that to:
   - the next token span when possible,
   - otherwise the previous token span,
   - never the whole macro invocation unless no finer location exists.

5. **Cross-token ranges are collapsed deliberately**  
   If an error spans multiple source fragments, choose one actionable primary
   span and mention the larger construct in text instead of highlighting the
   whole statement.

#### Related-location policy

Use a secondary location only for:

- duplicate alias / duplicate output name (`first defined here`)
- repeated placeholder contract conflicts (`first constrained here`)
- composition fragments when the imported fragment has a tracked origin worth
  showing

All other cases should stay single-span to keep output calm.

### Notes, help, and suggestions

Suggestions should be **textual**, not machine-applicable edits. Favor no more
than two help lines.

#### Notes

Use `note:` for durable facts that explain *why* the macro rejected the query:

- `note: v1 requires fully-qualified column references`
- `note: LEFT JOIN makes posts.title nullable downstream`
- `note: placeholder $id must have one SQL type across all uses`

#### Help

Use `help:` for concrete next actions:

- rewrite to a qualified column (`users.id`)
- add an explicit `AS alias`
- change a placeholder type / codec
- move an unsupported fragment to the lower-level runtime SQL API

#### Suggestions policy

Only suggest something when confidence is high:

- Levenshtein-style near match on a column within the already bound relation
- one visible binding is an obvious qualifier for a bare column
- one deferred-but-known alternative rewrite exists (`COUNT(*)` -> not suggested;
  too semantic)

Do **not** guess across multiple plausible schema symbols.

### Unsupported-feature messaging

Unsupported-feature diagnostics should follow a strict template:

> `<construct>` is not supported by `query!` yet

Then add:

- the highlighted unsupported span,
- one sentence saying the supported subset rule being violated,
- one `help:` line with the nearest supported rewrite or escape hatch.

Examples:

- `WITH` / subqueries / `SELECT *` / aggregates / windows / `USING` joins
- unsupported operators or expression forms that parse but normalize cannot
  lower into the v1 IR

Fallback rule: **typed macros reject unsupported constructs outright**. They do
not silently bypass checking. If an escape hatch exists, name it directly in the
help text; otherwise say the feature is deferred.

### Composition-specific diagnostics

Composition errors must talk about the semantic contract, not string splicing.

#### Required composition error cases

1. **Wrong fragment kind**
   - `WHERE {order_fragment}` where a `PredicateFragment` is required
   - message: `expected a predicate fragment in WHERE, found an order fragment`

2. **Missing required binding**
   - imported fragment references `posts` but the outer statement only binds
     `users`
   - message should name the missing binding and list visible bindings when short

3. **Parameter contract mismatch**
   - imported fragment solved `$id` as `uuid`, outer query solved `$id` as `int4`
   - primary span should be the hook insertion site, with a related note about
     the earlier constraint if available

4. **Alias/output-name collision from composition**
   - imported ordering/projection fragment introduces a duplicate name

5. **Invalid hook placement**
   - predicate inserted into `SELECT` list, order fragment inserted into `WHERE`,
     etc.

Composition diagnostics should prefer the **call site** span because that is
where the user can act. If the imported fragment's definition span is available,
mention it as secondary context only when it materially reduces confusion.

### Concrete example diagnostics

#### Parse error

```text
error: expected an expression after `=`
  --> src/users.rs:12:48
   |
12 |     let q = query!("select users.id from users where users.id =");
   |                                                                ^ expected expression here
   |
   = help: add a placeholder like `$id` or a qualified column like `users.id`
```

#### Unsupported feature

```text
error: `WITH` is not supported by `query!` yet
  --> src/users.rs:18:20
   |
18 |     let q = query!("WITH recent AS (select users.id from users) select recent.id");
   |                    ^^^^
   |
   = note: v1 only accepts a single SELECT statement without CTEs or subqueries
   = help: use the lower-level runtime SQL API for this query until CTE support lands
```

#### Qualification / resolution error

```text
error: unknown column `users.nmae`
  --> src/users.rs:25:34
   |
25 |     let q = query!("select users.nmae from users");
   |                                  ^^^^
   |
   = help: did you mean `users.name`?
```

#### Placeholder type error

```text
error: placeholder `$name` is used as `text`, but this comparison expects `int4`
  --> src/users.rs:31:61
   |
31 |     let q = query!("select users.id from users where users.id = $name");
   |                                                             ^^^^^
   |
   = note: `users.id` has SQL type `int4`
   = help: use an `int4` parameter here or compare against a text column instead
```

#### Composition error

```text
error: predicate fragment requires binding `posts`, but this query only binds `users`
  --> src/users.rs:40:41
   |
40 |     let q = query!("select users.id from users where {published_posts()}");
   |                                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: join `posts` into this query first, or use a fragment built for `users`
```

### Testing implications

Diagnostics need their own test surface in addition to parser/resolver behavior
tests.

1. **Unit-level diagnostic builder tests**
   - category -> message template
   - span selection (`binding`, `column`, `placeholder`, `hook`)
   - help/suggestion suppression when confidence is low

2. **Source-map fidelity tests**
   - placeholder rewriting still points at the original named placeholder
   - multi-line literals and comments highlight the right token
   - hook insertions point at the braces/call site, not synthetic SQL text

3. **Semantic golden tests**
   - fixed schema fixtures for resolution/type errors
   - assert category, canonical span, rendered primary message, and help text

4. **`trybuild` UI goldens**
   - one `.stderr` fixture per major category
   - include at least one composition failure, one unsupported-feature case, and
     one nearest-symbol suggestion case
   - keep wording intentionally stable so diagnostics remain part of the API

5. **Regression discipline**
   - every new unsupported syntax gate adds a negative test
   - every bug fix in span mapping adds a focused source-map regression test
   - avoid snapshotting full rustc noise when only the macro-owned message
     matters; prefer assertions around the relevant excerpt where possible

## Work Items

1. Prove pure-Rust parser integration and normalized parse-tree extraction
   inside the macros crate.
2. Design a schema symbol/module system for tables, columns, SQL types, and
   nullability.
3. Define a minimal checked SQL subset and full-qualification rules.
4. Design resolver/type-check passes for placeholders, predicates, and
   projections.
5. Design lowering from checked query IR to executable `babar` query values or
   replacement typed statement/runtime abstractions.
6. Define query-composition APIs that interoperate with the typed macro system.
7. Design diagnostics, fallback rules, and explicit unsupported-feature errors
   (see Diagnostics UX above).

## Notes

- The first implementation should optimize for a delightful narrow path, not
  broad SQL completeness.
- Because this is intended as the future query direction, optimize for
  architectural cleanliness and long-term ergonomics rather than minimizing
  churn against today's query APIs.
- The parser backend should remain replaceable, but the canonical architecture
  should optimize first for a pure-Rust implementation with excellent spans and
  clean proc-macro ergonomics.
