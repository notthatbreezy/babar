# Plan

Add prepared-statement support around the existing extended-protocol work already in progress. The implementation should make server-side prepare/execute/close flows reliable, keep per-session statement caching consistent with handle lifetimes, and expose the public `Session::prepare_*` API with tests that cover reuse, schema validation, and deallocation.

## Work Items

1. Finalize the driver-side prepare / execute-prepared / close-statement command handling so protocol state, replies, and error propagation are correct.
2. Tighten the public prepared-statement surface in `session/`, including cache ownership, exports, and related error/reporting behavior.
3. Finish integration coverage for prepared queries and commands, then run repository formatting and test checks and address any resulting issues.

## Notes

- The repository already has substantial uncommitted prepared-statement changes; this plan treats them as the starting point rather than fresh work.
- Prepared statement caching should stay keyed by SQL plus parameter OIDs so decoder differences do not duplicate server-side statements.
- Cleanup behavior must be explicit (`close`) and best-effort on drop without hiding protocol failures from explicit close paths.
