# Rollback, Uninstall, And Reinstall

Use this runbook when a shipped artifact has to be backed out or reinstalled without guessing at the install layout.

## 1. Inspect The Current Install

```bash
gcc-formed --formed-self-check
```

Confirm:

- `paths.install_root`
- `paths.install_root_includes_target_triple`
- `paths.install_root_access`

If the installed binary still runs, also capture:

```bash
gcc-formed --formed-version=verbose
```

## 2. Preview The Recovery Step

Preview rollback before changing anything:

```bash
cargo xtask rollback \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>" \
  --version <previous-version> \
  --dry-run
```

The dry-run output should show a single `swap_symlink` on `<install-root>/current` when the managed launcher symlinks are already healthy.

## 3. Perform Rollback

```bash
cargo xtask rollback \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>" \
  --version <previous-version>
```

Verify the result:

```bash
<bin-dir>/gcc-formed --formed-version
```

## 4. Uninstall Paths

Remove one non-active version only:

```bash
cargo xtask uninstall \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>" \
  --mode remove-version \
  --version <old-version>
```

Purge the managed install payload and launchers:

```bash
cargo xtask uninstall \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>" \
  --mode purge-install
```

By default, uninstall does not remove user state. Only purge state when you explicitly mean to delete support artifacts:

```bash
cargo xtask uninstall \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>" \
  --mode purge-install \
  --state-root "<state-root>" \
  --purge-state
```

## 5. Reinstall

From a control-dir bundle:

```bash
cargo xtask install \
  --control-dir "<control-dir>" \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>"
```

From an immutable release repository with exact version pin:

```bash
cargo xtask install-release \
  --repository-root "<release-repo-root>" \
  --target-triple "<target-triple>" \
  --version <version> \
  --expected-primary-sha256 "<primary-archive-sha256>" \
  --expected-signing-key-id "<signing-key-id>" \
  --expected-signing-public-key-sha256 "<trusted-public-key-sha256>" \
  --install-root "<install-root>" \
  --bin-dir "<bin-dir>"
```

If the reinstall is part of a stable-cut investigation, cross-check the evidence in [STABLE-RELEASE.md](../../STABLE-RELEASE.md).
