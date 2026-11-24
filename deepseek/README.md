# REPO / FOLIO

Готовое портфолио по публичному GitHub-профилю: **Rust stable + axum + tokio + reqwest/rustls**, JSON API и лёгкий HTML/CSS/JS-фронтенд. Новые репозитории появляются без редактирования сайта и без пересборки.

## Быстрый старт

Нужен актуальный Rust stable. Зависимости зафиксированы в `Cargo.lock`; проверено на Rust 1.99.1.

```bash
cp .env.example .env
# Укажите свой GITHUB_USERNAME в .env
cargo run
```

PowerShell:

```powershell
Copy-Item .env.example .env
# Откройте .env и укажите GITHUB_USERNAME
cargo run
```

Откройте **http://localhost:8080**. При первом запуске отображается skeleton, пока GitHub возвращает данные. После рестарта данные сразу восстанавливаются из JSON-кэша. Без JavaScript доступна только оболочка сайта — выбран разрешённый в задании вариант JSON API + vanilla frontend, не SSR.

## Docker

```bash
cp .env.example .env
# Настройте GITHUB_USERNAME и при необходимости токены
docker compose up -d --build
docker compose logs -f
```

- Сборка: `rust:slim-bookworm`, BuildKit-кэш Cargo registry/git/target.
- Runtime: `debian:bookworm-slim`, статические файлы встроены в бинарник.
- Пользователь `10001:10001`, read-only root, dropped capabilities, no-new-privileges.
- Именованный volume `portfolio-cache` хранит `/data/cache.json`.
- `HEALTHCHECK` проверяет `/health` каждые 30 секунд.
- Compose публикует порт только на `127.0.0.1`. Для публичного сайта поставьте HTTPS reverse proxy (Caddy/nginx) перед контейнером. Для прямой публикации измените привязку порта осознанно.
- Compose принудительно задаёт внутренний `PORT=8080`, `CACHE_FILE=/data/cache.json`, `LOG_FORMAT=json`; `PORT` из `.env` выбирает внешний порт.

```bash
docker compose down           # данные volume сохраняются
# docker compose down -v      # удалит и кэш — только если это нужно
```

Без Compose:

```bash
docker build -t repo-folio .
docker run -d --name folio -p 127.0.0.1:8080:8080 \
  -e GITHUB_USERNAME=octocat -v folio-data:/data repo-folio
```

## Конфигурация

| Переменная | По умолчанию | Назначение |
|---|---|---|
| `GITHUB_USERNAME` | обязательная | GitHub user или organization |
| `GITHUB_TOKEN` | пусто | GitHub token только на сервере; обычно повышает лимит с 60 до 5000 запросов/час |
| `REFRESH_INTERVAL_SECS` | `300` | Пауза между циклами, 1…2592000 секунд |
| `PORT` | `8080` | Порт приложения; в Compose внешний порт |
| `ADMIN_TOKEN` | пусто | Секрет для POST `/api/refresh`; пустой отключает эндпоинт (401) |
| `PINNED_REPOS` | пусто | Имена или `owner/name` через запятую, в порядке закрепления |
| `EXCLUDED_REPOS` | пусто | Исключения, не возвращаются и не входят в диаграмму |
| `SHOW_FORKS` | `false` | Показывать форки по умолчанию |
| `SHOW_ARCHIVED` | `false` | Показывать архивные по умолчанию |
| `CACHE_FILE` | `data/cache.json` | Путь к JSON-кэшу; пустая строка или `none` отключает диск |
| `LANGUAGE_BUDGET` | `40` | Максимум запросов `/languages` за цикл; положительное число |
| `GITHUB_API_BASE` | `https://api.github.com` | Базовый API URL, например для локального mock-сервера |
| `CORS_ALLOW_ORIGIN` | пусто | Один точный разрешённый origin; по умолчанию same-origin |
| `RUST_LOG` | `github_portfolio=info,tower_http=info,warn` | Фильтр tracing |
| `LOG_FORMAT` | pretty | `json` для структурированных логов |

Boolean env принимает true/false, 1/0, yes/no, on/off; неизвестное значение использует default. Pins/exclusions сопоставляются без учёта регистра. Изменение env требует рестарта, **новые репозитории — нет**.

Генерация ADMIN_TOKEN (при установленном OpenSSL):

```bash
openssl rand -hex 32
```

Не коммитьте `.env` и не передавайте токены в URL. Для публичного профиля не нужны права на приватные репозитории. `.env` исключён из Docker build context и Git.

## Как работает обновление

