---
title: OKF Platform, CI, and Release Artifacts Plan
type: Plan
status: Active
created: 2026-07-18
updated: 2026-07-21
---

# OKF Platform, CI, and Release Artifacts Plan

This plan pulls platform support, GitHub CI, release binaries, and packaging
preparation out of the later VM/deployment plans. These topics should be handled
early because unclear platform support causes noisy GitHub failures and makes
the project harder to trust.

The goal is to make OKF broader and more predictable without forcing every
feature into a single `lib.rs`.

## Goals

- Define the first supported platform baseline.
- Make GitHub CI failures meaningful instead of noisy.
- Build release artifacts before VM testing.
- Keep platform-specific behavior modular.
- Prepare packaging without requiring users to install Rust.
- Keep provider-backed and deployment-specific tests opt-in.

## Non-Goals

- Do not promise every operating system immediately.
- Do not make Windows a release blocker before Windows behavior is reviewed.
- Do not publish packages automatically before manual release confidence exists.
- Do not move all platform, packaging, service, and HTTP code into one
  monolithic `lib.rs`.

## Immediate Supported Platform Baseline

Initial supported platforms:

- Linux x86_64
- Linux aarch64
- macOS x86_64
- macOS aarch64

Windows remains experimental until path handling, service integration, browser
installation, local TLS, filesystem watching, and shell behavior have been
reviewed separately.

CI failures on supported platforms are release blockers. CI failures on
experimental platforms should be visible but must not block releases until that
platform is explicitly declared supported.

## Architecture Rule: No Monolithic `lib.rs`

Platform and release work must keep OKF maintainable.

- `lib.rs` should remain a public API entrypoint and module map, not the
  implementation home for every subsystem.
- Platform-specific code should live in focused files or modules. The current
  internal `okf-http` split is:

```text
crates/okf-http/src/platform/mod.rs
  shared platform path model and dispatch

crates/okf-http/src/platform/linux.rs
  Linux/XDG filesystem defaults and runtime paths

crates/okf-http/src/platform/macos.rs
  macOS filesystem defaults and runtime paths

crates/okf-http/src/platform/fallback.rs
  conservative fallback behavior for not-yet-supported targets
```

- CI scripts should live under `scripts/`.
- Packaging files should live under a dedicated packaging directory when they
  become non-trivial, for example:

```text
packaging/
  deb/
  systemd/
  release/
```

- Shared platform decisions should be documented before they become hidden
  assumptions in CLI code.

Completion criteria:

- New platform support does not inflate `lib.rs` with implementation details.
- Platform-specific behavior has clear module boundaries.
- CI and packaging logic is scriptable and reviewable outside Rust source files.

Implementation status:

- Implemented with internal modules under `crates/okf-http/src/platform/`.
- This keeps `okf-http` publishable as a single crates.io package while still
  avoiding platform-specific code in the main HTTP, TLS, and auth modules.

## Phase 1: Document Platform Support

- Add user-facing documentation that distinguishes:
  - supported platforms
  - experimental platforms
  - not-yet-supported platforms
- Document the first supported platform baseline.
- State that Rust is required only for source builds and `cargo install`, not
  for future prebuilt release artifacts.
- Explain that Windows is experimental, not rejected.

Completion criteria:

- Users can tell whether their platform is supported before installing OKF.
- Release notes can refer to the documented support matrix.

Implementation status:

- Implemented in
  [OKF Platform Support](../references/platform-support.md), the repository
  README, and the standalone getting-started guide.

## Phase 2: Clean Up Existing GitHub CI Signal

The current CI baseline already includes:

- release gates on Ubuntu
- workspace tests on Ubuntu and macOS
- MSRV checks for `okf` and `okf-http`
- offline tests without Voyage credentials

This phase should make those checks quieter and more intentional.

- Review the existing workflow errors.
- Ensure required jobs use supported platforms only.
- Ensure experimental jobs are marked clearly or moved behind manual dispatch.
- Keep provider-backed tests, such as Voyage AI API tests, opt-in.
- Keep offline tests without provider credentials as the default.
- Keep locked dependency resolution in required CI.

Completion criteria:

- GitHub no longer reports avoidable failures for platforms not yet supported.
- Required CI jobs correspond to the documented support baseline.
- Optional jobs are visibly optional.

Implementation status:

- Implemented for the current baseline by renaming required jobs, making macOS
  architecture coverage explicit, installing `cargo-audit` before release
  gates, keeping provider-backed tests credential-free by default, and moving
  Windows behind manual `workflow_dispatch` as an optional experimental job.
