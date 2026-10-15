import { render } from "solid-js/web";
import { A, Route, Router, useNavigate, type RouteSectionProps } from "@solidjs/router";
import { Show, lazy } from "solid-js";
import "./styles.css";
import Home from "./pages/Home";
import Language from "./pages/Language";
import Search from "./pages/Search";
import Biel from "./pages/Biel";
import Status from "./pages/Status";
import Checks from "./pages/Checks";
import { SessionCorner, me } from "./ui/session";
import { TourOutlet } from "./tour/TourOutlet";

// The repo page carries the USFM viewer (CodeMirror + the engine wasm), so it
// loads on demand.
const Repo = lazy(() => import("./pages/Repo"));
// Chart candidates (Observable Plot), loaded on demand.
const Viz = lazy(() => import("./pages/Viz"));
const Sources = lazy(() => import("./pages/Sources"));
const Welcome = lazy(() => import("./pages/Welcome"));

function Layout(props: RouteSectionProps) {
  const navigate = useNavigate();
  const search = (e: SubmitEvent) => {
    e.preventDefault();
    const q = new FormData(e.currentTarget as HTMLFormElement).get("q");
    navigate(`/search?q=${encodeURIComponent(String(q ?? ""))}`);
  };
  return (
    <>
      <header class="top">
        <A class="brand" href="/" end>
          Translation Hub
        </A>
        <form class="search-inline" role="search" onSubmit={search}>
          <input name="q" type="search" placeholder="Search languages and repos…" aria-label="Search" autocomplete="off" />
        </form>
        <nav>
          <Show when={me()?.me}>
            <A href="/welcome" activeClass="on">Your work</A>
          </Show>
          <A href="/biel" activeClass="on">BIEL conflicts</A>
          <A href="/checks" activeClass="on">Checks</A>
          <A href="/status" activeClass="on">Status</A>
        </nav>
        <SessionCorner />
      </header>
      <main>{props.children}</main>
      {/* High in the tree, so a tour crosses routes. */}
      <TourOutlet />
    </>
  );
}

function NotFound() {
  return (
    <>
      <h1>Not found</h1>
      <p>
        <A href="/">Home</A>
      </p>
    </>
  );
}

render(
  () => (
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/welcome" component={Welcome} />
      <Route path="/repos/:owner/:name" component={Repo} />
      <Route path="/languages/:ietf" component={Language} />
      <Route path="/languages/:ietf/sources" component={Sources} />
      <Route path="/search" component={Search} />
      <Route path="/biel" component={Biel} />
      <Route path="/checks" component={Checks} />
      <Route path="/status" component={Status} />
      <Route path="/viz" component={Viz} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  document.getElementById("app")!,
);
