// The Releases tab of an official repo (a proposal): what is published, what
// changed since, and, for a publisher, a new release with its bulletin (the
// release notes readers see). Changes are counted the way the sources page
// counts them, in verses by book, so "since v1.2: Matthew 40 verses changed"
// is what the bulletin is written from. A bulletin can carry translations;
// they are committed beside the text as bulletins/{tag}/{language}.md.
import { For, Index, Show, createResource, createSignal } from "solid-js";
import { api } from "../../api/client";
import type { Release } from "../../api/types";
import { Async, ago, dateOf, plural } from "../../ui/bits";
import { me } from "../../ui/session";
import VerseDiffView, { releaseDiff } from "../VerseDiffView";
import { agree, agreed, floresOf, translate } from "../../translate/nllb";

/**
 * Releases are named by date (Will, 2026-10-15): today as `2026.10.15`, and
 * `2026.10.15.2` for a second release the same day. Nobody has to decide what
 * kind of change it was, as a semantic version would ask.
 */
export const nextTag = (tags: readonly string[]) => {
  const d = new Date();
  const today = `${d.getUTCFullYear()}.${String(d.getUTCMonth() + 1).padStart(2, "0")}.${String(d.getUTCDate()).padStart(2, "0")}`;
  if (!tags.includes(today)) return today;
  let n = 2;
  while (tags.includes(`${today}.${n}`)) n++;
  return `${today}.${n}`;
};

/**
 * NLLB translation is hidden for now (Will, 2026-10-15): too slow on WASM
 * (about a minute a bulletin) to show in a demo. `?nllb=1` on the page shows it.
 * Next: WebGPU when the browser has it, WASM otherwise.
 */
const nllbOn = new URLSearchParams(location.search).get("nllb") === "1";

/** `41-MAT.usfm` → `MAT`. */
const bookOf = (path: string) => /(?:^|\/)\d*-?([1-3A-Z]{3})\.usfm$/i.exec(path)?.[1]?.toUpperCase() ?? path;