- Refined with architecture-aware change detection. Ordinary library,
  `okf-http`, and browser changes start with Linux x86_64. Linux platform
  module changes trigger Linux x86_64 and Linux aarch64. macOS platform module
  changes trigger macOS x86_64 and macOS aarch64. Shared platform module,
  lockfile, workflow, script, release-infrastructure, manual, and tag-triggered
  runs use the full supported matrix.

## Phase 3: Add Targeted Platform Builds

- Add CI build checks for the supported target triples:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
- Use cross-compilation only where it is reliable.
- Prefer native GitHub-hosted runners for macOS architecture checks when
  available.
- If a target cannot be fully tested in GitHub CI, document the gap and cover it
  through release artifact checks or VM/manual tests.

Completion criteria:

- CI proves that OKF and `okf-http` build for the supported targets.
- Unsupported target failures do not appear as unexplained release blockers.

Implementation status:

- Implemented in GitHub CI as separate target-build jobs per supported target.
- Linux x86_64, macOS x86_64, and macOS aarch64 use native GitHub-hosted
  runners.
- Linux aarch64 uses cross-compilation with the GNU aarch64 cross toolchain and
  an explicit Rust target linker.
- Native Linux-aarch64 runtime validation remains part of the later VM or
  hardware testing workflow.

## Phase 4: Build Release Artifacts in CI

- Add a manual or release-candidate workflow that builds release binaries.
- Start with archives such as:

```text
okf-http-<version>-x86_64-unknown-linux-gnu.tar.gz
okf-http-<version>-aarch64-unknown-linux-gnu.tar.gz
okf-http-<version>-x86_64-apple-darwin.tar.gz
okf-http-<version>-aarch64-apple-darwin.tar.gz
```

- Include checksums for each artifact.
- Upload artifacts for manual inspection.
- Do not publish automatically until the release process is proven.

Completion criteria:

- A release candidate produces downloadable binaries from GitHub CI.
- VM tests can consume CI-built Linux artifacts instead of rebuilding locally.

Implementation status:

- Implemented as `.github/workflows/okf-release-artifacts.yml`.
- The workflow runs manually through `workflow_dispatch` and automatically for
  `okf-http-v*` tags.
- It builds `okf-http` release archives for the supported target baseline and
  uploads each `.tar.gz` together with a `.sha256` checksum.
- The archive builder is `scripts/package-okf-http-release.sh`.
- On `okf-http-v*` tags, the workflow creates a GitHub Release and attaches
  the generated release archives and Debian packages directly to that release.
- Manually triggered workflow artifacts remain intended for release-candidate
  inspection before a final tag is created.
- crates.io publication remains a separate manual step and must happen only
  after required GitHub tests and release gates have completed successfully
  for the exact commit being released.
- Platform-specific implementation remains internal to `okf-http`; no
  additional crates.io packages are required for these platform modules.

## Phase 5: Prepare Debian Packaging

- Add packaging structure for `.deb` generation.
- Keep mutable data outside the package:

```text
/var/lib/okf/sites/<sitename>/
/etc/okf/
~/.config/okf/
```

- Package immutable files into standard locations, for example:

```text
/usr/bin/okf-http
/usr/share/doc/okf-http/
/usr/share/okf/browser/
/usr/share/okf/themes/
/usr/lib/systemd/user/okf-http.service.example
/usr/lib/systemd/system/okf-http.service.example
```

- Do not require Rust on the target system.
- Do not overwrite user content or user configuration on upgrade.

Completion criteria:

- CI or local release tooling can produce a `.deb` package for Linux.
- The package can be installed on a clean VM without Rust.

Implementation status:

- Implemented initial local Debian package generation with
  `scripts/package-okf-http-deb.sh`.
- Added `packaging/deb/` documentation and `packaging/systemd/` service
  examples.
- The release artifact workflow now builds and uploads `.deb` packages for
  Linux targets in addition to `.tar.gz` archives.
- The package installs immutable files under `/usr/bin`, `/usr/share/okf`,
  `/usr/share/doc/okf-http`, and systemd example locations.
- Mutable deployment data and user configuration remain outside the package.
- This is the first VM-testable `.deb` path. `cargo-dist` remains the preferred
  release-orchestration tool to evaluate next, with `cargo-deb` as the first
  fallback if needed.

## Phase 6: Add CI-to-VM Handoff

- Record the CI run ID, commit SHA, version, target triple, and artifact name.
- Download the CI artifact into the VM.
- Install or run that artifact without rebuilding from source.
- Use VM tests for system behavior:
  - systemd service
  - NGINX reverse proxy
  - `/var/lib/okf/sites/<sitename>`
  - rsync deployment
  - firewall guidance

Completion criteria:

- VM tests validate the same artifact that would be shipped to users.
- Release checklists can reference both CI artifacts and VM reports.

