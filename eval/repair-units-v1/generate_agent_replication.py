#!/usr/bin/env python3
"""Freeze a prospective, disjoint-fixture replication of the agent study."""

from pathlib import Path
import generate_agent_packets as generator

generator.ROOT = Path(__file__).resolve().parent / "agent-replication-v2"
generator.FIXTURES = [
    "corpus/repair-unit-exact-count/single/case-02",
    "corpus/repair-unit-exact-count/single/case-03",
    "corpus/repair-unit-exact-count/single/case-04",
    "corpus/repair-unit-exact-count/single/case-08",
    "corpus/repair-unit-exact-count/double/case-02",
    "corpus/repair-unit-exact-count/double/case-06",
    "corpus/repair-unit-exact-count/triple/case-02",
    "corpus/repair-unit-exact-count/triple/case-06",
    "corpus/real-project/direct-link-order-c/case-04",
    "corpus/real-project/make-generated-c/case-04",
    "corpus/real-project/make-werror-c/case-04",
    "corpus/real-project/cmake-frontier-cpp/case-04",
]
generator.SESSION_COUNT = 24
generator.SESSION_OFFSET = 11
generator.ANSWER_KEY_PATH = Path("/tmp/repair-unit-agent-replication-answer-key.json")
generator.CONDITION_KEY_PATH = Path("/tmp/repair-unit-agent-replication-condition-key.json")

generator.ROOT.mkdir(exist_ok=True)
generator.main()
