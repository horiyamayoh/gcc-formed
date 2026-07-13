# V2 attempt 1 qualification result

Candidate `d1ccc838c21bc060c4f958501363d83d3e52cc4c` and release binary
`6c7588440fbd328d5c1ae938b43d82d848509f2ff7aeafd957a80b6de44144fc`
completed 360 of 360 valid trials, 120 per condition, in one uninterrupted
single-agent controller process. Artifact integrity, fidelity, deterministic
readability, task success, wrong-anchor safety, and the improvement requirement
passed. The preregistered verdict was `inconclusive` because the 97.5% upper
bound for tool-call ratio exceeded 1.10 against both controls.

| Condition | Success within 3 loops | Wrong file/anchor | Mean tool calls | Mean diagnostic bytes |
|---|---:|---:|---:|---:|
| Candidate | 120/120 | 0/120 | 2.6083 | 504.4 |
| Current default | 120/120 | 0/120 | 2.2333 | 1214.2 |
| Native GCC | 120/120 | 0/120 | 2.2833 | 934.2 |

The revealed transcripts identified a concrete candidate-only inefficiency.
The compact candidate displayed a shortest-unambiguous basename such as
`main.c:1:25` while the editable path was `src/main.c`. In affected trials the
agent first attempted to open the displayed basename, received a file-not-found
error, and spent another tool call finding the actual path. Native GCC retained
`src/main.c`; the current default retained it in its residual evidence.

Attempt 1 remains immutable and is not pooled with later attempts. The path
fidelity correction is a product change and therefore requires a new candidate
SHA and the preregistered fresh attempt-2 source partition.
