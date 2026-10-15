// Your work (/welcome, a proposal): the home a signed-in person comes back
// to, tailored rather than a search box. The languages they work in (picked
// with one multi-select, kept by the hub), each with its resources and the
// way into its sources; the repos they or their organisations own; and the
// repos they starred on WACS. Search stays in the header.
import { A } from "@solidjs/router";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { HomeLanguage, LanguageSummary } from "../api/types";
import { Async, plural } from "../ui/bits";
import { me, signInHref } from "../ui/session";
import { RepoRow } from "./Home";

export default function Welcome() {
  createMemo(() => startPageView("welcome"));
  return (
    <Show
      when={me()?.me}
      fallback={
        <>
          <h1>Your work</h1>
          <p>
            <a href={signInHref()} rel="external">
              Sign in with WACS
            </a>{" "}
            to see the languages you work in and your projects.
          </p>
        </>
      }
    >
      <Signed />
    </Show>
  );
}

function Signed() {
  const [home, { refetch }] = createResource(() => api.json("/v1/me/home"));
  const [saving, setSaving] = createSignal(false);

  const save = async (languages: string[]) => {
    setSaving(true);
    try {
      await api.post("/v1/me/languages", {}, { languages });
      await refetch();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Async of={home} what="your work">
      {(h) => {
        const picked = () => h().languages.map((l) => l.ietf);
        return (
          <>
            <header class="page-head">
              <h1>Your work</h1>
            </header>

            <section class="welcome-section">
              <div class="section-head">
                <h2>Languages</h2>
                <span class="small muted">{picked().length ? `${plural(picked().length, "language")} picked` : "none picked yet"}</span>
              </div>
              <LanguagePicker picked={h().languages} busy={saving()} onChange={(ls) => void save(ls)} />
              <Show when={h().languages.length} fallback={<p class="small">Pick the languages you work in; they show here with their resources.</p>}>
                <ul class="welcome-languages">
                  <For each={h().languages}>{(l) => <LanguageRow l={l} />}</For>
                </ul>
              </Show>
            </section>

            <section class="welcome-section">
              <div class="section-head">
                <h2>Your projects</h2>
                <span class="small muted">
                  owned by {[h().username, ...h().orgs].join(", ")}
                </span>
              </div>
              <Show when={h().mine.length} fallback={<p class="small">No repos owned by you or your organisations.</p>}>
                <ul class="feed">
                  <For each={h().mine.slice(0, 30)}>{(r) => <RepoRow repo={r} />}</For>
                </ul>
                <Show when={h().mine.length > 30}>
                  <p class="small">and {h().mine.length - 30} more.</p>
                </Show>
              </Show>
            </section>

            <section class="welcome-section">
              <div class="section-head">
                <h2>Starred</h2>
                <span class="small muted">star a repo on WACS to keep it here</span>
              </div>
              <Show when={h().starred.length} fallback={<p class="small">Nothing starred yet.</p>}>
                <ul class="feed">
                  <For each={h().starred}>{(r) => <RepoRow repo={r} />}</For>
                </ul>
              </Show>
            </section>
          </>
        );
      }}
    </Async>
  );
}

const nameOf = (l: Pick<LanguageSummary, "ietf" | "english_name" | "name">) => l.english_name ?? l.name ?? l.ietf;

function LanguageRow(props: { l: HomeLanguage }) {
  const l = () => props.l;
  return (
    <li class="welcome-language">
      <A class="welcome-language-name" href={`/languages/${l().ietf}`}>
        {nameOf(l())}
      </A>
      <Show when={l().name && l().name !== l().english_name}>
        <span class="muted">{l().name}</span>
      </Show>
      <code class="muted">{l().ietf}</code>
      <span class="welcome-resources">
        <For each={l().resources} fallback={<span class="muted small">no repos</span>}>
          {(r) => (
            <A href={`/languages/${l().ietf}/sources?resource=${encodeURIComponent(r.resource)}`} title={`${r.repos} repos, ${r.writer} of them Writer projects`}>
              {r.resource} sources
              <span class="muted"> · {plural(r.repos, "repo")}</span>
              <Show when={r.official}><span class="muted"> · official</span></Show>
            </A>
          )}
        </For>
      </span>
    </li>
  );
}

/**
 * One multi-select: the picked languages as removable chips, and a search
 * that adds. Changes save at once (the server keeps the list).
 */
function LanguagePicker(props: { picked: readonly HomeLanguage[]; busy: boolean; onChange: (ietfs: string[]) => void }) {
  const [q, setQ] = createSignal("");
  const [found] = createResource(
    () => (q().trim().length >= 2 ? q().trim() : false),
    (q) => api.json("/v1/languages", { query: { q, limit: 8 } }),
  );
  const ids = () => props.picked.map((l) => l.ietf);
  const add = (ietf: string) => {
    setQ("");
    if (!ids().includes(ietf)) props.onChange([...ids(), ietf]);
  };
  return (
    <div class="picker">
      <div class="picker-box">
        <For each={props.picked}>
          {(l) => (
            <span class="picker-chip">
              {nameOf(l)}
              <button type="button" aria-label={`Remove ${nameOf(l)}`} disabled={props.busy} onClick={() => props.onChange(ids().filter((i) => i !== l.ietf))}>
                ×
              </button>
            </span>
          )}
        </For>
        <input
          type="search"
          placeholder={props.picked.length ? "Add a language…" : "Search languages: name or code"}
          aria-label="Add a language"
          value={q()}
          onInput={(e) => setQ(e.currentTarget.value)}
          onKeyDown={(e) => {
            const first = found()?.find((l) => !ids().includes(l.ietf));
            if (e.key === "Enter" && first) {
              e.preventDefault();
              add(first.ietf);
            }
          }}
        />
      </div>
      <Show when={q().trim().length >= 2 && found()}>
        {(list) => (
          <ul class="picker-results" role="listbox">
            <For each={list().filter((l) => !ids().includes(l.ietf))} fallback={<li class="small muted">No match.</li>}>
              {(l) => (
                <li role="option">
                  <button type="button" onClick={() => add(l.ietf)}>
                    {nameOf(l)} <code class="muted">{l.ietf}</code> <span class="muted small">{plural(l.repos, "repo")}</span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        )}
      </Show>
    </div>
  );
}
