# agent-loop

`agent-loop` — single-run оркестратор для `bd`-задач, который:
- берёт parent-задачу (`--task` или `--bootstrap`);
- делает `memory-bank PRIME/PREPARE + product-storm + spec-first`;
- создаёт child-задачи (dedup + blocking dependencies);
- возвращает итеративный командный план для внешнего агента.

Важно: loop **не запускает project-команды** (`cargo fmt/clippy/test`) сам, а только выдаёт их агенту.

Default model: `gpt-5.3-codex`.

## Установка skill для Codex

```bash
mkdir -p "$CODEX_HOME/skills"
cp -R "/Users/nikitakocnev/RustroverProjects/agent-loop/skills/agent-loop-runner" \
  "$CODEX_HOME/skills/agent-loop-runner"
```

После копирования откройте новый чат Codex.

## Минимальная настройка окружения

```bash
export VIBEPROXY_API_KEY="<your-token>"
```

Если задан только `VIBEPROXY_API_KEY`, loop автоматически использует `OPENAI_BASE_URL=http://127.0.0.1:8318`.

## Сборка release

```bash
cd /Users/nikitakocnev/RustroverProjects/agent-loop
cargo build --release
```

## Запуск из любой директории

Существующая задача:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --hindsight-bank agent-loop
```

Bootstrap (если задач ещё нет):

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --bootstrap "Сформировать roadmap для production-ready agent-loop" \
  --hindsight-bank agent-loop
```

## Использование через чат Codex

Шаблон запроса:

```text
Используй skill agent-loop-runner и запусти loop для задачи <bd-task-id>.
Команда: /Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --task <bd-task-id> --hindsight-bank agent-loop
```

Для product-storm без задач:

```text
Используй skill agent-loop-runner и запусти bootstrap loop.
Команда: /Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --bootstrap "..." --hindsight-bank agent-loop
```

## Основные флаги

- `--task <id>`
- `--bootstrap "<goal>"`
- `--model <model-id>`
- `--profile <delivery|discovery|hardening>`
- `--retain-mode <sync|async|off>`
- `--json-events`
- `--max-iterations <n>`
- `--timeout-minutes <n>`
- `--spawn-cap <n>`
- `--dry-run`

`--dry-run` не делает `bd update --claim`, не создаёт child-задачи и не пишет notes/retain.

## Model priority

1. `--model <id>`
2. `AGENT_LOOP_MODEL`
3. default `gpt-5.3-codex`

## Результат выполнения

- `RunOutcome::AgentActionRequired` — loop вернул task id и список команд для агента.
- `RunOutcome::NoReadyWork` — доступных задач нет.
