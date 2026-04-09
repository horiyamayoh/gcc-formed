# Trace Bundle Collection

Trace collection is opt-in. It should be enabled only for debugging or support, and the resulting bundle should be reviewed before sharing.

## 1. Discover Local Paths

Run:

```bash
gcc-formed --formed-self-check
```

Important fields:

- `paths.trace_root`
- `paths.state_root`
- `paths.runtime_root`
- `paths.install_root`
- `backend.support_tier`
- `rollout_matrix`

On a default Linux/XDG layout, `trace_root` is typically under `$XDG_STATE_HOME/cc-formed/traces` or `~/.local/state/cc-formed/traces`.

## 2. Capture A Trace

Re-run the failing command with trace retention enabled:

```bash
gcc-formed --formed-trace=always ...
```

If you invoke through `g++-formed`, use the same flag there.

## 3. Gather The Support Artifacts

Collect at least:

- `trace.json`
- preserved `stderr.raw`
- normalized IR such as `ir.analysis.json` when present
- `diagnostics.sarif` when present

The quickest way is to attach the whole trace directory after review, not only `trace.json`.

## 4. Redaction Checklist

Before sharing the bundle, review it for:

- usernames or home-directory paths
- proprietary source paths or file names
- project-specific compiler flags
- private include paths
- any copied source excerpts that should not leave the environment

If you must redact, keep these fields intact whenever possible:

- selected mode
- support tier
- fallback reason
- backend version
- target triple
- artifact file names (`trace.json`, `stderr.raw`, `ir.analysis.json`, `diagnostics.sarif`)

Do not upload trace bundles to public issues for embargoed or security-sensitive problems; use [SECURITY.md](../../SECURITY.md).

## 5. Minimal Public Bug Packet

If a full trace bundle cannot be shared, include:

1. `gcc-formed --formed-version=verbose`
2. `gcc-formed --formed-self-check`
3. the failing command line with any necessary redaction
4. the compatibility banner line, if one was printed
5. a note describing which trace artifacts were available locally
