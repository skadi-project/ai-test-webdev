"use strict";

const FRONTEND_REFRESH_MS = 60_000;
const FILTER_DEBOUNCE_MS = 250;

const state = {
  profile: null,
  allRepositories: [],
  languages: [],
  hasData: false,
  loading: false,
  filterController: null,
  filters: { q: "", language: "", topic: "", sort: "stars" },
  noticeTimer: null,
};

const elements = {
  profile: document.getElementById("profile"),
  profileSkeleton: document.getElementById("profile-skeleton"),
  languagesSkeleton: document.getElementById("languages-skeleton"),
  languagesContent: document.getElementById("languages-content"),
  languagesEmpty: document.getElementById("languages-empty"),
  languageChart: document.getElementById("language-chart"),
  languageList: document.getElementById("language-list"),
  repositoryGrid: document.getElementById("repo-grid"),
  repositoryCount: document.getElementById("repo-count"),
  syncTime: document.getElementById("sync-time"),
  search: document.getElementById("search"),
  languageFilter: document.getElementById("language-filter"),
  topicFilter: document.getElementById("topic-filter"),
  sortFilter: document.getElementById("sort-filter"),
  globalError: document.getElementById("global-error"),
  globalErrorMessage: document.getElementById("global-error-message"),
  notice: document.getElementById("notice"),
  noticeText: document.getElementById("notice-text"),
  themeToggle: document.getElementById("theme-toggle"),
  themeLabel: document.getElementById("theme-label"),
};

const languageColors = {
  JavaScript: "#d4b21f",
  TypeScript: "#3178c6",
  Rust: "#a7472a",
  Python: "#3776ab",
  Go: "#00add8",
  Java: "#b07219",
  Kotlin: "#a97bff",
  Swift: "#f05138",
  C: "#777777",
  "C++": "#f34b7d",
  "C#": "#68217a",
  Ruby: "#a91401",
  PHP: "#777bb4",
  HTML: "#e34c26",
  CSS: "#663399",
  SCSS: "#c6538c",
  Shell: "#89e051",
  Dart: "#00b4ab",
  Elixir: "#6e4a7e",
  Haskell: "#5e5086",
  Lua: "#5151a7",
  Vue: "#41b883",
  Svelte: "#ff3e00",
  Zig: "#ec915c",
  Dockerfile: "#384d54",
};

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}

function safeExternalUrl(value) {
  if (!value || typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  try {
    const url = new URL(candidate);
    return ["http:", "https:"].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

function externalLink(label, href, className = "") {
  const link = element("a", className, label);
  link.href = href;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  return link;
}

function formatNumber(value) {
  return new Intl.NumberFormat("ru-RU").format(value || 0);
}

function formatDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "дата неизвестна";
  return new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatShortDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  }).format(date);
}

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 Б";
  const units = ["Б", "КБ", "МБ", "ГБ"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 1 }).format(value)} ${units[index]}`;
}

function pluralizeRepositories(count) {
  const mod100 = count % 100;
  const mod10 = count % 10;
  if (mod100 >= 11 && mod100 <= 14) return "репозиториев";
  if (mod10 === 1) return "репозиторий";
  if (mod10 >= 2 && mod10 <= 4) return "репозитория";
  return "репозиториев";
}

function renderProfile(profile) {
  const topLine = element("div", "profile-topline");
  const avatarFrame = element("div", "avatar-frame");
  const avatarUrl = safeExternalUrl(profile.avatar_url);
  if (avatarUrl) {
    const avatar = element("img", "avatar");
    avatar.src = avatarUrl;
    avatar.alt = `Аватар GitHub-пользователя ${profile.login}`;
    avatar.width = 142;
    avatar.height = 142;
    avatar.fetchPriority = "high";
    avatarFrame.append(avatar);
  }
  avatarFrame.append(element("span", "avatar-index", "GH"));
  topLine.append(avatarFrame);

  const githubUrl = safeExternalUrl(profile.html_url);
  if (githubUrl) {
    const githubLink = externalLink("GitHub ↗", githubUrl, "github-link");
    githubLink.setAttribute("aria-label", `Открыть профиль ${profile.login} на GitHub`);
    topLine.append(githubLink);
  }

  const identity = element("div", "profile-identity");
  identity.append(element("div", "eyebrow", `@${profile.login} / DEVELOPER`));
  identity.append(element("h1", "", profile.name || profile.login));
  identity.append(element("p", "profile-bio", profile.bio || "Открытый код, проекты и эксперименты — всё из моего GitHub-профиля."));

  const metadata = element("ul", "profile-meta");
  const addMetadata = (text, href = null) => {
    if (!text) return;
    const item = element("li");
    item.append(element("span", "meta-dot"));
    item.setAttribute("aria-hidden", "false");
    item.append(href ? externalLink(text, href) : element("span", "", text));
    metadata.append(item);
  };

  addMetadata(profile.location);
  if (profile.company) {
    const companyName = profile.company.replace(/^@/, "");
    const companyUrl = /^[a-z\d-]+$/i.test(companyName)
      ? `https://github.com/${encodeURIComponent(companyName)}`
      : null;
    addMetadata(profile.company, companyUrl);
  }
  const blogUrl = safeExternalUrl(profile.blog);
  if (blogUrl) addMetadata("Сайт / блог ↗", blogUrl);
  if (profile.twitter_username) {
    addMetadata(
      `@${profile.twitter_username}`,
      `https://twitter.com/${encodeURIComponent(profile.twitter_username)}`,
    );
  }

  const stats = element("div", "profile-stats");
  stats.append(
    createStat(profile.public_repos, "публичных репозиториев"),
    createStat(profile.followers, "подписчиков"),
  );

  elements.profile.replaceChildren(topLine, identity, metadata, stats);
  document.title = `${profile.name || profile.login} — GitHub Portfolio`;
}

