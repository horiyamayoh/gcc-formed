---
doc_role: current-authority
lifecycle_status: accepted-baseline
audience: both
use_for: Disposition of agent-evaluator timing pilot sessions before confirmatory collection.
do_not_use_for: Removing unfavorable valid confirmatory trials.
supersedes: []
superseded_by: []
---
> [!IMPORTANT]
> Authority: `current-authority` / `accepted-baseline`
> Use for: Disposition of agent-evaluator timing pilot sessions before confirmatory collection.
> Do not use for: Removing unfavorable valid confirmatory trials.

# Pilot timing disposition

Sessions S01–S03 were the first transport pilot. Their recorded per-trial durations were 0–2 ms because each evaluator loaded the entire 12-trial session before starting individual timestamps. That contradicts the preregistered requirement that timing surround packet inspection and makes latency unusable. The raw answers are retained under `agent-results/pilot/`; they are excluded solely as proven timing transport failures, before any correctness scoring or condition unblinding.

The confirmatory collection uses fresh sessions S04–S11. Each agent receives exactly one trial per turn, starts the clock before extracting that trial, commits the first edit, stops the clock, and only then receives the next trial. All 96 confirmatory trials are retained regardless of result.
