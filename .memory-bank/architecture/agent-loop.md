# Agent Loop Architecture

Single-run orchestrator with parent/child task model and hybrid done evaluator.

Core behavior:
- Supports `--task` and `--bootstrap` parent selection.
- Supports execution drivers:
  - `agent` (default): emits iterative command plan for external agent execution.
  - `autonomous`: executes quality gates and decisions internally.
- Policy profiles:
  - `delivery` (strict gates),
  - `discovery` (default non-blocking `clippy` + quality debt child spawn),
  - `hardening` (strict, ignores non-blocking overrides).
- Retain strategy for hindsight:
  - `sync`, `async`, `off`.
- Optional `--json-events` stream for orchestration integrations.
