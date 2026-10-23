# Release runbook

How DuoHive is released, and how a bad release is undone. The release is run by the
author: an agent never tags, publishes, registers a name or posts an announcement
([phase 5 plan](plans/phase-5-plan.md), "Who does what").

> **Status:** the release workflow (`.github/workflows/release.yml`) and the `actionlint` and `package` jobs in `ci.yml` are not on `main` yet. They were written and reviewed, but pushing workflow files needs a token with the `workflow` scope. Until they land, steps 4 and 5 of "A release" below describe the intended pipeline.

## One-time setup (the author)

1. Phase 4 reads `Status: Done` and the name checks (phase 5 Task 5.5b) are clear.
2. Create the tap repository `duongnv129/homebrew-tap` (P5-Q5) and a fine-grained token that can push to it.
3. In the repository settings create the environment `release` with a required reviewer, and add the secrets `CARGO_REGISTRY_TOKEN` (crates.io) and `TAP_TOKEN` (the tap token).
4. Protect the release path. Done 2026-10-23: the tag ruleset `release tags` (only repository admins create, move or delete `v*` tags) and the `main` ruleset (pull requests, one owner approval with an admin bypass, squash only, seven required checks). Still to do: restrict the `release` environment's deployment branches and tags to `v*`, and turn on "prevent self-review" if a second account exists. The `verify` job refuses a tag whose commit is not on `main`.
5. Install `dist` (cargo-dist 0.30.0, the version in `dist-workspace.toml`) and run `dist plan` once. `dist-workspace.toml` and `.github/workflows/release.yml` have not been run by an agent; fix what `dist plan` and a first workflow run report.

## A release

1. On the release machine (Apple M-series, plugged in, 1-minute load at most 2): `scripts/set-version.sh <version>`, then in `Cargo.toml` set `publish = true` under `[workspace.package]`, then `make release`.
2. `./target/release/duohive bench --gate --json > docs/release-gates/<version>.json`. The run exits 0 only when every target is met and the binary is under 10,000,000 B. Exit 3 means the machine was too busy: quiet it and run again. A miss goes back to phase 4 Task 4.2; a target is never lowered without a PRD change.
3. Commit the version change, the lock file and the gate file in one release commit, open a pull request and merge it.
4. Tag the merged commit and push the tag: `git tag v<version> && git push origin v<version>`. A pre-release version (`1.0.0-rc.1`) is published as a GitHub pre-release.
5. The `Release` workflow runs `verify` (the tag, the Cargo version and the gate file agree, then `make check`), `build` (the three targets, the size check, a `--version` smoke test, build attestations), then waits at `github-release`, `publish-crates` and `publish-tap` for the `release` environment's reviewer. Approve them in that order.
6. Check: `brew install duongnv129/tap/duohive`, `cargo install duohive --version <version>`, and the release binary each print `duohive <version>` from `--version`.

`scripts/check-release-version.sh` refuses a tag that differs from the Cargo version, a missing or failed gate file, a gate run that was not on macOS arm64, an oversize binary, and a gate run on code that has since changed outside `docs/`.

## If a publish job fails half way

`publish-crates` publishes the seven crates in dependency order (`duohive-core`, `-cli`, `-store`, `-tmux`, `-tui`, `-mcp`, then `duohive`) and skips a crate whose version is already on crates.io, so re-running the failed job finishes the set. A crate version cannot be republished once it is on crates.io: fix forward with the next patch version.

## Rolling back a bad release

1. `cargo yank --version <version> <crate>` for each published crate (yanking stops new resolutions; existing lock files keep working).
2. Delete the GitHub Release (keep the tag) or mark it as a pre-release.
3. Revert the tap commit that bumped `Formula/duohive.rb`.
4. Fix, and release the next patch version through the steps above. Add the cause to `CHANGELOG.md`.

## Notes

- v1 binaries are not notarised (P5-D9). A tarball download may be quarantined by Gatekeeper: `xattr -d com.apple.quarantine duohive`. `brew` and `cargo install` are not affected.
- Linux aarch64 is not shipped in v1 (P5-D4).
- The `dh` alias is created only by `install.sh`.
