import { render } from "solid-js/web";
import { A, Route, Router, useNavigate, type RouteSectionProps } from "@solidjs/router";
import { lazy } from "solid-js";
import "./styles.css";
import Home from "./pages/Home";
import Language from "./pages/Language";
import Search from "./pages/Search";
import Biel from "./pages/Biel";
import Status from "./pages/Status";
import Checks from "./pages/Checks";

// The repo page carries the USFM viewer (CodeMirror + the engine wasm), so it
// loads on demand.
const Repo = lazy(() => import("./pages/Repo"));

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
          <A href="/biel" activeClass="on">BIEL conflicts</A>
          <A href="/checks" activeClass="on">Checks</A>
          <A href="/status" activeClass="on">Status</A>
        </nav>
      </header>
      <main>{props.children}</main>
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
      <Route path="/repos/:owner/:name" component={Repo} />
      <Route path="/languages/:ietf" component={Language} />
      <Route path="/search" component={Search} />
      <Route path="/biel" component={Biel} />
      <Route path="/checks" component={Checks} />
      <Route path="/status" component={Status} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  document.getElementById("app")!,
);