function createStat(value, label) {
  const stat = element("div", "stat");
  stat.append(element("strong", "stat-value", formatNumber(value)));
  stat.append(element("span", "stat-label", label));
  return stat;
}

function colorForLanguage(language) {
  if (languageColors[language]) return languageColors[language];
  let hash = 0;
  for (const character of language) hash = (hash * 31 + character.codePointAt(0)) % 360;
  return `hsl(${hash} 58% 48%)`;
}

function renderLanguages(statistics) {
  if (!Array.isArray(statistics) || statistics.length === 0) {
    elements.languagesContent.hidden = true;
    elements.languagesEmpty.hidden = false;
    elements.languagesEmpty.textContent = "GitHub не сообщил данных о языках для этих репозиториев.";
    return;
  }

  const totalBytes = statistics.reduce((sum, item) => sum + Number(item.bytes || 0), 0);
  if (totalBytes <= 0) {
    elements.languagesContent.hidden = true;
    elements.languagesEmpty.hidden = false;
    return;
  }

  const visible = statistics.slice(0, 6).map((item) => ({ ...item }));
  const remainingBytes = statistics.slice(6).reduce((sum, item) => sum + Number(item.bytes || 0), 0);
  if (remainingBytes > 0) visible.push({ name: "Другие", bytes: remainingBytes });

  let cursor = 0;
  const segments = visible.map((item) => {
    const start = cursor;
    cursor += (Number(item.bytes) / totalBytes) * 100;
    return `${colorForLanguage(item.name)} ${start.toFixed(2)}% ${cursor.toFixed(2)}%`;
  });
  elements.languageChart.style.background = `conic-gradient(${segments.join(", ")})`;
  elements.languageChart.setAttribute(
    "aria-label",
    statistics.map((item) => `${item.name}: ${item.percentage}%`).join(", "),
  );

  const fragment = document.createDocumentFragment();
  visible.forEach((item) => {
    const row = element("li", "language-item");
    const swatch = element("span", "language-swatch");
    swatch.style.setProperty("--language-color", colorForLanguage(item.name));
    swatch.setAttribute("aria-hidden", "true");
    const percentage = (Number(item.bytes) / totalBytes) * 100;
    row.append(
      swatch,
      element("span", "language-name", item.name),
      element("span", "language-share", `${percentage.toFixed(1)}%`),
      element("span", "language-bytes", formatBytes(Number(item.bytes))),
    );
    fragment.append(row);
  });
  elements.languageList.replaceChildren(fragment);
  elements.languagesEmpty.hidden = true;
  elements.languagesContent.hidden = false;
}

function populateFilters(repositories) {
  const languages = [...new Set(repositories.map((repo) => repo.language).filter(Boolean))]
    .sort((left, right) => left.localeCompare(right, "ru"));
  const topics = [...new Set(repositories.flatMap((repo) => repo.topics || []))]
    .sort((left, right) => left.localeCompare(right, "ru"));

  updateSelectOptions(elements.languageFilter, languages, "Все языки", "language");
  updateSelectOptions(elements.topicFilter, topics, "Все темы", "topic");
}

function updateSelectOptions(select, values, placeholder, filterName) {
  const options = [new Option(placeholder, ""), ...values.map((value) => new Option(value, value))];
  select.replaceChildren(...options);
  if (!values.includes(state.filters[filterName])) state.filters[filterName] = "";
  select.value = state.filters[filterName];
}

function renderRepositories(repositories) {
  const fragment = document.createDocumentFragment();
  if (repositories.length === 0) {
    const empty = element("div", "empty-state");
    empty.append(element("span", "empty-glyph", "{ ∅ }"));
    const hasFilters = Boolean(state.filters.q || state.filters.language || state.filters.topic);
    empty.append(element("h3", "", hasFilters ? "Ничего не найдено" : "Пока нет репозиториев"));
    empty.append(element("p", "", hasFilters
      ? "Попробуйте изменить запрос или сбросить фильтры."
      : "Новые публичные репозитории появятся здесь после синхронизации с GitHub."));
    if (hasFilters) {
      const reset = element("button", "text-button", "Сбросить фильтры");
      reset.type = "button";
      reset.addEventListener("click", resetFilters);
      empty.append(reset);
    }
    fragment.append(empty);
  } else {
    repositories.forEach((repository, index) => fragment.append(createRepositoryCard(repository, index)));
  }
  elements.repositoryGrid.replaceChildren(fragment);
  elements.repositoryCount.textContent = `${formatNumber(repositories.length)} ${pluralizeRepositories(repositories.length)}`;
}

function createRepositoryCard(repository, index) {
  const card = element("article", `repo-card${repository.pinned ? " pinned" : ""}`);
  card.style.setProperty("--delay", `${Math.min(index * 45, 270)}ms`);

  const header = element("div", "repo-card-header");
  header.append(element("span", "repo-number", `/${String(index + 1).padStart(3, "0")}`));
  const flag = repository.pinned ? "pinned" : repository.archived ? "archived" : repository.fork ? "fork" : "public";
  header.append(element("span", "repo-flag", flag));

  const title = element("h3");
  const repositoryUrl = safeExternalUrl(repository.html_url);
  if (repositoryUrl) title.append(externalLink(repository.name, repositoryUrl));
  else title.textContent = repository.name;

  const description = element(
    "p",
    "repo-description",
    repository.description || "Описание не указано в репозитории.",
  );

  const topics = element("ul", "repo-topics");
  (repository.topics || []).forEach((topic) => topics.append(element("li", "topic", `#${topic}`)));

  const footer = element("div", "repo-footer");
  if (repository.language) {
    const language = element("span", "repo-language");
    const dot = element("span", "repo-language-dot");
    dot.style.setProperty("--language-color", colorForLanguage(repository.language));
    dot.setAttribute("aria-hidden", "true");
    language.append(dot, element("span", "", repository.language));
    footer.append(language);
  }
  footer.append(
    element("span", "repo-metric", `★ ${formatNumber(repository.stargazers_count)}`),
    element("span", "repo-metric", `⑂ ${formatNumber(repository.forks_count)}`),
  );

  const homepageUrl = safeExternalUrl(repository.homepage);
  if (homepageUrl) {
    const homepage = externalLink("Live ↗", homepageUrl, "repo-homepage");
    homepage.setAttribute("aria-label", `Открыть сайт проекта ${repository.name}`);
    footer.append(homepage);
  }

  const updated = element("time", "repo-date", formatShortDate(repository.updated_at));
  updated.dateTime = repository.updated_at;
  updated.title = `Обновлён: ${formatDate(repository.updated_at)}`;
  footer.append(updated);

  card.append(header, title, description, topics, footer);
  return card;
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    cache: "no-store",
    headers: { Accept: "application/json" },
    ...options,
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(payload?.message || `Запрос завершился с ошибкой HTTP ${response.status}`);
  }
  return payload;
}

async function applyFilters() {
  if (state.filterController) state.filterController.abort();
  const controller = new AbortController();
  state.filterController = controller;
  elements.repositoryGrid.setAttribute("aria-busy", "true");

  const parameters = new URLSearchParams();
  Object.entries(state.filters).forEach(([key, value]) => {
    if (value) parameters.set(key, value);
  });
  const suffix = parameters.size ? `?${parameters}` : "";

  try {
    const response = await fetchJson(`/api/repos${suffix}`, { signal: controller.signal });
    if (controller !== state.filterController) return;
    renderRepositories(response.data);
    updateSyncTime(response.updated_at);
  } finally {
    if (controller === state.filterController) {
      state.filterController = null;
      elements.repositoryGrid.setAttribute("aria-busy", "false");
    }
  }
}

async function loadPortfolio() {
  if (state.loading || document.hidden) return;
  const hadData = state.hasData;
  state.loading = true;
  elements.globalError.hidden = true;
  if (!hadData) {
    elements.profileSkeleton.hidden = false;
    elements.repositoryGrid.setAttribute("aria-busy", "true");
  }

  try {
    const [profileResponse, repositoriesResponse, languagesResponse] = await Promise.all([
      fetchJson("/api/profile"),
      fetchJson("/api/repos"),
      fetchJson("/api/languages"),
    ]);
    state.profile = profileResponse.data;
    state.allRepositories = repositoriesResponse.data;
    state.languages = languagesResponse.data;

    renderProfile(state.profile);
    populateFilters(state.allRepositories);
    renderLanguages(state.languages);
    await applyFilters();

    state.hasData = true;
    elements.profileSkeleton.hidden = true;
    elements.profile.hidden = false;
    elements.languagesSkeleton.hidden = true;
    elements.globalError.hidden = true;
    setFiltersEnabled(true);
    updateSyncTime(repositoriesResponse.updated_at);
  } catch (error) {
    if (error.name === "AbortError") return;
    if (hadData) {
      showNotice("Не удалось получить свежие данные. Показываем последнюю успешную версию.");
    } else {
      showFatalError(error.message);
    }
  } finally {
    state.loading = false;
    elements.repositoryGrid.setAttribute("aria-busy", "false");
  }
}

function showFatalError(message) {
  elements.profileSkeleton.hidden = true;
  elements.profile.hidden = true;
  elements.languagesSkeleton.hidden = true;
  elements.languagesContent.hidden = true;
  elements.languagesEmpty.hidden = false;
  elements.languagesEmpty.textContent = "Статистика временно недоступна.";
  elements.repositoryGrid.replaceChildren();
  elements.globalErrorMessage.textContent = message || "Не удалось загрузить данные. Попробуйте ещё раз.";
  elements.globalError.hidden = false;
  elements.repositoryCount.textContent = "нет данных";
  elements.syncTime.textContent = "синхронизация не удалась";
}

function updateSyncTime(value) {
  elements.syncTime.textContent = formatDate(value);
  elements.syncTime.dateTime = value;
}

function setFiltersEnabled(enabled) {
  [elements.search, elements.languageFilter, elements.topicFilter, elements.sortFilter]
    .forEach((control) => { control.disabled = !enabled; });
}

function handleFilterError(error) {
  if (error.name !== "AbortError") {
    showNotice("Фильтр не применён: сервер временно недоступен. Текущий список сохранён.");
  }
}

function resetFilters() {
  state.filters = { q: "", language: "", topic: "", sort: "stars" };
  elements.search.value = "";
  elements.languageFilter.value = "";
  elements.topicFilter.value = "";
  elements.sortFilter.value = "stars";
  applyFilters().catch(handleFilterError);
}

function showNotice(message) {
  window.clearTimeout(state.noticeTimer);
  elements.noticeText.textContent = message;
  elements.notice.hidden = false;
  state.noticeTimer = window.setTimeout(() => { elements.notice.hidden = true; }, 8000);
}

function updateThemeControl() {
  const dark = document.documentElement.dataset.theme === "dark";
  elements.themeToggle.setAttribute("aria-pressed", String(dark));
  elements.themeToggle.setAttribute("aria-label", dark ? "Включить светлую тему" : "Включить тёмную тему");
  elements.themeLabel.textContent = dark ? "Тёмная" : "Светлая";
  document.querySelector('meta[name="theme-color"]').content = dark ? "#0d1411" : "#f4f3ed";
}

function toggleTheme() {
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("portfolio-theme", next);
  } catch {
    // The selected theme still applies when storage is unavailable.
  }
  updateThemeControl();
}

let searchTimer;
elements.search.addEventListener("input", () => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    state.filters.q = elements.search.value.trim();
    applyFilters().catch(handleFilterError);
  }, FILTER_DEBOUNCE_MS);
});

elements.languageFilter.addEventListener("change", () => {
  state.filters.language = elements.languageFilter.value;
  applyFilters().catch(handleFilterError);
});
elements.topicFilter.addEventListener("change", () => {
  state.filters.topic = elements.topicFilter.value;
  applyFilters().catch(handleFilterError);
});
elements.sortFilter.addEventListener("change", () => {
  state.filters.sort = elements.sortFilter.value;
  applyFilters().catch(handleFilterError);
});

document.getElementById("filters").addEventListener("submit", (event) => {
  event.preventDefault();
  window.clearTimeout(searchTimer);
  state.filters.q = elements.search.value.trim();
  applyFilters().catch(handleFilterError);
});
document.getElementById("retry").addEventListener("click", loadPortfolio);
document.getElementById("notice-close").addEventListener("click", () => {
  elements.notice.hidden = true;
});
elements.themeToggle.addEventListener("click", toggleTheme);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) loadPortfolio();
});
window.addEventListener("online", loadPortfolio);

document.getElementById("current-year").textContent = String(new Date().getFullYear());
updateThemeControl();
loadPortfolio();
window.setInterval(loadPortfolio, FRONTEND_REFRESH_MS);
