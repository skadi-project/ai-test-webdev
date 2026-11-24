/* gh-portfolio frontend: no frameworks, no build step.
   Fetches /api/* periodically and swaps content without a page reload. */

(function () {
  "use strict";

  var REFRESH_MS = 30000; // client-side polling interval

  // GitHub-like language colors (subset) so chips feel familiar.
  var LANG_COLORS = {
    Assembly: "#6e4c13", Astro: "#ff5a03", C: "#555555", "C#": "#178600",
    "C++": "#f34b7d", Clojure: "#db5855", Crystal: "#000100", CSS: "#563d7c",
    Dart: "#00b4ab", Dockerfile: "#384d54", Elixir: "#6e4a7e", Elm: "#60b5cc",
    EmacsLisp: "#c06588", Erlang: "#b83998", Go: "#00add8", Groovy: "#4298b8",
    Haskell: "#5e5086", HTML: "#e34c26", Java: "#b07219", JavaScript: "#f1e05a",
    Julia: "#a270ba", Jupyter: "#da5b0b", Kotlin: "#a97bff", Lua: "#000080",
    MDX: "#fcb32c", Matlab: "#e16737", Nim: "#ffc200", Nix: "#7e7eff",
    OCaml: "#3be133", Perl: "#0298c3", PHP: "#4f5d95", PowerShell: "#012456",
    Python: "#3572a5", R: "#198ce7", Ruby: "#701516", Rust: "#dea584",
    Sass: "#a53b70", Scala: "#c22d40", Shell: "#89e051", Solidity: "#aa6746",
    SQL: "#e38c00", Swift: "#f05138", TypeScript: "#3178c6", VimScript: "#199f4b",
    Vue: "#41b883", Zig: "#ec915c"
  };

  var state = {
    q: "",
    language: "",
    topic: "",
    sort: "updated",
    order: "desc"
  };

  var els = {};
  ["profile", "stats", "lang-chart", "lang-empty", "repo-list", "repo-count",
   "search", "lang-filter", "topic-filter", "sort", "error-banner",
   "error-text", "retry-btn", "empty-state", "pending-hint", "updated-at",
   "theme-toggle"].forEach(function (id) {
    els[id] = document.getElementById(id);
  });

  /* ------------------------------ helpers ------------------------------ */

  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function fmtCount(n) {
    if (n >= 10000) return Math.round(n / 1000) + "k";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return String(n);
  }

  function fmtBytes(bytes) {
    if (bytes > 1048576) return (bytes / 1048576).toFixed(1) + " MB";
    if (bytes > 1024) return Math.round(bytes / 1024) + " KB";
    return bytes + " B";
  }

  function timeAgo(iso) {
    var then = Date.parse(iso);
    if (isNaN(then)) return "";
    var seconds = Math.max(1, Math.floor((Date.now() - then) / 1000));
    var units = [
      [60, "minute"], [24, "hour"], [30, "day"], [12, "month"], [Infinity, "year"]
    ];
    var value = seconds, unit = "second";
    for (var i = 0; i < units.length; i++) {
      if (value < units[i][0]) break;
      value = Math.floor(value / units[i][0]);
      unit = units[i][1];
    }
    var plural = value === 1 ? "" : "s";
    return "updated " + value + " " + unit + plural + " ago";
  }

  function langColor(name) {
    if (LANG_COLORS[name]) return LANG_COLORS[name];
    // Stable pseudo-random hue for unknown languages.
    var hash = 0;
    for (var i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) % 360;
    return "hsl(" + hash + ", 60%, 55%)";
  }

  function fail(message) {
    els["error-text"].textContent = message;
    els["error-banner"].classList.remove("hidden");
  }

  function clearError() {
    els["error-banner"].classList.add("hidden");
  }

  /* ------------------------------ profile ------------------------------ */

  function renderProfile(payload) {
    var p = payload.profile;
    if (!p) return;
    var card = els.profile;
    card.setAttribute("aria-busy", "false");
    card.innerHTML = "";

    var img = document.createElement("img");
    img.src = p.avatar_url;
    img.alt = "Avatar of " + (p.name || p.login);
    img.width = 112;
    img.height = 112;
    var avatar = el("div", "avatar");
    avatar.appendChild(img);

    var main = el("div", "profile-main");

    var h1 = el("h1", "profile-name", p.name || p.login);
    var login = el("p", "profile-login", "@" + p.login);
    main.appendChild(h1);
    main.appendChild(login);
    if (p.bio) main.appendChild(el("p", "profile-bio", p.bio));

    var meta = el("ul", "profile-meta");
    function metaItem(icon, text, href) {
      var li = el("li");
      li.append(icon + " ");
      if (href) {
        var a = el("a", null, text);
        a.href = href;
        a.rel = "noopener noreferrer";
        a.target = "_blank";
        li.appendChild(a);
      } else {
        li.appendChild(document.createTextNode(text));
      }
      return li;
    }
    if (p.location) meta.appendChild(metaItem("⌖", p.location));
    if (p.company) meta.appendChild(metaItem("⌗", p.company));
    if (p.blog) {
      var url = p.blog.indexOf("http") === 0 ? p.blog : "https://" + p.blog;
      meta.appendChild(metaItem("◇", p.blog.replace(/^https?:\/\//, ""), url));
    }
    if (p.twitter_username) {
      meta.appendChild(metaItem("✦", "@" + p.twitter_username,
        "https://twitter.com/" + p.twitter_username));
    }
    main.appendChild(meta);

    var stats = el("div", "profile-stats");
    function stat(value, label) {
      var box = el("div", "stat");
      box.appendChild(el("div", "stat-value", fmtCount(value)));
      box.appendChild(el("div", "stat-label", label));
      return box;
    }
    stats.appendChild(stat(p.followers, "followers"));
    stats.appendChild(stat(p.public_repos, "repos"));

    card.appendChild(avatar);
    card.appendChild(main);
    card.appendChild(stats);

    if (payload.updated_at) {
      var note = el("p", "empty-hint", "profile refreshed " + timeAgo(payload.updated_at));
      note.classList.add("profile-link");
      card.appendChild(note);
    }
  }

  /* ------------------------------ languages ------------------------------ */

  function renderLangs(payload) {
    var chart = els["lang-chart"];
    var entries = Object.entries(payload.languages || {})
      .sort(function (a, b) { return b[1] - a[1]; })
      .slice(0, 12);
    chart.innerHTML = "";
    if (!entries.length) {
      els["lang-empty"].classList.remove("hidden");
      return;
    }
    els["lang-empty"].classList.add("hidden");

    var total = entries.reduce(function (sum, e) { return sum + e[1]; }, 0);
    entries.forEach(function (entry) {
      var name = entry[0];
      var pct = total ? (entry[1] / total) * 100 : 0;

      var row = el("div", "lang-row");
      var label = el("span", "lang-name");
      var dot = el("span", "lang-dot");
      dot.style.background = langColor(name);
      dot.setAttribute("aria-hidden", "true");
      label.appendChild(dot);
      label.appendChild(document.createTextNode(name));

      var track = el("div", "lang-bar-track");
      var bar = el("div", "lang-bar");
      bar.style.width = Math.max(pct, 0.8) + "%";
      bar.style.background = langColor(name);
      track.appendChild(bar);
      track.title = fmtBytes(entry[1]);

      var pctLabel = el("span", "lang-pct", pct < 1 && pct > 0 ? "<1%" : pct.toFixed(0) + "%");

      row.appendChild(label);
      row.appendChild(track);
      row.appendChild(pctLabel);
      chart.appendChild(row);
    });
  }

  /* ------------------------------ repos ------------------------------ */

  function renderRepos(payload) {
    els["pending-hint"].classList.add("hidden");
    if (payload.pending) return;

    syncSelect(els["lang-filter"], payload.languages || [], "All languages");
    syncSelect(els["topic-filter"], payload.topics || [], "All topics");

    var list = els["repo-list"];
    list.innerHTML = "";

    if (!payload.repos.length) {
      els["empty-state"].classList.remove("hidden");
    } else {
      els["empty-state"].classList.add("hidden");
    }

    payload.repos.forEach(function (repo) {
      list.appendChild(repoCard(repo));
    });

    els["repo-count"].textContent =
      payload.count + " / " + payload.total + " repos";
    els["updated-at"].textContent = payload.updated_at
      ? "synced " + timeAgo(payload.updated_at)
      : "";
  }

  function repoCard(repo) {
    var card = el("li", "repo-card");

    var head = el("div", "repo-head");
    var link = el("a", "repo-name", repo.name);
    link.href = repo.html_url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    head.appendChild(link);

    var badges = el("div", "repo-badges");
    if (repo.pinned) badges.appendChild(el("span", "badge", "pinned"));
    if (repo.fork) badges.appendChild(el("span", "badge badge-fork", "fork"));
    if (repo.archived) badges.appendChild(el("span", "badge badge-archived", "archived"));
    if (badges.childNodes.length) head.appendChild(badges);
    card.appendChild(head);

    if (repo.description) {
      card.appendChild(el("p", "repo-desc", repo.description));
    }

    if (repo.topics && repo.topics.length) {
      var topics = el("div", "repo-topics");
      repo.topics.slice(0, 8).forEach(function (topic) {
        var chip = el("button", "topic-chip", topic);
        chip.type = "button";
        chip.title = "Filter by topic " + topic;
        chip.addEventListener("click", function () {
          state.topic = topic;
          els["topic-filter"].value = topic;
          reloadRepos();
        });
        topics.appendChild(chip);
      });
      card.appendChild(topics);
    }

    var foot = el("div", "repo-foot");
    if (repo.language) {
      var lang = el("span", "repo-lang");
      var dot = el("span", "lang-dot");
      dot.style.background = langColor(repo.language);
      dot.setAttribute("aria-hidden", "true");
      lang.appendChild(dot);
      lang.appendChild(document.createTextNode(repo.language));
      foot.appendChild(lang);
      foot.appendChild(el("span", "sep", "·"));
    }
    if (repo.stargazers_count) {
      foot.appendChild(el("span", "repo-metric", "★ " + fmtCount(repo.stargazers_count)));
    }
    if (repo.forks_count) {
      foot.appendChild(el("span", "repo-metric", "⑂ " + fmtCount(repo.forks_count)));
    }
    if (repo.updated_at) {
      foot.appendChild(el("span", "repo-metric", timeAgo(repo.updated_at)));
    }
    if (repo.homepage) {
      var home = repo.homepage.indexOf("http") === 0 ? repo.homepage : "https://" + repo.homepage;
      var siteLink = el("a", "repo-homepage", "website ↗");
      siteLink.href = home;
      siteLink.target = "_blank";
      siteLink.rel = "noopener noreferrer";
      foot.appendChild(siteLink);
    }
    card.appendChild(foot);
    return card;
  }

  function syncSelect(select, values, placeholder) {
    var current = select.value;
    select.innerHTML = "";
    select.appendChild(new Option(placeholder, ""));
    values.forEach(function (value) {
      select.appendChild(new Option(value, value));
    });
    if (values.indexOf(current) !== -1) select.value = current;
  }

  /* ------------------------------ data loading ------------------------------ */

  function reposUrl() {
    var params = new URLSearchParams();
    if (state.q) params.set("q", state.q);
    if (state.language) params.set("language", state.language);
    if (state.topic) params.set("topic", state.topic);
    if (state.sort) params.set("sort", state.sort);
    params.set("order", state.order);
    return "/api/repos?" + params.toString();
  }

  function getJson(url) {
    return fetch(url, { headers: { Accept: "application/json" } }).then(function (response) {
      if (!response.ok) {
        return response
          .json()
          .catch(function () { return { error: response.statusText }; })
          .then(function (body) {
            throw new Error(body.error || ("HTTP " + response.status));
          });
      }
      return response.json();
    });
  }

  function loadAll() {
    getJson("/api/profile").then(renderProfile).catch(function () { /* keep skeleton */ });
    getJson("/api/stats/languages").then(renderLangs).catch(function () { /* keep skeleton */ });
    reloadRepos();
  }

  function reloadRepos() {
    clearError();
    return getJson(reposUrl())
      .then(renderRepos)
      .catch(function (error) {
        fail("Could not load repositories: " + error.message +
             ". Showing the last rendered state.");
      });
  }

  /* ------------------------------ events ------------------------------ */

  var searchTimer = null;
  els.search.addEventListener("input", function () {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(function () {
      state.q = els.search.value.trim();
      reloadRepos();
    }, 250);
  });

  els["lang-filter"].addEventListener("change", function () {
    state.language = this.value;
    reloadRepos();
  });

  els["topic-filter"].addEventListener("change", function () {
    state.topic = this.value;
    reloadRepos();
  });

  els.sort.addEventListener("change", function () {
    var parts = this.value.split(":");
    state.sort = parts[0];
    state.order = parts[1] || "desc";
    reloadRepos();
  });

  els["retry-btn"].addEventListener("click", loadAll);

  var toggle = els["theme-toggle"];
  function syncToggle() {
    toggle.setAttribute(
      "aria-pressed",
      String(document.documentElement.dataset.theme === "dark")
    );
  }
  toggle.addEventListener("click", function () {
    var next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try { localStorage.setItem("theme", next); } catch (e) { /* ignore */ }
    syncToggle();
  });
  syncToggle();

  // Poll for fresh data without a page reload.
  setInterval(loadAll, REFRESH_MS);
  // Re-render relative timestamps even without new data.
  setInterval(reloadRepos, 60000);

  loadAll();
})();