export default function Releases(props: { owner: string; name: string; releases: readonly Release[]; onPublished: () => void }) {
  const id = () => ({ path: { owner: props.owner, name: props.name } });
  const [changes, { refetch }] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}/unreleased", c));
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<string>();
  const [failed, setFailed] = createSignal<string>();
  /** The changed book whose verse diff is open. */
  const [openDiff, setOpenDiff] = createSignal<string>();
  const [translations, setTranslations] = createSignal<{ language: string; text: string }[]>([]);
  const publisher = () => me()?.me?.publisher === true;
  /** What NLLB is doing (the first time, downloading), or nothing. */
  const [working, setWorking] = createSignal<string>();

  /** Fill translation row `i` with the bulletin translated by NLLB-200, in this browser. */
  const nllb = async (button: HTMLElement, i: number) => {
    const form = button.closest("form") as HTMLFormElement;
    const f = new FormData(form);
    const text = String(f.get("notes") ?? "").trim();
    const from = String(f.get("notes_language") ?? "en");
    const to = translations()[i]?.language ?? "";
    if (!text) return setFailed("Write the bulletin first; NLLB translates it.");
    if (!floresOf(to)) return setFailed(`NLLB-200 has no "${to || "(no language)"}". Type a language tag it knows, such as fr, sw or bem.`);
    if (!floresOf(from)) return setFailed(`NLLB-200 has no "${from}", the bulletin's language.`);
    if (
      !agreed() &&
      !confirm(
        "Translate with NLLB-200 in this browser? The model (several hundred MB) downloads once from Hugging Face and stays cached here. Its weights are licensed CC-BY-NC 4.0 (non-commercial). Machine translation: read it before you publish.",
      )
    )
      return;
    agree();
    setFailed(undefined);
    setWorking("Starting NLLB…");
    try {
      const out = await translate(text, from, to, (p) =>
        setWorking(p === "ready" ? `Translating into ${to}…` : `Downloading NLLB: ${Math.round((100 * p.loaded) / p.total)}% of ${(p.total / 1e6).toFixed(0)} MB (${p.file ?? ""})`),
      );
      setTranslations(translations().map((x, j) => (j === i ? { ...x, text: out } : x)));
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String(e));
    } finally {
      setWorking(undefined);
    }
  };

  const publish = async (form: HTMLFormElement) => {
    const f = new FormData(form);
    const tag = String(f.get("tag") ?? "").trim();
    const notes = String(f.get("notes") ?? "");
    const bulletins = translations().filter((t) => t.language.trim() && t.text.trim());
    if (!confirm(`Publish ${props.owner}/${props.name} ${tag} at its current head${bulletins.length ? `, with the bulletin in ${bulletins.length + 1} languages` : ""}?`)) return;
    setBusy(true);
    setFailed(undefined);
    try {
      const r = await api.post("/v1/repos/{owner}/{name}/releases", id(), {
        tag,
        notes,
        notes_language: String(f.get("notes_language") ?? "en"),
        bulletins,
      });
      setDone(r.message);
      setTranslations([]);
      form.reset();
      props.onPublished();
      await refetch();
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String((e as { message?: string }).message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="releases-tab">
      <Async of={changes} what="changes since the last release">
        {(u) => (
          <section>
            <h2>
              {u().last_tag ? <>Since {u().last_tag}</> : <>Not released yet</>}
              <Show when={u().last_unix}><span class="count"> {ago(u().last_unix)}</span></Show>
            </h2>
            <Show
              when={u().books.length || u().commits.length}
              fallback={<p class="small">Nothing has changed since {u().last_tag}.</p>}
            >
              <p class="small">
                {plural(u().commits.length, "commit")}
                {u().commits.length >= 500 ? " or more" : ""}; {plural(u().books.length, "book")} changed in their verses
                <Show when={u().other.length}>; also {u().other.join(", ")}</Show>.
              </p>
              <Show when={u().books.length}>
                <table class="release-books">
                  <thead>
                    <tr>
                      <th scope="col">Book</th>
                      <th scope="col" class="num">Chapters</th>
                      <th scope="col" class="num">Verses</th>
                      <th scope="col">Since {u().last_tag ?? "nothing"}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={u().books}>
                      {(b) => (
                        <tr>
                          <th scope="row" class="mono">{bookOf(b.path)}</th>
                          <td class="num">
                            <Show when={b.before && b.after && b.before.chapters !== b.after.chapters} fallback={b.after?.chapters ?? "—"}>
                              {b.before!.chapters} → {b.after!.chapters}
                            </Show>
                          </td>
                          <td class="num">
                            <Show when={b.before && b.after && b.before.verses !== b.after.verses} fallback={b.after?.verses.toLocaleString() ?? "—"}>
                              {b.before!.verses.toLocaleString()} → {b.after!.verses.toLocaleString()}
                            </Show>
                          </td>
                          <td>
                            <button type="button" class="linkish small release-diff-toggle" onClick={() => setOpenDiff(openDiff() === b.path ? undefined : b.path)}>
                              {openDiff() === b.path ? "Hide" : "See"}
                            </button>{" "}
                            {!b.before ? "new book" : !b.after ? "removed" : [
                              b.diff.changed && `${b.diff.changed.toLocaleString()} changed`,
                              b.diff.added && `${b.diff.added.toLocaleString()} added`,
                              b.diff.missing && `${b.diff.missing.toLocaleString()} removed`,
                            ].filter(Boolean).join(" · ")}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
                <Show when={openDiff()}>
                  {(path) => (
                    <div class="release-diff">
                      <h4 class="sources-sub">
                        {bookOf(path())}: {u().last_tag ?? "nothing released"} → now
                      </h4>
                      <VerseDiffView load={releaseDiff(props.owner, props.name, path())} />
                    </div>
                  )}
                </Show>
              </Show>
            </Show>

            <Show when={publisher()}>
              <form
                class="release-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void publish(e.currentTarget);
                }}
              >
                <h3>New release</h3>
                <label>
                  Version <input name="tag" value={nextTag(props.releases.map((r) => r.tag_name))} size={8} />
                </label>
                <label class="release-notes">
                  Bulletin <span class="muted small">what changed, for readers</span>
                  <textarea name="notes" rows={5} placeholder={u().books.length ? `${u().books.map((b) => bookOf(b.path)).slice(0, 6).join(", ")}…` : ""} />
                </label>
                <label>
                  Written in <input name="notes_language" value="en" size={6} aria-label="Language of the bulletin" />
                </label>
                <Index each={translations()}>
                  {(t, i) => (
                    <fieldset class="release-translation">
                      <input
                        aria-label="Language"
                        placeholder="fr"
                        size={6}
                        value={t().language}
                        onInput={(e) => setTranslations(translations().map((x, j) => (j === i ? { ...x, language: e.currentTarget.value } : x)))}
                      />
                      <textarea
                        aria-label="Translated bulletin"
                        rows={3}
                        value={t().text}
                        onInput={(e) => setTranslations(translations().map((x, j) => (j === i ? { ...x, text: e.currentTarget.value } : x)))}
                      />
                      <span class="release-translation-actions">
                        <Show when={nllbOn}>
                        <button
                          type="button"
                          class="linkish small"
                          disabled={!!working()}
                          title={floresOf(t().language) ? "Translate the bulletin in this browser with NLLB-200" : "NLLB-200 has no such language"}
                          onClick={(e) => void nllb(e.currentTarget, i)}
                        >
                          Translate with NLLB
                        </button>
                        </Show>
                        <button type="button" class="linkish small" onClick={() => setTranslations(translations().filter((_, j) => j !== i))}>
                          Remove
                        </button>
                      </span>
                    </fieldset>
                  )}
                </Index>
                <Show when={working()}>{(m) => <p class="loading small" role="status">{m()}</p>}</Show>
                <p class="release-buttons">
                  <button type="button" class="linkish small" onClick={() => setTranslations([...translations(), { language: "", text: "" }])}>
                    Add the bulletin in another language
                  </button>
                  <button type="submit" class="primary" disabled={busy()}>
                    Publish
                  </button>
                </p>
              </form>
            </Show>
          </section>
        )}
      </Async>
      <Show when={done()}>{(m) => <p class="ok" role="status">{m()}</p>}</Show>
      <Show when={failed()}>{(m) => <p class="failure" role="alert">{m()}</p>}</Show>

      <section>
        <h2>Releases</h2>
        <Show when={props.releases.length} fallback={<p class="small">None yet.</p>}>
          <ol class="release-list">
            <For each={props.releases}>
              {(r) => (
                <li>
                  <a href={r.html_url} target="_blank" rel="noopener">
                    <b>{r.tag_name}</b>
                  </a>{" "}
                  <span class="muted" title={dateOf(Date.parse(r.published_at ?? "") / 1000)}>{r.published_at ? ago(Date.parse(r.published_at) / 1000) : ""}</span>
                  <Show when={r.assets?.length}>
                    <span class="release-assets small">
                      <For each={r.assets ?? []}>
                        {(a) => (
                          <a href={a.browser_download_url} rel="noopener">
                            {a.name}
                          </a>
                        )}
                      </For>
                    </span>
                  </Show>
                  <Show when={r.body}>{(body) => <p class="release-body">{body()}</p>}</Show>
                </li>
              )}
            </For>
          </ol>
        </Show>
      </section>
    </div>
  );
}
