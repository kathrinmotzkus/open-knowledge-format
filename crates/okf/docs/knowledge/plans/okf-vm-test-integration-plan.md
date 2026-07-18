---
title: OKF VM Test Integration Plan
type: Plan
status: Draft
created: 2026-07-18
updated: 2026-07-18
---

# OKF VM Test Integration Plan

This plan defines how OKF and `okf-http` can be tested on virtual machines
without requiring the development workstation to be the deployment target.

The goal is to make release, packaging, service, reverse-proxy, and deployment
tests realistic while keeping the user's real server safe.

## Goals

- Test OKF on clean Linux systems without relying on the local development
  environment.
- Verify installation paths that do not require Rust.
- Test public read-only deployment before using a real public server.
- Test internal deployments with authentication and reverse-proxy assumptions.
- Allow Codex-assisted debugging through controlled SSH access and copied logs.
- Keep VM access explicit, auditable, and easy to disable.

## Non-Goals

- Do not require cloud infrastructure.
- Do not expose private OKF document roots directly to a VM unless the user
  deliberately copies or deploys them.
- Do not store SSH private keys, passwords, API keys, or server credentials in
  the repository.
- Do not make production deployment depend on a VM-only workflow.

## Recommended VM Access Model

The preferred first setup is SSH access from the host to the VM with a local
SSH alias.

Example host-side SSH config:

```sshconfig
Host okf-vm
    HostName 192.168.122.50
    User kathrin
    IdentityFile ~/.ssh/id_ed25519
```

Codex can then run controlled diagnostic commands such as:

```bash
ssh okf-vm 'uname -a'
ssh okf-vm 'command -v okf-http'
ssh okf-vm 'systemctl --user status okf-http'
ssh okf-vm 'ls -la /var/lib/okf/sites'
```

The user remains in control of:

- creating the VM
- approving SSH access
- choosing the SSH alias
- managing SSH keys
- granting or withholding sudo rights

## Alternative Access Models

### Shared Folder

A shared folder can be used when direct SSH command execution is not desired.

The VM writes logs, status files, package-install output, and deployment reports
into a shared host directory. Codex reads those files from the host workspace.

This is safer but slower than SSH.

### Manual Copy/Paste

The user can run commands manually in the VM and paste the output back into the
conversation.

This works for diagnosis but is not ideal for repeated release tests.

### Vagrant or Ansible

Later, OKF can add automated test environments through Vagrant or Ansible.

This should come after the manual VM workflow is proven.

## Relationship to Platform and CI Work

Platform support, GitHub CI, release artifacts, and Debian packaging are tracked
in [Platform, CI, and release artifacts](platform-ci-and-release-artifacts-plan.md).

This VM plan starts after CI has produced an artifact or when a local artifact
needs realistic installation testing. VM tests should prove system behavior that
GitHub-hosted runners cannot fully prove, such as systemd, NGINX,
`/var/lib/okf`, rsync deployment, rollback, and firewall guidance.

## Phase 1: Define Supported Test VM Profiles

Start with a small number of realistic profiles:

- Debian stable without Rust
- Ubuntu LTS without Rust
- optional ARM/aarch64 VM when hardware or virtualization support is available

Each VM profile should document:

- OS version
- architecture
- package manager
- init system
- default firewall status
- whether NGINX is installed
- whether the VM has internet access

Completion criteria:

- At least one clean Debian or Ubuntu VM is available for OKF testing.
- The VM can be reached from the host by a known SSH alias or through a shared
  folder workflow.

## Phase 2: Establish Safe SSH Test Access

- Create a dedicated SSH alias, for example `okf-vm`.
- Prefer key-based SSH access.
- Avoid password-based automation.
- Do not copy private SSH keys into the repository.
- Decide whether the test user has passwordless sudo, passworded sudo, or no
  sudo.
- Document which commands require user approval before they are run.

Completion criteria:

- `ssh okf-vm 'uname -a'` works from the host.
- The user can revoke VM access by removing the SSH alias or key authorization.
- Codex-assisted commands can inspect the VM without receiving secrets.

