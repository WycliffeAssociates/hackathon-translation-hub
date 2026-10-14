// Who is signed in, for every page: one /v1/me read, shared, re-read after
// sign-out. Sign-in itself is a full-page trip to Forgejo and back
// (/v1/auth/login, rel="external" so the router lets the browser go), so a
// fresh page load is all it needs.
import { Show, createRoot, createResource } from "solid-js";
import { api } from "../api/client";

const session = createRoot(() => {
  const [me, { refetch }] = createResource(() => api.json("/v1/me").catch(() => null));
  return { me, refetch };
});

export const me = () => session.me.latest ?? null;

/** Where to go to sign in and come back here. */
export const signInHref = () => `/v1/auth/login?next=${encodeURIComponent(location.pathname + location.search)}`;

export const signOut = async () => {
  await fetch("/v1/auth/logout", { method: "POST" }).catch(() => undefined);
  await session.refetch();
};

/** The header's corner: sign in, or who you are and sign out. Nothing when publishing is off. */
export function SessionCorner() {
  return (
    <Show when={me()?.publishing}>
      <Show
        when={me()?.me}
        fallback={
          <a class="session-link" href={signInHref()} rel="external">
            Sign in
          </a>
        }
      >
        {(m) => (
          <span class="session">
            <Show when={m().avatar_url}>{(src) => <img src={src()} alt="" width="22" height="22" />}</Show>
            <span title={m().full_name ?? undefined}>{m().username}</span>
            <Show when={m().publisher}>
              <span class="tag official-tag" title={`In ${me()?.official_org}'s publishers team`}>publisher</span>
            </Show>
            <button type="button" class="linkish small" onClick={() => void signOut()}>
              Sign out
            </button>
          </span>
        )}
      </Show>
    </Show>
  );
}