Preparation status:

- Prepared, not completed.
- Added [CI-to-VM handoff](../references/ci-to-vm-handoff.md).
- Added [VM test report template](../references/vm-test-report-template.md).
- Added `scripts/check-okf-vm-artifact.sh` for artifact smoke checks on a VM.
- Completion remains blocked until at least one clean VM installs or runs a
  CI-built artifact without rebuilding from source.

## Phase 7: Decide Windows Support Separately

Windows should remain a deliberate future decision, not accidental CI noise.

- Review Windows path handling.
- Review browser installation and user configuration paths.
- Review service installation alternatives.
- Review local TLS certificate handling.
- Review shell examples and scripts.
- Decide whether Windows support belongs in the same package family or needs
  separate installer guidance.

Completion criteria:

- Windows has a documented status.
- Windows failures are either fixed, skipped, or clearly marked experimental.

## Architecture Decisions

The following decisions replace the previous open questions. They should guide
the first implementation round for CI, release artifacts, and packaging.

### Release artifacts are built manually and from release tags

- OKF should support both manual and tag-based artifact builds.
- Manually triggered workflows build artifacts for release candidates and test
  runs.
- Real release tags automatically build reproducible release artifacts and
  publish them as GitHub Release assets.
- crates.io publishing is intentionally not automated by the artifact workflow.
  It happens only after the required GitHub tests and release gates are green.

Rationale:

- Manual builds provide control before a release.
- Tag-based builds ensure that published artifacts map clearly to a Git version.
- Release candidates can be tested without being treated as final releases.

### macOS Apple Silicon starts with GitHub-hosted runners

- macOS x86_64 and macOS aarch64 remain part of the first supported platform
  baseline.
- For macOS Apple Silicon (`aarch64-apple-darwin`), OKF initially uses the
  tests and builds provided by GitHub-hosted runners.
- Real tests on Apple Silicon hardware are desirable later, but they are not a
  prerequisite for the first GitHub-built artifacts.
- External volunteer testing, for example through interested community users,
  should wait until OKF and `okf-http` are mature and well documented enough to
  make the test task clear and reasonable.

Rationale:

- A successful build does not prove that installation, browser integration, and
  local paths feel good on a real system.
- At the moment, GitHub-hosted runners are the only practical Apple Silicon test
  path available to the project.
- External testers should receive a small, clear, and fair task instead of an
  immature debugging burden.

### Linux aarch64 starts with cross-compilation and later native tests

- Linux-aarch64 artifacts are built by cross-compilation first.
- Native VM, runner, or hardware tests should follow once a practical test path
  is available.
- A Raspberry Pi can later serve as real ARM64 test hardware if native tests
  become easier or more reliable that way.

Rationale:

- Cross-compilation is practical for producing early release artifacts.
- Cross-compilation does not prove every runtime detail.
- Native tests provide stronger confidence, especially for installation,
  systemd service behavior, NGINX integration, `/var/lib/okf`, and real ARM64
  runtime behavior.

### Debian packaging starts with `cargo-dist`

- OKF evaluates `cargo-dist` first because it can connect release artifacts and
  GitHub Releases well.
- If `cargo-dist` cannot represent the required package structure well enough,
  especially for systemd examples, browser/theme assets, or Debian package
  metadata, `cargo-deb` is the first fallback.
- `nfpm` and classic Debian packaging remain later options if neither
  `cargo-dist` nor `cargo-deb` meets the project requirements cleanly.

Rationale:

- The packaging tool determines how well OKF can automate binaries, `.deb`
  packages, checksums, GitHub Releases, and later APT repositories.
- The first solution must be good enough for real installation tests, but it
  does not need to be the final Debian maintainer solution.

### `okf-http` and the OKF library may use separate release policies

- `okf-open-knowledge-format` and `okf-http` do not have to be versioned
  together.
- They may use separate release policies.
- `okf-http` is expected to receive more frequent minor upgrades because it is
  more complex and closer to operating-system, browser, service, TLS,
  deployment, and packaging concerns.

Consequences:

- crates.io versions may remain separate.
- Release artifacts are primarily relevant for `okf-http` because it is the
  executable program.
- If only the library changes, new operating-system binaries are not
  necessarily required.
- If `okf-http` changes in a release, new release binaries must be built.
- `okf-http` must document which version or version range of
  `okf-open-knowledge-format` it was built and tested with.
- Release notes should clearly distinguish whether a change affects the OKF
  library, `okf-http`, packaging, service integration, or all components.

Rationale:

- The library and the binary have different user groups.
- Joint releases are easier to explain, but separate releases avoid unnecessary
  binary artifacts.
- The OKF library can remain more conservative while `okf-http` evolves faster.
