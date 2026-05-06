# Release 0.2.0 Implementation Plan

## Overview

Prepare babar for a 0.2.0 release, align the source and runbook surfaces that
still point at 0.1.0, then execute the documented manual release path so the
published crates, pushed `v0.2.0` tag, and release-facing materials all
converge on the same shipped outcome.

## Current State Analysis

The workspace is still versioned as `0.1.0` in `Cargo.toml`, and the workspace
dependency pin for `babar-macros` still points at `0.1.0`. Release-facing
materials are also still on the prior release: `CHANGELOG.md` stops at `0.1.0`,
`README.md` still says `babar 0.1.0` is the published version, and
`.internal/RELEASE.md` still describes the release steps using `0.1.0` as the
example version.

`.internal/RELEASE.md` is the repository’s release source of truth today. It
defines a manual path: validate locally, push the release commit, create and
push `v<version>`, publish `babar-macros`, publish `babar`, then verify docs.rs.
For 0.2.0, there is no authoritative publish workflow to rely on, so the plan
follows the runbook rather than inventing an automation-first release route.

## Desired End State

The repository’s release-ready source identifies `0.2.0` consistently across
workspace versioning, release-facing materials, and the documented runbook. The
approved release-source identifier for 0.2.0 is captured exactly once after the
release commit is merged and GitHub Actions are green, the manual tag/publish
path completes successfully, and after execution the crates registry, pushed
`v0.2.0` tag, release-facing materials, docs.rs validation, and post-release
smoke checks all visibly point at the same 0.2.0 release outcome.

Verification should prove both readiness and completion: the local runbook
validation checklist before release, preflight confirmation that `0.2.0` is not
already published or tagged, successful tag and `cargo publish` execution, and
post-release checks that confirm the published crates, pushed tag, docs.rs
result, and clean-checkout smoke tests are aligned.

## What We're NOT Doing

- Reworking the broader long-term release strategy beyond what 0.2.0 needs
- Shipping unrelated feature or refactor changes as part of the release branch
- Publishing or maintaining a separate `0.1.x` line in this workflow
- Replacing the documented manual release runbook with a different release
  system in this workflow

## Phase Status
- [x] **Phase 1: Release-Ready Source Alignment** - Update version metadata, runbook references, and release-facing project materials so the source state consistently identifies 0.2.0.
- [ ] **Phase 2: Release Readiness Verification** - Validate the release-ready source against the documented runbook preconditions, confirm execution prerequisites, and preflight the release markers/version availability before execution.
- [ ] **Phase 3: Release Execution and Verification** - Merge the release branch, approve the single immutable release-source identifier after CI is green, execute the manual tag/publish flow, and verify the published result plus release markers.
- [ ] **Phase 4: Documentation** - Capture the as-built release process and final verification state in Docs.md and ensure project documentation remains accurate.

## Phase Candidates
- [ ] Decide later whether future releases should keep the manual runbook or gain additional automation once 0.2.0 is shipped

---

## Phase 1: Release-Ready Source Alignment

### Changes Required:
- **`Cargo.toml`**: Move the workspace version to `0.2.0` and keep the internal
  workspace dependency on `babar-macros` aligned with the same release number.
- **Release-facing project materials**:
  - `CHANGELOG.md`
  - `README.md`
  - `.internal/RELEASE.md`
  - any other directly affected version/status surfaces discovered during
    implementation
  Update them so 0.2.0 is presented as the next intended release rather than
  0.1.0.
- **Runbook step 1 alignment**:
  - review `CHANGELOG.md` and confirm milestone coverage is complete for the
    0.2.0 release
- **Tests / validation**:
  - local release-readiness validation using the documented runbook checklist
  - targeted packaging validation to ensure the release-ready source is still
    publishable

### Success Criteria:

#### Automated Verification:
Re-run the full validation suite after the Phase 2 runbook completeness review
to confirm the release-ready source still passes every gate before execution.
- [ ] Tests pass: `MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) && cargo "+${MSRV}" check --workspace --all-features`
- [ ] Tests pass: `MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) && cargo "+${MSRV}" test --workspace --no-run`
- [ ] Tests pass: `cargo fmt --check`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Tests pass: `cargo test --all-features`
- [ ] Docs build: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Packaging check: `cargo publish --dry-run --allow-dirty -p babar-macros`
- [ ] Hygiene: `cargo deny check`
- [ ] Security: `cargo audit`
- [ ] Compatibility: `cargo semver-checks`
- [ ] Targeted validation: `cargo test --test tracing`

#### Manual Verification:
- [ ] The release-ready source consistently identifies 0.2.0 across workspace
  version metadata, runbook examples, and release-facing materials.
- [ ] No release-facing surface still presents 0.1.0 as the current intended
  release except for preserved historical changelog content.
- [ ] `CHANGELOG.md` milestone coverage for 0.2.0 is reviewed and complete.

---

## Phase 2: Release Readiness Verification

### Changes Required:
- **`.internal/RELEASE.md` completeness review**: Ensure the documented
  preconditions, validation checklist, release steps, and post-release checks
  are all accurate for the 0.2.0 source state after Phase 1's mechanical
  version updates, including the release-source approval step and the local
  crates.io credential expectation.