1. Tokio-задача стартует сразу; HTTP-сервер не ждёт GitHub.
2. Условный `GET /users/{user}` с `If-None-Match`.
3. `/users/{user}/repos?per_page=100&sort=full_name&type=owner` с обходом всех страниц. **У каждой страницы свой ETag и сохранённый Link**. Ответ 304 первой страницы не пропускает последующие — изменения и удаления на них обнаруживаются.
4. Языки загружаются из `/repos/{owner}/{repo}/languages`. Сначала отсутствующие данные, затем репозитории с изменившимся `updated_at`; повторные запросы используют ETag. Пустой language map — корректный результат, а не ошибка.
5. Профиль и список публикуются вместе в `Arc<RwLock<...>>`. Статистика языков догружается постепенно, с сохранением старых breakdown при ошибках. Публичный API не вызывает GitHub напрямую.
6. Снимок, raw-репозитории, per-page ETag/Link и языки сохраняются в JSON через временный файл и rename. Ошибка диска не сбрасывает память. При рестарте чужой профиль/повреждённый JSON игнорируется; текущие pins/exclusions применяются снова.
7. При ошибке profile/repos сохраняется последний целый снимок и выставляется `stale`. При 403/429 клиент выдерживает cooldown по `Retry-After` или rate-limit reset (fallback 60 секунд).
8. Браузер опрашивает API каждые 30 секунд, не опрашивает скрытую вкладку. Изменение фильтров отменяет предыдущий запрос списка. Неизменившиеся карточки не пересоздаются.

При GitHub outage API данных возвращает 503 **только если кэша ещё нет**. Ошибки языка не мешают обновлению профиля и списка: `languages_complete=false` показывает неполную статистику. Без токена большой профиль может догружаться несколько окон rate limit. Условные запросы экономят трафик; отсутствие расхода primary limit для 304 гарантируется GitHub при корректной авторизации, не следует рассчитывать на это анонимно.

Фоновая задача завершается по SIGINT/SIGTERM; сервер дожидается HTTP-соединений. Незавершённое сетевое обновление отменяется, последний записанный снимок остаётся. Атомарная замена файла защищает от частичного JSON, но это не база с гарантиями fsync при потере питания.

## API

### `GET /`
HTML-оболочка, загружающая `/app.css` и `/app.js`. Также доступны `/favicon.svg`, `/robots.txt`.

### `GET /api/profile`

```json
{
  "profile": {
    "login": "octocat", "name": "The Octocat",
    "avatar_url": "https://avatars.githubusercontent.com/u/583231?v=4",
    "html_url": "https://github.com/octocat", "bio": null,
    "location": "San Francisco", "blog": "https://github.blog",
    "twitter_username": null, "company": "@github",
    "followers": 12345, "following": 9, "public_repos": 8
  },
  "repo_count": 8,
  "updated_at": "2026-11-24T12:00:00Z",
  "stale": false
}
```

`updated_at` — время успешной проверки GitHub, включая 304, не время последнего commit.

### `GET /api/repos`

Параметры: `q`, `language`, `topic`, `sort=stars|updated|name`, `show_forks=true|false`, `show_archived=true|false`.

```bash
curl 'http://localhost:8080/api/repos?language=Rust&topic=cli&sort=updated'
```

```json
{
  "repos": [{
    "id": 1, "name": "demo", "full_name": "octocat/demo",
    "description": "Example", "html_url": "https://github.com/octocat/demo",
    "homepage": "https://example.com", "language": "Rust",
    "stargazers_count": 42, "forks_count": 3, "topics": ["cli"],
    "updated_at": "2026-11-24T10:00:00Z", "fork": false,
    "archived": false, "pinned": true
  }],
  "languages": [{"name": "Rust", "bytes": 10000, "color": "#dea584"}],
  "available_languages": ["Rust"], "available_topics": ["cli"], "total": 1,
  "updated_at": "2026-11-24T12:00:00Z", "stale": false,
  "languages_complete": true,
  "defaults": {"show_forks": false, "show_archived": false}
}
```

Поиск по имени/описанию без учёта регистра. Язык и topic — точное совпадение без учёта регистра. Неизвестная сортировка заменяется на stars. Закреплённые идут первыми в порядке env, но **не обходят фильтры/исключения**. `total` — размер результата фильтрации. Facets учитывают видимость fork/archive, но не поиск.

Диаграмма всегда суммирует **байты всех не исключённых репозиториев, включая форки и архивы**, независимо от UI-фильтров. Это не число репозиториев. `languages_complete` проверяет наличие актуального breakdown для каждого такого репозитория.

Оба GET API возвращают `Cache-Control: no-cache`, `ETag` и поддерживают `If-None-Match` → 304.

### `POST /api/refresh`

```bash
curl -X POST http://localhost:8080/api/refresh \
  -H 'Authorization: Bearer YOUR_ADMIN_TOKEN'
# или -H 'x-admin-token: YOUR_ADMIN_TOKEN'
```

