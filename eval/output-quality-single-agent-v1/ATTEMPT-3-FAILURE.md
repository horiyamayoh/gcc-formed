# V1 attempt 3 qualification result

The final v1 sealed partition is immutable failure evidence. Candidate
`0e518b81ca235082b5d4674c048c0218e8d75de3` completed 360 of 360 valid trials,
120 per concealed condition, with artifact integrity passing. The
preregistered analyzer was run once and returned `fail`.

The deterministic readability proxy and improvement requirement passed, but
the absolute fidelity gate found 28 condition-identity leaks. The controller
had appended compiler-driver stdout containing the explicit candidate preset
to `DIAGNOSTIC.txt`. Non-inferiority also failed the preregistered upper bounds
for wrong-file-or-anchor rate and tool calls against both comparators. The
missing-header cases exposed a frozen oracle defect: valid newly created
headers were not represented by `git diff` and were classified as wrong
anchors.

Observed task summaries were:

| Condition | Success within 3 loops | Wrong file/anchor | Mean tool calls | Mean diagnostic bytes |
|---|---:|---:|---:|---:|
| Candidate | 119/120 | 1/120 | 2.4417 | 557.1 |
| Current default | 120/120 | 0/120 | 2.0833 | 1256.8 |
| Native GCC | 117/120 | 3/120 | 2.1583 | 950.3 |

This packet is not reanalyzed, repaired, pooled, or promoted. Its protocol,
candidate, and raw packet hashes remain the historical v1 result. The v2
protocol fixes the concealment and new-file evidence defects before any v2
task is materialized and uses a source-disjoint family epoch.
