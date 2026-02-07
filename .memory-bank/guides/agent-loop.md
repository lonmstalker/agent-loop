# Agent Loop Guide

Run `agent-loop run` to process one parent task end-to-end.

Common modes:
- Standard agent-driven:
  - `agent-loop run --task <id> --driver agent --hindsight-bank agent-loop`
- Bootstrap discovery (no initial tasks):
  - `agent-loop run --bootstrap "<goal>" --driver agent --profile discovery --retain-mode async --json-events --hindsight-bank agent-loop`
- Hardening:
  - `agent-loop run --task <id> --driver autonomous --profile hardening --hindsight-bank agent-loop`
