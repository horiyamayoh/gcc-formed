# Confirmatory agent-evaluator collection freeze

The eight files below are the complete, still-blinded confirmatory collection:
8 isolated sessions, 12 trials per session, 96 trials total.  Each file contains
all scheduled packet positions and reports no transport error.

| session | file | SHA-256 |
|---|---|---|
| S04 | `agent-results/S04.json` | `f5ea0e155e61bc30c8cc256fb89ff83ab298c4d0530c2b3bc469935768c7b4bf` |
| S05 | `agent-results/S05.json` | `7c46137c9af7a9a5b8f90eefa1350e0680979339a91130ac5c3a705ec050b6ec` |
| S06 | `agent-results/S06.json` | `51e0acf2e640c2e28018406515cb5adf5f448238f09ed70e7362331af9487c77` |
| S07 | `agent-results/S07-confirmatory.json` | `98cf5eb36c2b3b6bc6cfcdac171c053add7b9089b2208e082893a5544cbbc59e` |
| S08 | `agent-results/S08-confirmatory.json` | `6f5689054b1e92f16846ac4ce474a83190b2ef2f32cc2e95376abc5b5cb76fb3` |
| S09 | `agent-results/S09-confirmatory.json` | `1814b3af01b2c0d5a9ba2fadc36d8fbc88f0d8f07752b55e97bc483eac0caedc` |
| S10 | `agent-results/S10.json` | `43353acf135411ca40426e3f07aaaeebff4dc9b5133480233fde53d3299bcbb3` |
| S11 | `agent-results/S11.json` | `426560725ae59919de3883c3901e592bd89be6bfca20869d4f2f7b0f76eb0a23` |

`agent-results/S09.json` is a retained, one-record transport-failure attempt.  It
is not part of the confirmatory collection and is not eligible for analysis.
Likewise, the previously frozen S01--S03 records remain timing-pipeline pilots,
as preregistered in `PILOT-DISPOSITION.md`.

This manifest and the raw result files must be committed before the private
condition key is opened.  Raw evaluator prose is retained even where it is
wrong or awkward; no response was corrected, removed, or replaced.
