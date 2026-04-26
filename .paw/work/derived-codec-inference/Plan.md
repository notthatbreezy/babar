# Derived Codec Inference Plan

## Summary
Keep explicit per-field codec annotations fully supported, but make them optional for common unambiguous field types in `#[derive(Codec)]`. The default behavior should infer codecs for obvious Rust types, while `#[pg(codec = "...")]` remains the override path and still becomes required when inference is ambiguous or unavailable.

## Approach
1. Extend the derive macro so each field either:
   - uses an explicit `#[pg(codec = "...")]` override, or
   - falls back to an inferred default codec expression for supported Rust field types.
2. Keep inference intentionally narrow and deterministic:
   - primitives and standard obvious mappings (`i16`, `i32`, `i64`, `bool`, `String`, `Vec<u8>`, `Option<T>`)
   - feature-gated common types where there is already one canonical codec (`Uuid`, `time`, `chrono`, etc.) if inference can be generated cleanly
3. Preserve compile errors for unsupported or ambiguous fields, but change the messaging to explain that explicit `#[pg(codec = ...)]` is still available as an override.
4. Update examples, integration tests, and trybuild coverage so both inferred and explicit modes are exercised.

## Work Items
- **derive-inference-core** — implement inference logic in the derive macro and preserve explicit override behavior
- **derive-inference-validation** — add/update UI and integration tests for inferred defaults, unsupported fields, and explicit overrides
- **derive-inference-docs** — update example(s) and docs to present inferred defaults as the normal path while retaining explicit override examples

## Key Decisions
- Inference should be **default-on**, not behind an extra attribute.
- Explicit field attributes stay supported forever as the escape hatch.
- Unsupported fields should fail with actionable guidance rather than silent guessing.
- If a Rust type could map to multiple codecs, inference should refuse and require `#[pg(codec = ...)]`.
