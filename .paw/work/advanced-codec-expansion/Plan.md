# Advanced Codec Expansion Plan

## Summary
Plan a codec expansion wave that puts **PostGIS first** using `geo-types` as the primary Rust type surface, while also covering `pgvector`, `tsvector` / `tsquery`, `macaddr` / `macaddr8`, `bit` / `varbit`, `multirange`, `hstore`, and `citext`.

This plan explicitly **excludes PostgreSQL built-in geometric types**. The intended spatial story is PostGIS `geometry` / `geography`, not `point` / `line` / `polygon`.

## Current State Analysis
- babar already has a clean pattern for optional codec families: feature-gated modules under `crates/core/src/codec/` plus re-exports in `codec/mod.rs`.
- The current optional families cover `uuid`, temporal codecs, JSON, numeric, net, interval, array, and range.
- There is no feature or module yet for PostGIS, pgvector, text search, macaddr, bit strings, hstore/citext, or multirange.
- The existing `range` support should make `multirange` materially easier than a truly greenfield codec family.

## Key Decisions
- **Primary focus**: PostGIS support with `geo-types`.
- **No built-in PG geometric types** in this work.
- **Feature-gated family expansion** remains the model.
- **Parallel-friendly structure**: a small shared architecture phase first, then PostGIS, vectors/search, scalar/extension codecs, and multirange can be implemented with minimal overlap.
- **Shared type metadata**: codecs can now describe slots by stable SQL type metadata, not only fixed OIDs, so extension-defined families can resolve dynamic OIDs per session before prepare-time validation.
- **PostGIS v1 wrapper story**: `Geometry<T>` and `Geography<T>` are distinct wrappers over `geo-types` values and carry an optional `Srid`; no separate Rust model for PostgreSQL built-in geometric types will be added.

## Proposed Feature Families
- `postgis` — PostGIS `geometry` / `geography` codecs using `geo-types`
- `pgvector` — embedding/vector codecs
- `text-search` — `tsvector` / `tsquery`
- `macaddr` — `macaddr` / `macaddr8`
- `bits` — `bit` / `varbit`
- `hstore` — `hstore`
- `citext` — `citext`
- `multirange` — PostgreSQL multirange codecs

## Work Items

### 1. `codec-architecture`
- Decide feature names, dependency crates, wrapper types, and docs.rs feature list updates.
- Resolve the main spatial modeling questions:
  - how SRID is represented
  - how `geometry` vs `geography` is exposed
  - how much of PostGIS is supported in the first cut
- Shared substrate chosen:
  - reserve narrow Cargo features for `postgis`, `pgvector`, `text-search`, `macaddr`, `bits`, `hstore`, `citext`, and `multirange`
  - use runtime type resolution for extension-defined codecs whose OIDs are not globally stable
  - keep PostGIS wrappers in core behind `postgis` so later EWKB codecs can reuse them without revisiting the public shape

### 2. `postgis-codecs`
- Add `postgis` feature and dependencies.
- Implement PostGIS `geometry` / `geography` codecs around `geo-types`.
- Cover common shapes and document limitations clearly.

### 3. `vector-search-codecs`
- Add `pgvector`.
- Add `tsvector` / `tsquery`.
- Decide whether text-search starts as wrapper/newtype-based or richer structured types.

### 4. `scalar-extension-codecs`
- Add `macaddr` / `macaddr8`.
- Add `bit` / `varbit`.
- Add `hstore`.
- Add `citext`.

### 5. `multirange-codecs`
- Build multirange support on top of the existing `range` model and codec patterns.

### 6. `codec-validation-docs`
- Add per-feature tests, examples if appropriate, and documentation updates.
- Update the README feature matrix and codec-discovery docs once the new families land.

## Suggested Phase Breakdown

### Phase 1: Shared architecture
- finalize feature names
- choose dependency crates
- decide wrapper/newtype strategy
- define PostGIS SRID / geography handling

### Phase 2: PostGIS
- ship the primary spatial codec family first

### Phase 3: Parallel codec families
- `pgvector`
- `text-search`
- scalar/extension codecs
- `multirange`

### Phase 4: Validation and docs
- feature coverage
- examples
- docs / feature matrix updates

## Notes
- `geo-types` should be the core spatial dependency; treat broader `geo` integration as optional follow-on convenience rather than the central codec contract.
- `pgvector` and `tsvector` are both important, but PostGIS remains the first milestone in this wave.
- `multirange` is a good fit for reuse of the existing `range` implementation patterns.
