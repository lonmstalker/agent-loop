# agent-loop

`agent-loop` — single-run оркестратор для задач из `bd` с workflow:
`memory-bank -> product-storm/spec-first -> TDD (red/green/refactor) -> quality gates -> done evaluator`.

По умолчанию модель: `gpt-5.3-codex`.

## Установка skill для Codex

1. Скопируйте skill в локальные skills Codex:

```bash
mkdir -p "$CODEX_HOME/skills"
cp -R "/Users/nikitakocnev/RustroverProjects/agent-loop/skills/agent-loop-runner" \
  "$CODEX_HOME/skills/agent-loop-runner"
```

2. Откройте новый чат в Codex (или перезапустите сессию), чтобы skill подхватился.

## Минимальная настройка окружения

```bash
export VIBEPROXY_API_KEY="<your-token>"
```

Если задан только `VIBEPROXY_API_KEY`, loop автоматически использует `OPENAI_BASE_URL=http://127.0.0.1:8318`.

## Сборка release (один раз заранее)

```bash
cd /Users/nikitakocnev/RustroverProjects/agent-loop
cargo build --release
```

После этого можно запускать loop из любой директории.

## Запуск loop из любой директории

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task <bd-task-id> \
  --hindsight-bank agent-loop
```

Пример:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task agent-loop-84u.2 \
  --hindsight-bank agent-loop
```

## Запуск product-storm без существующих задач

`agent-loop` может сам создать bootstrap parent-задачу и сразу запустить loop:

```bash
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --bootstrap "Сформировать roadmap для production-ready agent-loop" \
  --hindsight-bank agent-loop
```

## Опционально: добавить бинарник в PATH

```bash
export PATH="/Users/nikitakocnev/RustroverProjects/agent-loop/target/release:$PATH"
agent-loop run --task agent-loop-84u.2 --hindsight-bank agent-loop
```

## Опционально: установить как global binary через cargo

```bash
cargo install --path /Users/nikitakocnev/RustroverProjects/agent-loop --force
agent-loop run --task agent-loop-84u.2 --hindsight-bank agent-loop
```

## Шаблон запроса в чат Codex

Используйте такой промпт:

```text
Используй skill agent-loop-runner и запусти loop для задачи agent-loop-84u.2.
Команда: /Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run --task agent-loop-84u.2 --hindsight-bank agent-loop
```

## Переопределение модели (опционально)

Приоритет модели:
1. `--model <id>`
2. `AGENT_LOOP_MODEL`
3. default `gpt-5.3-codex`

Пример:

```bash
AGENT_LOOP_MODEL=gpt-5-codex \
/Users/nikitakocnev/RustroverProjects/agent-loop/target/release/agent-loop run \
  --task agent-loop-84u.2
```
