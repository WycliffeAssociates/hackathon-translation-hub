// The assembling step before consolidating a language's Writer projects into
// a new official repo (a proposal): every Writer candidate per book, the
// default picked (Primary and shown on BIEL first), with what helps choose:
// chapters and verses, coverage of the source, last updated, saves, and the
// book's progress over time (verses present, scaled to the source's count).
import { For, Show, createResource, createSignal } from "solid-js";
import { A } from "@solidjs/router";
import { api } from "../../api/client";
import type { Done } from "../../api/types";
import { Async, Sparkline, ago, dateOf, plural } from "../../ui/bits";

export default function Assemble(props: {
  owner: string;
  name: string;
  busy: boolean;
  onCreate: (what: string, run: () => Promise<Done>) => void;
}) {
  const id = () => ({ path: { owner: props.owner, name: props.name } });
  const [plan] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}/assemble", c));
  /** Book → `owner/name`, or "" to leave it out; unset books take the default. */
  const [picks, setPicks] = createSignal<Record<string, string>>({});
  const chosen = (book: string, fallback: string) => picks()[book] ?? fallback;

  return (
    <Async of={plan} what="Writer candidates">
      {(p) => {
        const defaults = () =>
          Object.fromEntries(
            p().books.map((b) => {
              const c = b.candidates.find((x) => x.default) ?? b.candidates[0];
              return [b.book, c ? `${c.owner}/${c.name}` : ""];
            }),
          );
        const taken = () => p().books.filter((b) => chosen(b.book, defaults()[b.book] ?? "") !== "").length;
        return (
          <div class="assemble">
            <p class="small">
              One Writer project per book goes into <b class="mono">{p().target}</b>, every save kept as a commit. The default is the
              one BIEL shows as Primary; pick another, or leave a book out.
            </p>
            <table>
              <thead>
                <tr>
                  <th scope="col">Book</th>
                  <th scope="col">Writer project</th>
                  <th scope="col">Catalog</th>
                  <th scope="col" class="num">ch · vv</th>
                  <th scope="col" class="num">Coverage</th>
                  <th scope="col">Updated</th>
                  <th scope="col" class="num">Saves</th>
                  <th scope="col">Progress</th>
                </tr>
              </thead>
              <For each={p().books}>
                {(b) => (
                  <tbody class="assemble-book">
                    <For each={b.candidates}>
                      {(c, i) => {
                        const key = `${c.owner}/${c.name}`;
                        const on = () => chosen(b.book, defaults()[b.book] ?? "") === key;
                        const pct = () => (c.source_verses ? Math.round(((c.present ?? 0) / c.source_verses) * 100) : null);
                        return (
                          <tr classList={{ on: on() }}>
                            <Show when={i() === 0}>
                              <th scope="rowgroup" rowSpan={b.candidates.length + 1} class="mono">
                                {b.book}
                              </th>
                            </Show>
                            <td>
                              <label class="assemble-pick">
                                <input
                                  type="radio"
                                  name={`pick-${b.book}`}
                                  checked={on()}
                                  onChange={() => setPicks({ ...picks(), [b.book]: key })}
                                />
                                <span>
                                  <A href={`/repos/${c.owner}/${c.name}`} class="mono">
                                    {key}
                                  </A>
                                  <Show when={c.default}> <span class="tag small">default</span></Show>
                                </span>
                              </label>
                            </td>
                            <td>
                              <Show when={c.status} fallback={<span class="small muted">not listed</span>}>
                                <span class="tag small" classList={{ "official-tag": c.status === "Primary" }}>
                                  {c.status}
                                </span>
                              </Show>
                              <Show when={c.show_on_biel}> <span class="tag biel small">BIEL</span></Show>
                            </td>
                            <td class="num mono">
                              {c.chapters} · {c.verses.toLocaleString()}
                            </td>
                            <td class="num" title={c.source_verses ? `${c.present} of the source's ${c.source_verses} verses` : "no source"}>
                              {pct() === null ? "—" : `${pct()}%`}
                            </td>
                            <td title={dateOf(c.head_unix)}>{ago(c.head_unix)}</td>
                            <td class="num">{c.saves.toLocaleString()}</td>
                            <td>
                              <Sparkline
                                values={c.progress}
                                max={c.source_verses ?? undefined}
                                title={`Verses present from ${dateOf(c.first_unix)} to ${dateOf(c.head_unix)}: ${c.progress.at(-1) ?? 0}${c.source_verses ? ` of ${c.source_verses}` : ""}`}
                              />
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                    <tr class="assemble-skip" classList={{ on: chosen(b.book, defaults()[b.book] ?? "") === "" }}>
                      <td colSpan={7}>
                        <label class="assemble-pick small">
                          <input
                            type="radio"
                            name={`pick-${b.book}`}
                            checked={chosen(b.book, defaults()[b.book] ?? "") === ""}
                            onChange={() => setPicks({ ...picks(), [b.book]: "" })}
                          />
                          Leave {b.book} out
                        </label>
                      </td>
                    </tr>
                  </tbody>
                )}
              </For>
            </table>
            <p class="official-actions">
              <button
                type="button"
                class="primary"
                disabled={props.busy || p().exists || taken() === 0}
                onClick={() => {
                  const all = Object.fromEntries(p().books.map((b) => [b.book, chosen(b.book, defaults()[b.book] ?? "")]));
                  props.onCreate(`Create ${p().target} from ${plural(taken(), "Writer project")}, every save kept as a commit?`, () =>
                    api.post("/v1/repos/{owner}/{name}/consolidate", id(), { picks: all }),
                  );
                }}
              >
                Create {p().target} from {plural(taken(), "book")}, with history
              </button>
              <Show when={p().exists}>
                <span class="small">{p().target} exists: pull books into it instead.</span>
              </Show>
            </p>
          </div>
        );
      }}
    </Async>
  );
}
