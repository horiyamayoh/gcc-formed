# Signing Key Operations

This document defines the minimum operating procedure for release signing keys used for `SHA256SUMS.sig`.

## Current Release Evidence

- `pr-gate` and `nightly-gate` now emit `release-provenance.json` in the uploaded release artifacts.
- The provenance bundle records the GitHub run identity plus the package, publish, promote, resolve, install, and hermetic build JSON emitted by `xtask`.
- Detached signature verification remains bound to both `signing_key_id` and `signing_public_key_sha256`.

## Rotation

1. Generate a new Ed25519 private key offline.
2. Compute the new public key SHA-256 and key id from the resulting `SHA256SUMS.sig`.
3. Update CI/install pin consumers to trust both the current and next public key SHA during the overlap window.
4. Publish a release signed by the new key and confirm `release-provenance.json` records the new `signing_key_id` and `signing_public_key_sha256`.
5. Remove the old trust pin only after the new signed release is published and install smoke has passed.

## Revoke

1. Treat any suspected private-key exposure as an immediate release blocker.
2. Stop publishing new artifacts with the compromised key.
3. Remove the compromised public key SHA pin from CI/install automation.
4. Publish a signed revocation notice in the release repository and release notes.
5. Re-sign the latest intended release with a fresh key before re-opening the release channel.

## Emergency Re-sign

1. Rebuild or recover the exact control directory for the target release version.
2. Verify `manifest.json`, `build-info.txt`, and `SHA256SUMS` still match the intended payload.
3. Generate a new `SHA256SUMS.sig` with the replacement key.
4. Re-run `cargo xtask install`, `rollback`, `uninstall`, and `install-release` against the re-signed control directory using the new trust pin.
5. Publish updated release metadata and retain both the old and new provenance bundles for audit history.

## Audit Trail

- Keep `release-provenance.json`, `manifest.json`, `SHA256SUMS`, and `SHA256SUMS.sig` together in retained CI artifacts.
- Do not overwrite prior release metadata in-place without preserving the previous signed state.
- Any rotation, revoke, or emergency re-sign event must be called out in `RELEASE-NOTES.md`.
