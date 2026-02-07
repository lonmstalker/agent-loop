# Agent Loop Guide

Run `agent-loop run` to process one parent task end-to-end.

Common modes:
- Standard delivery:
  - `agent-loop run --task <id> --hindsight-bank agent-loop`
- Bootstrap discovery (no initial tasks):
  - `agent-loop run --bootstrap "<goal>" --profile discovery --retain-mode async --json-events --hindsight-bank agent-loop`
- Hardening:
  - `agent-loop run --task <id> --profile hardening --hindsight-bank agent-loop`
