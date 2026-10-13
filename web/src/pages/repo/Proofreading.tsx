// Sous Chef's findings, grouped by code. The server keeps at most a few
// hundred findings per code; the rest are counted in `dropped`, and the page
// says so instead of pretending the list is complete.
import { For, Show, createSignal } from "solid-js";
import type { Finding, Proof, ProofSummary } from "../../api/types";
import { ago } from "../../ui/bits";

export default function Proofreading(props: { proof: Proof; summary: ProofSummary | null | undefined; onOpen: (f: Finding) => void }) {
  const groups = () => {
    const by = new Map<string, Finding[]>();
    for (const f of props.proof.findings) by.set(f.code, [...(by.get(f.code) ?? []), f]);
    return [...by.entries()]
      .map(([code, list]) => ({ code, list, dropped: props.proof.dropped[code] ?? 0, rule: list[0]?.rule ?? "" }))
      .sort((a, b) => b.list.length + b.dropped - (a.list.length + a.dropped));
  };
  return (
    <>
      <p class="small">
        <Show when={props.summary}>
          {(s) => (
            <>
              {s().findings.toLocaleString()} findings over {props.proof.books.length} books · ran {ago(s().ran_unix)} with scripture-kitchen {s().sk_tag}
              <Show when={!s().current}> · <b>HEAD moved since; a new run is queued</b></Show>
              <Show when={s().error}> · <span class="bad">{s().error}</span></Show>
            </>
          )}
        </Show>
      </p>
      <Show when={props.proof.skipped.length}>
        <p class="small">
          Skipped: <For each={props.proof.skipped}>{(s) => <span class="mono">{s.id} ({s.reason}) </span>}</For>
        </p>
      </Show>
      <Show when={groups().length} fallback={<p class="clean-note">No findings.</p>}>
        <For each={groups()}>{(g) => <Group {...g} onOpen={props.onOpen} />}</For>
      </Show>
    </>
  );
}

function Group(props: { code: string; rule: string; list: Finding[]; dropped: number; onOpen: (f: Finding) => void }) {
  // Rows render on first open: a code can hold hundreds.
  const [opened, setOpened] = createSignal(false);
  return (
    <details class="proof-group" onToggle={(e) => e.currentTarget.open && setOpened(true)}>
      <summary>
        <b>{props.code}</b> <span class="count">{(props.list.length + props.dropped).toLocaleString()}</span>{" "}
        <span class="tag">{props.rule}</span>
        <Show when={props.dropped}>
          <span class="small"> · showing {props.list.length.toLocaleString()}, {props.dropped.toLocaleString()} more not kept by the server</span>
        </Show>
      </summary>
      <Show when={opened()}>
        <table class="findings">
          <tbody>
            <For each={props.list}>
              {(f) => (
                <tr>
                  <td class="mono nowrap">
                    <button type="button" class="linkish" onClick={() => props.onOpen(f)} title={`Open ${f.id} at this place`}>
                      {f.book} {f.chapter}:{f.verse ?? "–"}
                    </button>
                  </td>
                  <td>{f.message}</td>
                  <td class="excerpt mono">{f.text}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </details>
  );
}
