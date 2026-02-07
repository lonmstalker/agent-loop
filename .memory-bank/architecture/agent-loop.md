# Agent Loop Architecture

Single-run orchestrator with parent/child task model.

Core behavior:
- Supports `--task` and `--bootstrap` parent selection.
- Generates iterative TDD command plan for external coding agent.
- Does not run project quality commands (`cargo fmt/clippy/test`) internally.
- Can spawn child tasks from `product-storm` with dedup + blocking dependency wiring.
- Retain strategy for hindsight:
  - `sync`, `async`, `off`.
- Optional `--json-events` stream for orchestration integrations.
