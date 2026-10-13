// The repo's files as a collapsible tree. Directories on the way to the open
// file start open; so does a small top level.
import { For, Show, createEffect } from "solid-js";
import type { TreeEntry } from "../../api/types";

interface Dir {
  readonly dirs: Map<string, Dir>;
  readonly files: { name: string; path: string; size: number | null | undefined }[];
}

function build(entries: readonly TreeEntry[]): Dir {
  const root: Dir = { dirs: new Map(), files: [] };
  for (const e of entries) {
    if (e.kind !== "blob") continue;
    const parts = e.path.split("/");
    let d = root;
    for (const p of parts.slice(0, -1)) {
      let next = d.dirs.get(p);
      if (!next) d.dirs.set(p, (next = { dirs: new Map(), files: [] }));
      d = next;
    }
    d.files.push({ name: parts.at(-1)!, path: e.path, size: e.size });
  }
  return root;
}

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

const size = (n: number | null | undefined) =>
  n == null ? "" : n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(0)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;

export default function FileTree(props: { entries: readonly TreeEntry[]; selected: string | null; onPick: (path: string) => void }) {
  let nav!: HTMLElement;
  const root = () => build(props.entries);
  // Keep the open file in view inside the tree (not the page).
  createEffect(() => {
    if (!props.selected) return;
    requestAnimationFrame(() => {
      const on = nav.querySelector<HTMLElement>(".file.on");
      if (!on) return;
      const a = on.getBoundingClientRect();
      const b = nav.getBoundingClientRect();
      if (a.top < b.top || a.bottom > b.bottom) nav.scrollTop += a.top - b.top - b.height / 3;
    });
  });
  const Level = (p: { dir: Dir; prefix: string; depth: number }) => (
    <>
      <For each={[...p.dir.dirs.keys()].sort(byName)}>
        {(name) => {
          const path = `${p.prefix}${name}/`;
          const open = () => (props.selected?.startsWith(path) ?? false) || (p.depth === 0 && p.dir.dirs.size + p.dir.files.length < 8);
          return (
            <details open={open()}>
              <summary>{name}/</summary>
              <Level dir={p.dir.dirs.get(name)!} prefix={path} depth={p.depth + 1} />
            </details>
          );
        }}
      </For>
      <For each={p.dir.files.toSorted((a, b) => byName(a.name, b.name))}>
        {(f) => (
          <button type="button" class="file" classList={{ on: props.selected === f.path }} aria-current={props.selected === f.path ? "true" : undefined} title={`${f.path} · ${size(f.size)}`} onClick={() => props.onPick(f.path)}>
            {f.name}
          </button>
        )}
      </For>
    </>
  );
  return (
    <nav class="tree" aria-label="Files" ref={nav}>
      <Show when={props.entries.length} fallback={<p class="small">No files.</p>}>
        <Level dir={root()} prefix="" depth={0} />
      </Show>
    </nav>
  );
}
