---
name: agent-loop-runner
description: Запускает и сопровождает `agent-loop run` как оркестратор, который генерирует итеративные команды для внешнего агента (beads + hindsight + memory-bank + spec-first/product-storm + TDD-plan).
---

# Agent Loop Runner

## Purpose
Дать агенту детерминированный способ запуска `agent-loop` одной командой и корректной интерпретации `RunOutcome`.

## Workflow

1. Проверить контекст:
   - рабочая директория: корень репозитория `agent-loop`;
   - доступны `bd`, `hindsight`, `cargo`.

2. Синхронизировать трекер:
   - `bd sync`
   - если `--task` не передан: посмотреть кандидатов через `bd ready`.

3. Гарантировать release-бинарник:
   - `cargo build --release` в корне проекта.

4. Собрать команду:
   - базово:
     `/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --task <task-id> --hindsight-bank agent-loop`
   - bootstrap (если задач ещё нет):
     `/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --bootstrap "<goal>" --hindsight-bank agent-loop`
   - опции:
     `--profile`, `--retain-mode`, `--json-events`, `--model`.

5. Передать креды безопасно:
   - при наличии `OPENAI_API_KEY` используется он;
   - иначе используется `VIBEPROXY_API_KEY`;
   - если задан только `VIBEPROXY_API_KEY`, базовый URL автоматически `http://127.0.0.1:8318`.
   - токены в отчёт не печатать.

6. Выполнить команду и обработать результат:
   - `RunOutcome::AgentActionRequired` -> вернуть task id + командный план;
   - `RunOutcome::NoReadyWork` -> сообщить, что нет доступных задач.

7. После запуска:
   - `bd sync`;
   - кратко сообщить команду и outcome.

## Command templates

Базовый:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --hindsight-bank agent-loop
```

Bootstrap:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --bootstrap "<goal>" \
  --hindsight-bank agent-loop
```

С профилем/retain/events:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --profile discovery \
  --retain-mode async \
  --json-events \
  --hindsight-bank agent-loop
```

С override модели:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --model <model-id>
```

## Guardrails

- Не использовать `--dry-run`, если пользователь явно не просил dry run.
- Не менять `max_iterations`, `spawn_cap`, `timeout_minutes` без явного запроса.
- `agent-loop` не выполняет `cargo fmt/clippy/test` сам; эти команды выполняет внешний агент по сгенерированному плану.
