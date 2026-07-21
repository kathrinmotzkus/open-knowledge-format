# OKF nightly builds

This directory contains convenience tooling for unreleased `okf-http`
development builds.

Nightly builds are intentionally separate from stable releases:

- stable releases are published as versioned GitHub Releases and crates.io
  packages
- nightly builds are published to the mutable `okf-http-nightly` GitHub
  prerelease
- nightly builds may contain unreviewed, unreleased, or recently changed code
- nightly builds are intended for development machines, test systems, and clean
  validation environments

Do not use nightly builds as the documented production installation path.
Production-oriented Debian/Ubuntu installation should eventually use an APT
repository.

## Install or update the latest nightly Debian package

From a trusted checkout:

```bash
nightly/install-okf-http-nightly-deb.sh
```

For non-interactive test systems:

```bash
nightly/install-okf-http-nightly-deb.sh --yes
```

The script:

- detects the local Debian architecture
- downloads the matching `.deb` from the `okf-http-nightly` prerelease
- downloads the matching `.sha256`
- verifies the checksum
- installs the package with `apt`

Supported Debian architectures:

- `amd64`
- `arm64`

The script requires `wget`, `sha256sum`, and `apt`. It uses `sudo` when it is
not run as root.

## Download without installing

```bash
nightly/install-okf-http-nightly-deb.sh --download-only
```

This leaves the downloaded files in the printed temporary directory and does
not call `apt`.

## Publishing nightly packages

Maintainers publish nightly packages with the `OKF Nightly Debian Packages`
GitHub Actions workflow. The workflow is manual by design: not every small
commit should automatically become a downloadable development package.

The workflow updates the mutable `okf-http-nightly` prerelease and replaces the
attached Debian assets.