Возвращает **202** `{"status":"scheduled"}`. Это подтверждение постановки в очередь, не успешного GitHub fetch. Tokio Notify объединяет повторные сигналы, refresh gate не допускает параллельных циклов. Результат проверяйте по API/логам. Нет/неверный/ненастроенный токен → 401. Сравнение выполняется через `subtle`.

### `GET /health` / `GET /api/health`

```json
{"status":"ok","ready":true,"cached":true,"updated_at":"2026-11-24T12:00:00Z","uptime_secs":42,"refresh_interval_secs":300,"user":"octocat","version":"1.0.0"}
```

Liveness всегда 200, пока процесс отвечает. `ready=false` означает отсутствие кэша. Docker не должен перезапускать исправный процесс из-за outage GitHub. Для freshness контролируйте `stale` в API данных.

Прикладные ошибки — JSON `{"error":"...","code":"..."}`. Ошибки разбора query/method могут возвращаться стандартными axum 400/405.

## Дизайн

**REPO / FOLIO: редакционный стиль, бумага и код.** Кремовый фон `#f4f1e8`, почти чёрный текст `#16170f`, лаймовый акцент `#c8f751`, терракота `#b4462b`. Serif-заголовки Iowan/Palatino/Georgia сочетаются с системным sans-serif и monospace для названий репозиториев и метаданных. Нет загрузки сторонних шрифтов.

Mobile-first сетка, sticky профиль на больших экранах, светлая/тёмная темы с `prefers-color-scheme` и localStorage, focus rings, skip link, alt-тексты, live status, skeleton/empty/error состояния. Анимации отключаются при `prefers-reduced-motion`. Данные вставляются через `textContent`; URL допускают только HTTP(S); inline JS запрещён CSP.

## Структура

```text
.
├── Cargo.toml / Cargo.lock
├── Dockerfile / docker-compose.yml
├── .dockerignore / .gitignore / .env.example
├── README.md / LICENSE / Makefile
├── .github/workflows/ci.yml
├── src/
│   ├── main.rs / lib.rs
│   ├── config.rs / error.rs / state.rs
│   ├── cache.rs / refresh.rs / filters.rs
│   ├── colors.rs / util.rs
│   ├── github/{mod.rs,client.rs,error.rs}
│   ├── models/{mod.rs,github.rs,snapshot.rs}
│   └── routes/{mod.rs,api.rs,assets.rs,health.rs}
├── static/{index.html,app.css,app.js,favicon.svg}
└── tests/
    ├── integration.rs
    └── fixtures/{user.json,repos.json,languages.json}
```

## Проверки

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
node --check static/app.js
docker build -t github-portfolio:local .
```

Проверено: 40 unit-тестов и 2 интеграционных теста, Clippy без предупреждений, multi-stage Docker build и запуск под UID 10001. Интеграционные тесты используют локальный axum mock, не расходуют GitHub quota: cold cache, ETag, новая репозитория, изменение второй страницы при 304 первой, fallback на ошибке, авторизация refresh.

В браузере проверены реальные профили octocat и tokio-rs, поиск/empty/reset, темы и API. Lighthouse в проверенном desktop-состоянии: **Accessibility 100, Best Practices 100, SEO 100**, без замечаний. Это не гарантия SEO индексируемости динамического контента или всех состояний accessibility; полноценный визуальный/mobile аудит не выполнен. При реальном исчерпании GitHub quota подтверждено обслуживание старого кэша.

## Допущения и ограничения

1. Один публичный профиль на процесс, без базы данных и аккаунтов. API `/users/{user}/repos?type=owner` возвращает принадлежащие пользователю репозитории **вместе с форками**; форки скрывает приложение, не GitHub.
2. JSON API + vanilla frontend вместо SSR. README репозиториев не загружается (опциональная функция).
3. Pins задаются env, не GraphQL GitHub pins. EXCLUDED_REPOS скрывает публичное отображение, но raw-кэш содержит исходные публичные данные — это не механизм хранения секретов.
4. Обновление eventual-consistent: обычно до N секунд + длительность fetch + до 30 секунд браузерного polling. GitHub eventual consistency и rate limit могут увеличить задержку.
5. Статистика языков инкрементальная. Для больших аккаунтов настоятельно рекомендуется GitHub token; бюджет не гарантирует укладывание в анонимный лимит.
6. Пагинация имеет защитный предел 1000 страниц (до 100000 репозиториев). При превышении сохраняется старый кэш вместо публикации обрезанного списка.
7. При изменении репозиториев во время пагинации возможна временная несогласованность GitHub; повторный цикл её исправит, дубликаты по id удаляются.
8. Кэш предназначен для одного процесса на volume. Для нескольких реплик нужны отдельные кэши либо внешний shared store.
9. Админ-токен хранится только в env сервера. В production используйте HTTPS и rate limiting на reverse proxy. CORS не является механизмом авторизации.
10. GitHub username в примере — octocat; замените на свой.
