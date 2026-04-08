## Summary

-

## Support Tier

- [ ] GCC 15 primary render path
- [ ] GCC 13/14 compatibility path
- [ ] Older / unsupported path
- [ ] Packaging / install / release only

## Release Checklist Impact

- [ ] This PR changes the first-release scope documented in `README.md` / `RELEASE-NOTES.md`.
- [ ] This PR changes representative acceptance or snapshot gate behavior.
- [ ] This PR changes packaging, install, rollback, or release repository behavior.
- [ ] This PR preserves the rule that GCC 13/14 remain compatibility-only paths unless explicitly approved.

## Verification

- [ ] `cargo xtask check`
- [ ] `cargo xtask replay --root corpus --subset representative`
- [ ] `cargo xtask snapshot --root corpus --subset representative --check --docker-image gcc:15`
- [ ] Other:

## Trace / Evidence

- Trace bundle or CI artifact links:
