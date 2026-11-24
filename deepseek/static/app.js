/* REPO / FOLIO — client
 * Vanilla ES2020, no dependencies. Builds the repo list from the JSON API and
 * keeps it in sync without reloading the page.
 */
(() => {
  "use strict";

  /* --- Constants -------------------------------------------------------- */
  const POLL_INTERVAL_MS = 30_000;
  const REQUEST_TIMEOUT_MS = 12_000;
  const SEARCH_DEBOUNCE_MS = 220;
  const THEME_KEY = "repofolio-theme";
  const LANGS_SHOWN = 7;

  const GITHUB_ICON =
    '<svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" fill="currentColor">' +
    '<path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.4 7.4 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A7.995 7.995 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/></svg>';

  const STAR_ICON =
    '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="currentColor">' +
    '<path d="M8 1.3l2.05 4.15 4.58.67-3.31 3.23.78 4.56L8 11.76 3.9 13.91l.78-4.56L1.37 6.12l4.58-.67L8 1.3Z"/></svg>';

  const FORK_ICON =
    '<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="currentColor">' +
    '<path d="M5 3.5a1.5 1.5 0 1 1 0-.01ZM5 5.42a3 3 0 1 0 0-.01Zm6 0a3 3 0 1 0 0-.01ZM5 10.58a3 3 0 1 0 0-.01ZM12.5 8.5v1a1 1 0 0 1-1 1H9.5a1 1 0 0 0-1 1v1.08a3 3 0 1 0 1.5 0V11.5h5v-3h-1.5Z"/></svg>';

  /* --- Elements --------------------------------------------------------- */
  const el = {
    root: document.documentElement,
    themeToggle: document.getElementById("theme-toggle"),
    themeToggleText: document.getElementById("theme-toggle-text"),
    syncNote: document.getElementById("sync-note"),
    profileCard: document.getElementById("profile-card"),
    profileSkeleton: document.getElementById("profile-skeleton"),
    profileBody: document.getElementById("profile-body"),
    profileError: document.getElementById("profile-error"),
    avatar: document.getElementById("avatar"),
    name: document.getElementById("profile-name"),
    handle: document.getElementById("profile-handle"),
    bio: document.getElementById("profile-bio"),
    meta: document.getElementById("profile-meta"),
    followers: document.getElementById("stat-followers"),
    repoStat: document.getElementById("stat-repos"),
    cta: document.getElementById("profile-cta"),
    langChart: document.getElementById("lang-chart"),
    langLegend: document.getElementById("lang-legend"),
    langHint: document.getElementById("languages-hint"),
    search: document.getElementById("search"),
    language: document.getElementById("language"),
    topic: document.getElementById("topic"),
    sort: document.getElementById("sort"),
    showForks: document.getElementById("show-forks"),
    showArchived: document.getElementById("show-archived"),
    repoList: document.getElementById("repo-list"),
    reposCount: document.getElementById("repos-count"),
    notice: document.getElementById("notice"),
    empty: document.getElementById("empty-state"),
    resetFilters: document.getElementById("reset-filters"),
    footerUpdated: document.getElementById("footer-updated"),
    controls: document.getElementById("controls"),
  };

  const state = {
    repos: [],
    languages: [],
    languageBuckets: [],
    defaults: null,
    filters: { q: "", language: "", topic: "", sort: "stars", showForks: null, showArchived: null },
    requestSeq: 0,
    lastUpdated: null,
    languagesComplete: true,
    pollTimer: null,
    inFlight: null,
  };

  /* --- Utilities -------------------------------------------------------- */
  const pad = (value) => String(value).padStart(2, "0");

  function formatNumber(value) {
    if (typeof value !== "number" || !Number.isFinite(value)) return "—";
    return new Intl.NumberFormat("ru-RU").format(value);
  }

  function formatCompact(value) {
    if (typeof value !== "number" || !Number.isFinite(value)) return "0";
    if (value < 1000) return String(value);
    const units = [
      [1_000_000_000, "b"],
      [1_000_000, "m"],
      [1_000, "k"],
    ];
    for (const [size, suffix] of units) {
      if (value >= size) {
        const scaled = value / size;
        const rounded = Math.round(scaled * 10) / 10;
        return `${String(rounded).replace(".0", "")}${suffix}`;
      }
    }
    return String(value);
  }

  function formatDate(iso) {
    if (!iso) return "—";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "—";
    return new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "short", year: "numeric" }).format(date);
  }

  function formatDateTime(iso) {
    if (!iso) return "—";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "—";
    return new Intl.DateTimeFormat("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(date);
  }

  /* Only absolute http(s) URLs may become href/src values. */
  function safeUrl(raw) {
    if (typeof raw !== "string") return null;
    const trimmed = raw.trim();
    if (!trimmed) return null;
    try {
      const url = new URL(trimmed);
      if (url.protocol === "http:" || url.protocol === "https:") return url.href;
    } catch {
      return null;
    }
    return null;
  }

  function externalLink(href, text, className) {
    const link = document.createElement("a");
    link.href = href;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.textContent = text;
    if (className) link.className = className;
    return link;
  }

  function clear(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  function debounce(fn, delay) {
    let handle = null;
    return (...args) => {
      if (handle) clearTimeout(handle);
      handle = setTimeout(() => {
        handle = null;
        fn(...args);
      }, delay);
    };
  }

  function setSyncNote(text, tone) {
    el.syncNote.textContent = text;
    if (tone) el.syncNote.dataset.state = tone;
    else delete el.syncNote.dataset.state;
  }

  function showNotice(text, tone) {
    clear(el.notice);
    const dot = document.createElement("span");
    dot.className = "notice-dot";
    dot.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.textContent = text;
    el.notice.append(dot, label);
    if (tone) el.notice.dataset.tone = tone;
    else delete el.notice.dataset.tone;
    el.notice.hidden = false;
  }

  function hideNotice() {
    el.notice.hidden = true;
    delete el.notice.dataset.tone;
  }

  /* --- Theme ------------------------------------------------------------ */
  function preferredTheme() {
    const stored = safeStorageGet(THEME_KEY);
    if (stored === "light" || stored === "dark") return stored;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }

  function applyTheme(theme) {
    el.root.dataset.theme = theme;
    const isDark = theme === "dark";
    el.themeToggle.setAttribute("aria-pressed", String(isDark));
    el.themeToggleText.textContent = isDark ? "Светлая тема" : "Тёмная тема";
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", isDark ? "#0e0f0c" : "#f4f1e8");
  }

  function safeStorageGet(key) {
    try {
      return window.localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  function safeStorageSet(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch {
      /* private mode / disabled storage: theme just won't persist */
    }
  }

  function initTheme() {
    applyTheme(preferredTheme());
    el.themeToggle.addEventListener("click", () => {
      const next = el.root.dataset.theme === "dark" ? "light" : "dark";
      applyTheme(next);
      safeStorageSet(THEME_KEY, next);
    });
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event) => {
      if (!safeStorageGet(THEME_KEY)) applyTheme(event.matches ? "dark" : "light");
    };
    if (typeof media.addEventListener === "function") media.addEventListener("change", onChange);
    else if (typeof media.addListener === "function") media.addListener(onChange);
  }

  /* --- Networking ------------------------------------------------------- */
  async function apiGet(path, controller = new AbortController()) {
    const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
    try {
      const response = await fetch(path, {
        headers: { Accept: "application/json" },
        signal: controller.signal,
        credentials: "same-origin",
      });
      if (response.status === 304) return { notModified: true, data: null };
      if (!response.ok) {
        let message = `HTTP ${response.status}`;
        try {
          const body = await response.json();
          if (body && typeof body.error === "string") message = body.error;
        } catch {
          /* body was not JSON */
        }
        const error = new Error(message);
        error.status = response.status;
        throw error;
      }
      return { notModified: false, data: await response.json() };
    } finally {
      clearTimeout(timer);
    }
  }

  function reposQuery() {
    const params = new URLSearchParams();
    const f = state.filters;
    if (f.q) params.set("q", f.q);
    if (f.language) params.set("language", f.language);
    if (f.topic) params.set("topic", f.topic);
    params.set("sort", f.sort);
    // Until the server tells us its configured defaults, let it decide.
    if (f.showForks !== null) params.set("show_forks", String(f.showForks));
    if (f.showArchived !== null) params.set("show_archived", String(f.showArchived));
    const query = params.toString();
    return `/api/repos${query ? `?${query}` : ""}`;
  }

  /* --- Profile rendering ------------------------------------------------ */
  function renderProfile(profile, updatedAt) {
    if (!profile || typeof profile !== "object") return;
    el.profileCard.setAttribute("aria-busy", "false");
    el.profileSkeleton.hidden = true;
    el.profileError.hidden = true;
    el.profileBody.hidden = false;

    const avatar = safeUrl(profile.avatar_url);
    if (avatar) {
      el.avatar.src = avatar;
      el.avatar.alt = `Аватар ${profile.name || profile.login || "пользователя GitHub"}`;
    } else {
      el.avatar.removeAttribute("src");
      el.avatar.alt = "";
    }

    el.name.textContent = profile.name || profile.login || "Без имени";
    el.handle.textContent = `@${profile.login || "unknown"}`;
    const profileUrl = safeUrl(profile.html_url) || (profile.login ? `https://github.com/${encodeURIComponent(profile.login)}` : "https://github.com");
    el.handle.href = profileUrl;

    if (profile.bio) {
      el.bio.textContent = profile.bio;
      el.bio.hidden = false;
    } else {
      el.bio.hidden = true;
    }

    clear(el.meta);
    const metaRows = [];
    if (profile.location) metaRows.push({ icon: "◉", text: profile.location });
    if (profile.company) {
      const company = profile.company.trim();
      metaRows.push({ icon: "▣", text: company, href: /^@[a-z\d-]+$/i.test(company) ? `https://github.com/${company.slice(1)}` : safeUrl(company) });
    }
    const blog = safeUrl(profile.blog) || (typeof profile.blog === "string" && !profile.blog.includes(":") ? safeUrl(`https://${profile.blog}`) : null);
    if (profile.blog) {
      metaRows.push({ icon: "↗", text: profile.blog, href: blog });
    }
    if (profile.twitter_username) {
      metaRows.push({
        icon: "@",
        text: `@${profile.twitter_username}`,
        href: `https://twitter.com/${encodeURIComponent(profile.twitter_username)}`,
      });
    }
    for (const row of metaRows) {
      const li = document.createElement("li");
      const icon = document.createElement("span");
      icon.className = "meta-icon";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = row.icon;
      li.append(icon);
      if (row.href) {
        const url = safeUrl(row.href);
        if (url) {
          li.append(externalLink(url, row.text));
        } else {
          const span = document.createElement("span");
          span.textContent = row.text;
          li.append(span);
        }
      } else {
        const span = document.createElement("span");
        span.textContent = row.text;
        li.append(span);
      }
      el.meta.append(li);
    }
    el.meta.hidden = metaRows.length === 0;

    el.followers.textContent = formatNumber(profile.followers);
    el.repoStat.textContent = formatNumber(profile.public_repos);
    el.cta.href = profileUrl;
    if (updatedAt) state.lastUpdated = updatedAt;
  }

  function renderProfileError() {
    el.profileCard.setAttribute("aria-busy", "false");
    el.profileSkeleton.hidden = true;
    el.profileBody.hidden = true;
    el.profileError.hidden = false;
  }

  /* --- Language chart --------------------------------------------------- */
  function renderLanguages(languages, complete) {
    const usable = Array.isArray(languages) ? languages.filter((item) => item && item.bytes > 0) : [];
    const total = usable.reduce((sum, item) => sum + item.bytes, 0);

    clear(el.langChart);
    clear(el.langLegend);

    if (!usable.length || total === 0) {
      const placeholder = document.createElement("div");
      placeholder.className = "lang-seg";
      placeholder.style.flex = "1 1 auto";
      placeholder.style.background = "var(--paper-3)";
      el.langChart.append(placeholder);
      el.langChart.setAttribute("aria-busy", "false");
      el.langHint.textContent = "Данные о языках ещё собираются";
      return;
    }

    el.langChart.setAttribute("aria-busy", "false");
    el.langChart.setAttribute(
      "aria-label",
      `Распределение языков: ${usable
        .slice(0, LANGS_SHOWN)
        .map((item) => `${item.name} ${Math.round((item.bytes / total) * 100)}%`)
        .join(", ")}`
    );
    el.langChart.setAttribute("role", "img");

    const shown = usable.slice(0, LANGS_SHOWN);
    const restBytes = usable.slice(LANGS_SHOWN).reduce((sum, item) => sum + item.bytes, 0);
    const buckets = shown.slice();

    for (const item of buckets) {
      const seg = document.createElement("span");
      seg.className = "lang-seg";
      seg.style.flex = `${item.bytes} 1 0`;
      seg.style.background = item.color || "var(--ink-4)";
      el.langChart.append(seg);
    }
    if (restBytes > 0) {
      const seg = document.createElement("span");
      seg.className = "lang-seg";
      seg.style.flex = `${restBytes} 1 0`;
      seg.style.background = "var(--line-strong)";
      el.langChart.append(seg);
    }

    const legendRows = buckets.map((item) => ({
      name: item.name,
      color: item.color,
      pct: Math.round((item.bytes / total) * 100),
    }));
    if (restBytes > 0) {
      legendRows.push({ name: "Другие", color: "var(--line-strong)", pct: Math.round((restBytes / total) * 100) });
    }

    for (const row of legendRows) {
      const li = document.createElement("li");
      const dot = document.createElement("span");
      dot.className = "lang-dot";
      dot.style.background = row.color || "var(--ink-4)";
      dot.setAttribute("aria-hidden", "true");
      const name = document.createElement("span");
      name.className = "lang-name";
      name.textContent = row.name;
      const pct = document.createElement("span");
      pct.className = "lang-pct";
      pct.textContent = `${row.pct}%`;
      li.append(dot, name, pct);
      el.langLegend.append(li);
    }

    state.languageBuckets = legendRows;
    el.langHint.textContent = complete
      ? "По объёму кода во всех репозиториях"
      : "По объёму кода; часть данных ещё догружается";
  }

  /* --- Facets ----------------------------------------------------------- */
  function renderFacets(availableLanguages, availableTopics) {
    syncSelect(el.language, availableLanguages, "Все языки", state.filters.language);
    syncSelect(el.topic, availableTopics, "Все топики", state.filters.topic);
  }

  function syncSelect(select, values, placeholder, current) {
    const items = Array.isArray(values) ? values.filter((value) => typeof value === "string" && value) : [];
    if (current && !items.includes(current)) items.push(current);
    const signature = items.join("\u0000");
    if (select.dataset.signature !== signature) {
      clear(select);
      const base = document.createElement("option");
      base.value = "";
      base.textContent = placeholder;
      select.append(base);
      for (const value of items) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = value;
        select.append(option);
      }
      select.dataset.signature = signature;
    }
    // Keep the current choice only if it still exists.
    if (current && !items.includes(current)) {
      select.value = "";
      if (select === el.language) state.filters.language = "";
      if (select === el.topic) state.filters.topic = "";
    } else {
      select.value = current || "";
    }
  }

  /* --- Repo cards ------------------------------------------------------- */
  function languageColor(name) {
    const found = state.languages.find((item) => item.name === name);
    return found ? found.color : null;
  }

  function buildCard(repo) {
    const card = document.createElement("article");
    card.className = "repo-card";
    card.dataset.pinned = repo.pinned ? "true" : "false";

    const top = document.createElement("div");
    top.className = "repo-top";

    const heading = document.createElement("h3");
    heading.className = "repo-name";
    const repoUrl = safeUrl(repo.html_url);
    if (repoUrl) {
      const link = document.createElement("a");
      link.href = repoUrl;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.textContent = repo.name || repo.full_name;
      heading.append(link);
    } else {
      heading.textContent = repo.name || repo.full_name;
    }
    top.append(heading);

    if (repo.pinned) {
      const badge = document.createElement("span");
      badge.className = "pin-badge";
      badge.textContent = "★ закреплён";
      top.append(badge);
    }
    card.append(top);

    const desc = document.createElement("p");
    desc.className = "repo-desc";
    if (repo.description) {
      desc.textContent = repo.description;
    } else {
      desc.textContent = "Без описания";
      desc.dataset.empty = "true";
    }
    card.append(desc);

    if (Array.isArray(repo.topics) && repo.topics.length) {
      const list = document.createElement("ul");
      list.className = "repo-topics";
      for (const topic of repo.topics.slice(0, 5)) {
        const item = document.createElement("li");
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "topic-chip";
        chip.textContent = topic;
        chip.dataset.topic = topic;
        chip.title = `Фильтровать по топику «${topic}»`;
        item.append(chip);
        list.append(item);
      }
      card.append(list);
    }

    const foot = document.createElement("div");
    foot.className = "repo-foot";

    if (repo.language) {
      const lang = document.createElement("span");
      lang.className = "repo-lang";
      const dot = document.createElement("span");
      dot.className = "lang-dot";
      const color = languageColor(repo.language);
      dot.style.background = color || "var(--ink-4)";
      dot.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.textContent = repo.language;
      lang.append(dot, label);
      foot.append(lang);
    }

    const stars = document.createElement("span");
    stars.className = "repo-metric";
    stars.innerHTML = STAR_ICON; // static, trusted markup
    const starsText = document.createElement("span");
    starsText.textContent = formatCompact(repo.stargazers_count);
    const starsFull = document.createElement("span");
    starsFull.className = "visually-hidden";
    starsFull.textContent = ` ${formatNumber(repo.stargazers_count)} звёзд`;
    stars.append(starsText, starsFull);
    stars.title = `${formatNumber(repo.stargazers_count)} звёзд`;
    foot.append(stars);

    const forks = document.createElement("span");
    forks.className = "repo-metric";
    forks.innerHTML = FORK_ICON; // static, trusted markup
    const forksText = document.createElement("span");
    forksText.textContent = formatCompact(repo.forks_count);
    const forksFull = document.createElement("span");
    forksFull.className = "visually-hidden";
    forksFull.textContent = ` ${formatNumber(repo.forks_count)} форков`;
    forks.append(forksText, forksFull);
    forks.title = `${formatNumber(repo.forks_count)} форков`;
    foot.append(forks);

    const updated = document.createElement("span");
    updated.className = "repo-metric";
    updated.textContent = formatDate(repo.updated_at);
    updated.title = `Обновлён ${formatDateTime(repo.updated_at)}`;
    foot.append(updated);

    if (repo.archived) {
      const flag = document.createElement("span");
      flag.className = "repo-flag";
      flag.textContent = "архив";
      foot.append(flag);
    }
    if (repo.fork) {
      const flag = document.createElement("span");
      flag.className = "repo-flag";
      flag.textContent = "форк";
      foot.append(flag);
    }

    const links = document.createElement("span");
    links.className = "repo-links";
    const homepage = safeUrl(repo.homepage);
    if (homepage) {
      const demo = externalLink(homepage, "демо ↗", "repo-link");
      demo.title = `Открыть ${homepage}`;
      links.append(demo);
    }
    if (repoUrl) {
      const gh = document.createElement("a");
      gh.href = repoUrl;
      gh.target = "_blank";
      gh.rel = "noopener noreferrer";
      gh.className = "repo-link";
      gh.innerHTML = GITHUB_ICON; // static, trusted markup
      gh.setAttribute("aria-label", `Открыть ${repo.full_name || repo.name} на GitHub`);
      gh.title = "Открыть на GitHub";
      links.append(gh);
    }
    if (links.childNodes.length) foot.append(links);

    card.append(foot);
    return card;
  }

  function renderRepos(repos) {
    const signature = JSON.stringify(repos);
    if (el.repoList.dataset.signature === signature) return;
    clear(el.repoList);
    el.repoList.setAttribute("aria-busy", "false");
    el.repoList.dataset.signature = signature;

    if (!Array.isArray(repos) || repos.length === 0) {
      el.empty.hidden = false;
      return;
    }
    el.empty.hidden = true;

    const fragment = document.createDocumentFragment();
    repos.forEach((repo, index) => {
      const card = buildCard(repo);
      // Stagger the entrance a little, capped so long lists feel instant.
      card.style.animationDelay = `${Math.min(index, 8) * 35}ms`;
      fragment.append(card);
    });
    el.repoList.append(fragment);
  }

  function renderSkeletons() {
    delete el.repoList.dataset.signature;
    clear(el.repoList);
    el.repoList.setAttribute("aria-busy", "true");
    for (let index = 0; index < 4; index += 1) {
      const card = document.createElement("article");
      card.className = "sk sk-card";
      card.setAttribute("aria-hidden", "true");
      el.repoList.append(card);
    }
  }

  function renderCount(total) {
    if (typeof total !== "number") {
      el.reposCount.textContent = "";
      return;
    }
    const plural = new Intl.PluralRules("ru-RU").select(total);
    const forms = { one: "репозиторий", few: "репозитория", many: "репозиториев", other: "репозитория" };
    el.reposCount.textContent = `${total} ${forms[plural] || forms.other}`;
  }

  /* --- Data loading ----------------------------------------------------- */
  async function loadProfile() {
    try {
      const { notModified, data } = await apiGet("/api/profile");
      if (notModified || !data) return;
      renderProfile(data.profile, data.updated_at);
      if (data.stale) setSyncNote("кэш устарел · обновим", "stale");
    } catch (error) {
      if (error.status === 503) {
        setSyncNote("ждём первые данные", "stale");
        return;
      }
      if (el.profileBody.hidden) renderProfileError();
      setSyncNote("нет связи с сервером", "error");
    }
  }

  let repoController = null;
  async function loadRepos() {
    const seq = ++state.requestSeq;
    if (repoController) repoController.abort();
    repoController = new AbortController();
    const path = reposQuery();
    try {
      const { notModified, data } = await apiGet(path, repoController);
      if (seq !== state.requestSeq) return; // a newer request superseded this one
      if (notModified || !data) return;

      state.repos = Array.isArray(data.repos) ? data.repos : [];
      state.languages = Array.isArray(data.languages) ? data.languages : [];
      state.languagesComplete = Boolean(data.languages_complete);

      if (data.defaults && typeof data.defaults === "object") {
        const wasUnknown = state.filters.showForks === null || state.filters.showArchived === null;
        state.defaults = data.defaults;
        if (state.filters.showForks === null) el.showForks.checked = Boolean(data.defaults.show_forks);
        if (state.filters.showArchived === null) el.showArchived.checked = Boolean(data.defaults.show_archived);
        if (wasUnknown) {
          // Adopt server defaults, then keep syncing explicitly from now on.
          state.filters.showForks = Boolean(state.filters.showForks === null ? data.defaults.show_forks : state.filters.showForks);
          state.filters.showArchived = Boolean(
            state.filters.showArchived === null ? data.defaults.show_archived : state.filters.showArchived
          );
        }
      }

      renderFacets(data.available_languages, data.available_topics);
      renderLanguages(state.languages, state.languagesComplete);
      renderRepos(state.repos);
      renderCount(typeof data.total === "number" ? data.total : state.repos.length);

      if (data.updated_at) {
        state.lastUpdated = data.updated_at;
        el.footerUpdated.textContent = `Данные GitHub от ${formatDateTime(data.updated_at)}`;
      }

      if (data.stale) {
        showNotice("Данные могли устареть: последнее обновление не удалось. Показываем кэш.", "warn");
        setSyncNote("кэш устарел", "stale");
      } else if (!state.languagesComplete) {
        hideNotice();
        setSyncNote("синхронизировано", "live");
      } else {
        hideNotice();
        setSyncNote("синхронизировано", "live");
      }

      if (state.repos.length === 0) {
        const hasFilters =
          Boolean(state.filters.q || state.filters.language || state.filters.topic) ||
          state.filters.showForks !== state.defaults?.show_forks ||
          state.filters.showArchived !== state.defaults?.show_archived;
        el.empty.hidden = false;
        const title = el.empty.querySelector(".state-title");
        if (title) {
          title.textContent = hasFilters ? "Ничего не нашлось" : "Пока нет репозиториев";
        }
      }
    } catch (error) {
      if (seq !== state.requestSeq) return;
      if (error.status === 503) {
        showNotice("Сервер ещё получает данные с GitHub. Обновим автоматически.", "warn");
        setSyncNote("ждём первые данные", "stale");
        return;
      }
      showNotice("Не удалось загрузить репозитории. Повторим через полминуты.", "warn");
      setSyncNote("нет связи с сервером", "error");
      if (!state.repos.length) {
        clear(el.repoList);
        el.repoList.setAttribute("aria-busy", "false");
      }
    }
  }

  async function refreshAll() {
    if (state.inFlight) return state.inFlight;
    state.inFlight = Promise.allSettled([loadProfile(), loadRepos()]).finally(() => {
      state.inFlight = null;
    });
    return state.inFlight;
  }

  function schedule() {
    if (state.pollTimer) clearInterval(state.pollTimer);
    state.pollTimer = setInterval(() => {
      if (document.hidden) return; // don't poll in a background tab
      refreshAll();
    }, POLL_INTERVAL_MS);
  }

  /* --- Filter wiring ---------------------------------------------------- */
  const onSearch = debounce(() => {
    state.filters.q = el.search.value.trim();
    loadRepos();
  }, SEARCH_DEBOUNCE_MS);

  function bindFilters() {
    el.search.addEventListener("input", onSearch);
    el.controls.addEventListener("submit", (event) => {
      event.preventDefault();
      state.filters.q = el.search.value.trim();
      loadRepos();
    });

    el.language.addEventListener("change", () => {
      state.filters.language = el.language.value;
      loadRepos();
    });
    el.topic.addEventListener("change", () => {
      state.filters.topic = el.topic.value;
      loadRepos();
    });
    el.sort.addEventListener("change", () => {
      state.filters.sort = el.sort.value || "stars";
      loadRepos();
    });
    el.showForks.addEventListener("change", () => {
      state.filters.showForks = el.showForks.checked;
      loadRepos();
    });
    el.showArchived.addEventListener("change", () => {
      state.filters.showArchived = el.showArchived.checked;
      loadRepos();
    });

    // Delegated topic chips inside cards.
    el.repoList.addEventListener("click", (event) => {
      const chip = event.target.closest(".topic-chip");
      if (!chip || typeof chip.dataset.topic !== "string") return;
      event.preventDefault();
      state.filters.topic = chip.dataset.topic;
      if (![...el.topic.options].some((option) => option.value === state.filters.topic)) {
        const option = document.createElement("option");
        option.value = state.filters.topic;
        option.textContent = state.filters.topic;
        el.topic.append(option);
      }
      el.topic.value = state.filters.topic;
      loadRepos();
      el.topic.focus();
    });

    el.resetFilters.addEventListener("click", () => {
      el.search.value = "";
      el.language.value = "";
      el.topic.value = "";
      el.sort.value = "stars";
      state.filters = {
        q: "",
        language: "",
        topic: "",
        sort: "stars",
        showForks: state.defaults ? Boolean(state.defaults.show_forks) : false,
        showArchived: state.defaults ? Boolean(state.defaults.show_archived) : false,
      };
      el.showForks.checked = state.filters.showForks;
      el.showArchived.checked = state.filters.showArchived;
      loadRepos();
      el.search.focus();
    });

    document.querySelectorAll("[data-retry]").forEach((button) => {
      button.addEventListener("click", () => refreshAll());
    });

    document.addEventListener("visibilitychange", () => {
      if (!document.hidden) refreshAll();
    });
  }

  /* --- Boot ------------------------------------------------------------- */
  function init() {
    initTheme();
    bindFilters();
    renderSkeletons();
    refreshAll().finally(schedule);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();