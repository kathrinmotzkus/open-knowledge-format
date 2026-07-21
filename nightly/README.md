# OKF nightly Debian packages for Linux x86_64

This directory contains the convenience path for installing unreleased
`okf-http` development builds on Debian-compatible Linux x86_64 systems.
Debian reports this architecture as `amd64`.

Nightly builds are intentionally separate from stable releases:

- stable releases are versioned GitHub Releases and crates.io packages
- nightly builds are published to the mutable `okf-http-nightly` GitHub
  prerelease
- nightly builds may contain unreleased code from recent development work
- nightly builds are meant for development machines, test systems, and clean
  validation environments

Do not use nightly builds as the documented production installation path.
Production-oriented Debian/Ubuntu installation should eventually use an APT
repository.

## Recommended path on Debian 13.6 x86_64

If you have a trusted checkout of this repository, update `okf-http` with one
command:

```bash
nightly/install-okf-http-nightly-deb.sh
```

For non-interactive test systems:

```bash
nightly/install-okf-http-nightly-deb.sh --yes
```

The script:

- verifies that the local system is Debian architecture `amd64`
- reads the mutable `okf-http-nightly` GitHub prerelease
- downloads the matching `.deb` package and `.sha256` file
- verifies the checksum before installation
- installs the package with `apt`, using `sudo` when needed
- leaves stable releases untouched unless the package manager upgrades or
  downgrades `okf-http` according to the downloaded package version

## One-copy command for a fresh Debian system

On a fresh Debian system without a source checkout, download the installer
script first and then run it:

```bash
wget -O /tmp/install-okf-http-nightly-deb.sh \
  https://raw.githubusercontent.com/kathrinmotzkus/open-knowledge-format/main/nightly/install-okf-http-nightly-deb.sh
sh /tmp/install-okf-http-nightly-deb.sh
```

Use `--yes` only on systems where you deliberately want unattended nightly
updates:

```bash
sh /tmp/install-okf-http-nightly-deb.sh --yes
```

Avoid piping the downloaded script directly into a root shell. The package is
verified with its `.sha256` file, but the installer script itself should still
be readable before it is executed.

## Download without installing

```bash
nightly/install-okf-http-nightly-deb.sh --download-only
```

This downloads and verifies the files, prints their paths, and does not call
`apt`.

## Verify the installed package

After installation:

```bash
dpkg-query -W okf-http
okf-http --version
cat /usr/share/doc/okf-http/VERSION
```

A healthy local read-only smoke test is:

```bash
okf-http 8003
wget -qO- http://127.0.0.1:8003/health
```

`okf-http` must bind directly only to loopback addresses. Intranet and public
server access must go through a reverse proxy such as NGINX.

## Publishing nightly packages

Maintainers publish nightly packages with the `OKF Nightly Debian Packages`
GitHub Actions workflow. The workflow is manual by design: not every commit
should automatically become a downloadable development package.

The current nightly comfort path publishes the Linux x86_64/`amd64` Debian
package. Other platforms remain covered by the regular release and CI plans,
but they are not part of this Debian nightly shortcut.

The workflow updates the mutable `okf-http-nightly` prerelease and replaces the
attached Debian assets.
