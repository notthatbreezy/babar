# Release runbook

## Preconditions
- Rust 1.85 installed locally (MSRV) plus current stable and nightly.
- Docker available for integration tests.
- crates.io ownership configured for `babar` and `babar-macros`.
- GitHub Actions green on the release commit.

## Validation checklist
1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-features`
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
5. `cargo publish --dry-run --allow-dirty -p babar-macros`
6. `cargo publish --dry-run --allow-dirty -p babar` *(only after `babar-macros` is published and visible on crates.io; see blocker note below)*
7. `cargo deny check`
8. `cargo audit`
9. `cargo semver-checks`
10. `cargo test --test tracing`

## Release steps
1. Review `CHANGELOG.md` and confirm milestone coverage is complete.
2. Confirm workspace version and crate versions are the intended release (`0.1.0` here).
3. Push the release commit and wait for CI to finish.
4. Create and push tag `v0.1.0`.
5. Publish `babar-macros`.
6. Publish `babar`.
7. Open docs.rs build and verify it completed successfully.
8. Announce the release with the README quick-start and example links.

## Post-release checks
- Install from crates.io in a clean checkout.
- Run `cargo doc --open -p babar`.
- Run the example apps against a disposable PostgreSQL instance.
- File follow-up issues for anything deferred from the M6 checklist.

## Residual actions that cannot be completed in-repo
- pushing the `v0.1.0` tag
- publishing to crates.io
- verifying the live docs.rs build after publish
- waiting for the crates.io index to contain `babar-macros` before `cargo publish --dry-run -p babar` can verify successfully. Cargo rewrites the workspace path dependency to a registry dependency during packaging, so local verification fails with `no matching package named 'babar-macros' found` until the macros crate has actually been published.
