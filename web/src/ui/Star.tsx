// Star a repo: Forgejo's own pin, done as the signed-in person, so it shows on
// WACS too and puts the repo on the hub's Your work page.
import { Show, createResource, createSignal } from "solid-js";
import { api } from "../api/client";
import { me } from "./session";

export function StarButton(props: { owner: string; name: string }) {
  const call = () => ({ path: { owner: props.owner, name: props.name } });
  const [state, { mutate }] = createResource(
    () => (me()?.me ? call() : false),
    (c) => api.json("/v1/repos/{owner}/{name}/star", c).catch(() => null),
  );
  const [busy, setBusy] = createSignal(false);
  const toggle = async () => {
    const next = !state()?.starred;
    setBusy(true);
    try {
      mutate(await api.post("/v1/repos/{owner}/{name}/star", call(), { star: next }));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Show when={state()}>
      {(s) => (
        <button type="button" class="star" aria-pressed={s().starred} disabled={busy()} onClick={() => void toggle()} title="Starred repos show on Your work; the star is on WACS too">
          {s().starred ? "★ Starred" : "☆ Star"}
        </button>
      )}
    </Show>
  );
}
