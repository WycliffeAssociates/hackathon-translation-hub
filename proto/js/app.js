// Hash router. Each route gets a fresh <div>, so an async page that finishes
// after you navigated away writes into a detached node and does no harm.
import { html, render } from "./ui.js";
import { home } from "./pages/home.js";
import { searchPage } from "./pages/search.js";
import { languagePage } from "./pages/language.js";
import { versionPage } from "./pages/version.js";
import { repoPage } from "./pages/repo.js";
import { checkPage } from "./pages/check.js";
import { sitemapPage } from "./pages/sitemap.js";
import { stackPage } from "./pages/stack.js";
import { schemaPage } from "./pages/schema.js";
import { conflictsPage } from "./pages/conflicts.js";

const ROUTES = [
  [/^\/$/, home, "Home"],
  [/^\/search$/, searchPage, "Search"],
  [/^\/l\/(?<ietf>[^/]+)$/, languagePage, "Language"],
  [/^\/l\/(?<ietf>[^/]+)\/(?<rtype>[^/]+)\/(?<track>[^/]+)$/, versionPage, "Version"],
  [/^\/r\/(?<owner>[^/]+)\/(?<repo>[^/]+)$/, repoPage, "Repo"],
  [/^\/check$/, checkPage, "Check"],
  [/^\/sitemap$/, sitemapPage, "Sitemap"],
  [/^\/stack$/, stackPage, "Stack"],
  [/^\/schema$/, schemaPage, "Schema"],
  [/^\/conflicts$/, conflictsPage, "Conflicts"],
];

const main = document.querySelector("main");

function route() {
  const [path, qs] = (location.hash.slice(1) || "/").split("?");
  const query = new URLSearchParams(qs);
  const el = document.createElement("div");
  main.replaceChildren(el);
  for (const [re, page, title] of ROUTES) {
    const m = path.match(re);
    if (!m) continue;
    const params = Object.fromEntries(Object.entries(m.groups ?? {}).map(([k, v]) => [k, decodeURIComponent(v)]));
    document.title = `${title} · Translation Hub`;
    for (const a of document.querySelectorAll("header nav a")) a.classList.toggle("on", path.startsWith(a.getAttribute("href").slice(1)) && a.getAttribute("href") !== "#/");
    const view = page(el, params, query);
    if (view && !(view instanceof Promise)) render(el, view);
    scrollTo(0, 0);
    return;
  }
  render(el, html`<h1>Not found</h1><p><a href="#/sitemap">Sitemap</a></p>`);
}

// In-page jumps (the hash is the route, so anchors cannot use it).
document.addEventListener("click", (e) => {
  const a = e.target.closest("[data-jump]");
  if (!a) return;
  e.preventDefault();
  document.getElementById(a.dataset.jump)?.scrollIntoView({ behavior: "smooth" });
});

// The home page's search box submits to the search route.
document.addEventListener("submit", (e) => {
  if (!e.target.matches("[data-search]")) return;
  e.preventDefault();
  location.hash = `#/search?q=${encodeURIComponent(new FormData(e.target).get("q"))}`;
});

addEventListener("hashchange", route);
route();
