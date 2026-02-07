---
name: agent-loop-runner
description: Запускает и сопровождает `agent-loop run` для полного цикла задачи (beads + hindsight + memory-bank + spec-first/product-storm + TDD). Использовать когда пользователь просит "запусти loop", "доведи задачу до done", "прогони agent-loop" или выполнить end-to-end проход по задаче из `bd`.
---

# Agent Loop Runner

## Purpose
Дать агенту детерминированный способ запускать loop одной командой и корректно интерпретировать результат `RunOutcome`.

## When to trigger
- Пользователь просит запустить `agent-loop` для конкретной задачи.
- Пользователь просит "включить loop" и довести задачу из `bd` до результата.
- Нужен единый runnable шаблон для повторяемого запуска loop.

## Workflow

1. Проверить контекст:
   - рабочая директория: корень репозитория `agent-loop`;
   - доступны `bd`, `hindsight`, `cargo`.

2. Синхронизировать трекер:
   - `bd sync`
   - если `--task` не передан пользователем: посмотреть кандидатов через `bd ready`.

3. Гарантировать release-бинарник:
   - сначала выполнить: `cargo build --release` в корне проекта.

4. Собрать команду:
   - базовый запуск из любой директории:
     `/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --task <task-id> --hindsight-bank agent-loop`
   - если задач ещё нет: использовать bootstrap-режим
     `/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --bootstrap "<goal>" --hindsight-bank agent-loop`
   - если нужен discovery-проход без блокировки по clippy:
     добавить `--profile discovery` (создаст quality debt child-задачу при clippy fail)
   - если retain в hindsight не должен блокировать UX:
     добавить `--retain-mode async` (или `--retain-mode off`)
   - если нужна интеграция с оркестратором:
     добавить `--json-events`
   - модель не задавать без явной необходимости (по умолчанию `gpt-5.3-codex`).

5. Передать креды безопасно:
   - при наличии `OPENAI_API_KEY` используется он;
   - иначе используется `VIBEPROXY_API_KEY`;
   - если задан только `VIBEPROXY_API_KEY`, базовый URL автоматически `http://127.0.0.1:8318`.
   - никогда не печатать токены в отчёте.

6. Выполнить команду и обработать результат:
   - `RunOutcome::Done` -> зафиксировать успех;
   - `RunOutcome::NeedsHuman` -> вернуть причину и blocking child tasks;
   - `RunOutcome::NoReadyWork` -> сообщить, что нет доступных задач.

7. После запуска:
   - `bd sync`;
   - кратко сообщить выполненную команду, outcome и следующие ограничения (если есть).

## Command templates

Базовый:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --hindsight-bank agent-loop
```

Bootstrap (когда нет задач в `bd`):

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --bootstrap "<goal>" \
  --hindsight-bank agent-loop
```

Discovery + async retain + json events:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --bootstrap "<goal>" \
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

С env override модели:

```bash
AGENT_LOOP_MODEL=<model-id> \
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id>
```

## Guardrails
- Не использовать `--dry-run`, если пользователь явно не просил dry run.
- Не менять `max_iterations`, `spawn_cap`, `timeout_minutes` без явного запроса.
- Если loop вернул `NeedsHuman`, не скрывать причину и не отмечать задачу как завершённую.
- Для production hardening использовать `--profile hardening`.
