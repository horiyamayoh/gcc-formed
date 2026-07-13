# V2 attempt 2 qualification result

Candidate `efa3dfc64905baf5fcd96811d35e50e1ed8387c9` and release binary
`cb08895d7dd13acdc642e56128a57bcd794e8e352922c9386e4ab7d3474b0bae`
completed 360 of 360 valid trials, 120 per condition, in one uninterrupted
single-agent controller process. The preregistered analyzer returned `pass`
and the independent packet verifier returned `pass`.

| Condition | Success within 3 loops | Wrong file/anchor | Mean tool calls | Mean diagnostic bytes |
|---|---:|---:|---:|---:|
| Candidate | 120/120 | 0/120 | 2.2083 | 509.5 |
| Current default | 120/120 | 0/120 | 2.2750 | 1214.2 |
| Native GCC | 120/120 | 0/120 | 2.3500 | 934.2 |

The candidate tool-call ratio was 0.9707 against the current default with a
97.5% interval of 0.9063–1.0308, and 0.9397 against native GCC with an interval
of 0.8758–1.0000. Diagnostic-byte ratios were 0.4196 and 0.5454 respectively.
Compile-loop ratios were 1.0, task-success differences were 0, and every
wrong-file-or-anchor difference was 0.

Fidelity stop-ship counts, condition leaks, invalid schemas, missing artifacts,
and hash mismatches were all zero. Raw fact coverage, observable RepairUnit
recall, and visible precision were all 1.0. The deterministic readability
contract passed. These are coding-agent task-performance and deterministic
display-contract results, not a human behavioral study.

The checked-in `evidence/` files are byte-identical to the verified packet
summaries. The full raw per-trial packet is retained for the RC release asset;
its Merkle root is
`8d4ffe765730d46c4a496ccd2d3c98a92a4b7e5d1666d8ac99d9b38c39dfd676`.