- **Release preflight surfaces**:
  - confirm the release-source selection rule: the single approved immutable
    release-source identifier for 0.2.0 will be the merge commit on `main`
    after GitHub Actions are green and before any tag or publish step begins
  - confirm `0.2.0` is not already published on crates.io
  - confirm `v0.2.0` does not already exist as a pushed release tag
  - confirm Rust 1.88 (MSRV), stable, and nightly toolchains are available
  - confirm Docker is available for integration and example verification
  - confirm crates.io ownership exists for `babar` and `babar-macros`
  - confirm local crates.io publish credentials are configured via `cargo login`
  - confirm the release-ready source has green GitHub Actions before execution
- **Validation**:
  - local runbook-equivalent validation commands from `.internal/RELEASE.md`
  - dry-run packaging check for `babar-macros`
  - explicitly defer `cargo publish --dry-run --allow-dirty -p babar` until
    `babar-macros` is published and visible on crates.io, per the runbook
    residual-action note
  - preflight checks for version/tag availability and release-source rule

### Success Criteria:

#### Automated Verification:
- [ ] Tests pass: `MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) && cargo "+${MSRV}" check --workspace --all-features`
- [ ] Tests pass: `MSRV=$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) && cargo "+${MSRV}" test --workspace --no-run`
- [ ] Tests pass: `cargo fmt --check`
- [ ] Lint/typecheck: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Tests pass: `cargo test --all-features`
- [ ] Docs build: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Packaging check: `cargo publish --dry-run --allow-dirty -p babar-macros`
- [ ] Hygiene: `cargo deny check`
- [ ] Security: `cargo audit`
- [ ] Compatibility: `cargo semver-checks`
- [ ] Targeted validation: `cargo test --test tracing`

#### Manual Verification:
- [ ] The release-source approval rule is settled before execution: exactly one
  immutable identifier will be approved after merge-to-main CI is green and
  before any tag or publish step begins.
- [ ] `v0.2.0` is confirmed absent before release execution and remains the
  planned repository marker for completion.
- [ ] The release-ready source satisfies the documented runbook preconditions,
  including required toolchains, Docker, crates.io ownership, local
  `cargo login` credentials, green GitHub Actions, and the local validation
  checklist.

---

## Phase 3: Release Execution and Verification

### Changes Required:
- **Git / release execution flow**: Merge `feature/release-0-2-0` into `main`
  after the branch is validated, push the release commit, wait for GitHub
  Actions to finish green, then approve the resulting immutable release-source
  identifier for the exact source that will ship as 0.2.0.
- **Manual release execution surfaces**:
  - push the validated release commit to `main`
  - wait for GitHub Actions on that commit to complete successfully
  - create and push tag `v0.2.0`
  - publish `babar-macros`
  - wait for `babar-macros` to become visible on crates.io
  - run `cargo publish --dry-run --allow-dirty -p babar`
  - publish `babar`
  - open docs.rs and verify the published build succeeds
  - announce the release with the README quick-start and example links
- **Post-release verification**:
  - confirm the crates registry shows `0.2.0` for both crates
  - confirm `v0.2.0` exists on the remote repository and points at the approved
    release-source identifier
  - confirm docs.rs builds successfully for the published release
  - install from crates.io in a clean checkout
  - run `cargo doc --open -p babar`
  - run the example apps against a disposable PostgreSQL instance
  - file follow-up issues for anything deferred from the release checklist
  - confirm release-facing materials and the pushed tag agree on the released
    version

### Success Criteria:

#### Release Commands:
- [ ] Release command: `git push origin main`
- [ ] Release command: `git tag v0.2.0 && git push origin v0.2.0`
- [ ] Release command: `cargo publish -p babar-macros`
- [ ] Release command: `cargo publish --dry-run --allow-dirty -p babar`
- [ ] Release command: `cargo publish -p babar`

#### Manual Verification:
- [ ] The approved immutable release-source identifier is captured exactly once
  after merge-to-main CI is green, before publish begins, and is the same source
  referenced by the pushed `v0.2.0` tag.
- [ ] Preflight checks confirm no existing `babar 0.2.0` crate release and no
  existing `v0.2.0` tag before execution begins.
- [ ] GitHub Actions is green on the pushed release commit before `v0.2.0` is
  created and pushed.
- [ ] The manual release steps complete successfully and publish both
  `babar-macros` and `babar` at 0.2.0.
- [ ] The deferred `cargo publish --dry-run --allow-dirty -p babar` check runs
  successfully after `babar-macros` becomes visible on crates.io.
- [ ] The package registry shows `0.2.0` as the latest published release for
  both crates.
- [ ] docs.rs completes a successful build for the published release.
- [ ] The runbook's post-release checks complete successfully, including clean
  install, generated docs, and example verification against disposable
  PostgreSQL.
- [ ] The release announcement uses the README quick-start and example links for
  the shipped 0.2.0 release.
- [ ] The release is only considered complete once registry state, the pushed
  tag, release-facing materials, docs.rs validation, and post-release smoke
  checks visibly agree on 0.2.0.

---

## Phase 4: Documentation

### Changes Required:
- **`.paw/work/release-0-2-0/Docs.md`**: Record the as-built release path,
  release-source identifier used, manual runbook behavior, and final
  verification results (load `paw-docs-guidance` during implementation).
- **Project docs**:
  - confirm `README.md`
  - confirm `CHANGELOG.md`
  - confirm `.internal/RELEASE.md`
  - any other touched release-facing documentation
  remain accurate after the actual 0.2.0 release completes.

### Success Criteria:
- [ ] Content accurate, style consistent
- [ ] `Docs.md` explains the final manual release flow, release-source
  identifier, and the checks used to declare 0.2.0 complete

---

## References
- Issue: none
- Spec: `.paw/work/release-0-2-0/Spec.md`
- Research: none
