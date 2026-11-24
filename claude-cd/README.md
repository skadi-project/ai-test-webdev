# GitHub Portfolio

Живой сайт-портфолио, который строится из GitHub REST API. Rust-сервис на Axum отдаёт SSR-страницу, кэширует snapshot на диске и в памяти, а фоновый worker обновляет данные без пересборки приложения.

## Концепция дизайна

Интерфейс использует mobile-first сетку карточек, глубокий navy для текста и фиолетово-cyan accent как визуальный сигнал активности. Стек системных sans-serif шрифтов не требует загрузки внешних ресурсов. `prefers-color-scheme` и кнопка темы поддерживают светлый и тёмный режимы; JS остаётся progressive enhancement, поэтому HTML доступен без него.

## Запуск локально

```powershell
Copy-Item .env.example .env
cargo run
```

Откройте http://localhost:8080. Для production-like проверки:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Docker

```powershell
docker compose up -d --build
```

Кэш сохраняется в volume `portfolio-cache`. Образ собирается в два этапа и запускается от пользователя `appuser`.

## Переменные окружения

| Переменная | Назначение |
|---|---|
| `GITHUB_USERNAME` | обязательный логин GitHub |
| `GITHUB_TOKEN` | опциональный token для большего rate limit |
| `ADMIN_TOKEN` | token заголовка `x-admin-token` для ручного refresh |
| `REFRESH_INTERVAL_SECS` | период фонового обновления, default `300` |
| `PORT` | HTTP-порт, default `8080` |
| `CACHE_DIR` | директория snapshot-кэша |
| `PINNED_REPOS` | имена через запятую, выводятся первыми |
| `EXCLUDED_REPOS` | имена через запятую, не показываются |
| `SHOW_FORKS` | показывать fork-репозитории, default `false` |
| `SHOW_ARCHIVED` | показывать archived-репозитории, default `false` |

## API

- `GET /` — SSR portfolio.
- `GET /api/profile` — профиль GitHub в JSON.
- `GET /api/repos?search=&language=&topic=&sort=updated|stars|name` — отфильтрованные репозитории.
- `GET /api/languages` — суммарная статистика языков по `/languages`.
- `GET /health` — healthcheck.
- `POST /api/refresh` — ручное обновление; передайте `x-admin-token: $env:ADMIN_TOKEN`.

GitHub API ошибки не очищают рабочий кэш: сайт продолжает отдавать последний успешный snapshot и пишет ошибку в structured logs. При старте snapshot загружается из JSON, а записи сохраняются атомарной заменой временного файла.

## Дерево проекта

```text
src/{main,config,error,models,github,cache,routes}.rs
static/{app.js,styles.css}
templates/index.html
Dockerfile  docker-compose.yml  .env.example  README.md
```

## Допущения

- Для простоты используются один snapshot и максимум пять страниц репозиториев (до 500 repos), чего достаточно для персонального portfolio.
- GitHub ETag сохраняется в модели snapshot как расширяемая metadata; текущий refresh безопасно переиспользует snapshot при сетевых ошибках.
- `/languages` запрашивается для видимых репозиториев последовательно, чтобы не создавать burst запросов.