## Phase 3: Test Release Installation Without Rust

- Start from a VM without Rust installed.
- Install `okf-http` from one of:
  - GitHub release archive
  - local `.deb` package
  - APT repository when available
- Verify that `okf-http` runs without `cargo`, `rustc`, or `rustup`.
- Verify that installed files land in documented paths.

Completion criteria:

- `okf-http --version` works on the VM.
- `command -v cargo` is not required.
- Installation can be repeated on a fresh VM.

## Phase 4: Test Public Read-Only Site Mode

- Generate a public OKF bundle on the development workstation.
- Deploy it to the VM under:

```text
/var/lib/okf/sites/<sitename>/
  releases/
    <release-id>/
  current -> releases/<release-id>
```

- Run `okf-http` against the `current` symlink.
- Verify that public pages render.
- Verify that mutation routes are unavailable.
- Verify that local source paths are not exposed.

Completion criteria:

- The VM serves the public OKF site from the bundle.
- The site works without access to the original local document roots.
- Protected or mutating routes are not mounted.

## Phase 5: Test Service Installation

- Run the installer in the VM.
- Let the installer ask whether `okf-http` should be installed as a service.
- Choose the service mode.
- Choose the listening port.
- Verify that the installer checks whether the selected port is available.
- Verify that the service survives reboot.

Completion criteria:

- `okf-http` can run as a managed user or system service.
- Service mode, bind host, and port are visible in generated configuration.
- The user receives clear firewall and reverse-proxy guidance.

## Phase 6: Test NGINX Reverse Proxy

- Install and configure NGINX in the VM.
- Proxy traffic to `okf-http` on loopback.
- Let NGINX own public-facing HTTP/TLS concerns where applicable.
- Verify that `okf-http` itself does not need to listen on a public interface.

Completion criteria:

- The OKF site is reachable through NGINX.
- `okf-http` binds to loopback for reverse-proxy deployment.
- The documented NGINX snippet is sufficient or corrected based on VM results.

## Phase 7: Test rsync Deployment and Rollback

- Generate a new OKF bundle locally.
- Deploy it to a new release directory with `rsync` over SSH.
- Do not write directly into `current`.
- Switch `current` after upload.
- Verify the new release.
- Roll back by repointing `current` to the previous release.

Completion criteria:

- Incremental deployment works over SSH.
- Rollback does not require rebuilding the site.
- File ownership and permissions remain correct.

## Phase 8: Test Internal Protected Mode

Only start this phase after public read-only deployment is stable.

- Configure internal mode behind a trusted reverse proxy.
- Enable protected editor or mutation routes only through explicit flags.
- Verify that protected routes require the expected authentication,
  capabilities, CSRF/origin checks, revision checks, digest checks, and
  confirmation headers.
- Verify that protected internal editing cannot be enabled accidentally on a
  directly exposed listener.

Completion criteria:

- Internal protected mode works only under the trusted reverse-proxy profile.
- Public read-only mode remains simpler and safer.

## Phase 9: Capture VM Test Reports

Each VM test should produce a short report:

- VM profile
- OKF version
- `okf-http` version
- installation method
- service mode
- bind host and port
- bundle path
- reverse-proxy status
- test result
- problems found
- fixes applied

Reports may later become release checklist evidence.

Completion criteria:

- Release candidates can be validated against at least one VM report.
- Repeated packaging mistakes become visible before publishing.

## Security Notes

- VM SSH access should be treated as real system access.
- Commands that install packages, change services, modify firewall rules, or
  edit `/var/lib/okf` should remain explicit.
- Secrets must not be copied into the repository or pasted into logs.
- Public test bundles should be generated from documents intentionally marked
  public.
- A VM is a test target, not a reason to weaken production security rules.

## Open Questions

- Should the first VM target be Debian stable, Ubuntu LTS, or both?
- Should VM tests be scripted first with shell scripts or documented manually
  until the workflow stabilizes?
- Should OKF provide a `doctor` command for collecting safe diagnostic output
  from VMs?
- Which VM reports should be required before a release is promoted from release
  candidate to public recommendation?
