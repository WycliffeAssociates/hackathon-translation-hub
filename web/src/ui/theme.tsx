// Light, dark, or the system's: one switch in the header, cycling through the
// three, kept in this browser. It sets data-theme on <html>, which the hub's
// styles, its engine tokens and Sefer's editor.css all honour.
import { createSignal } from "solid-js";

type Theme = "system" | "light" | "dark";
const KEY = "hub.theme";

const read = (): Theme => {
  try {
    const t = localStorage.getItem(KEY);
    return t === "light" || t === "dark" ? t : "system";
  } catch {
    return "system";
  }
};

const apply = (t: Theme) => {
  if (t === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", t);
};

// Applied as the module loads, so the first paint is already right.
const [theme, setTheme] = createSignal<Theme>(read());
apply(theme());

const NEXT: Record<Theme, Theme> = { system: "light", light: "dark", dark: "system" };
const LABEL: Record<Theme, string> = { system: "◐", light: "☀", dark: "☾" };

export function ThemeToggle() {
  const cycle = () => {
    const t = NEXT[theme()];
    setTheme(t);
    apply(t);
    try {
      if (t === "system") localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, t);
    } catch {
      // Storage blocked: the choice lasts for this page.
    }
  };
  return (
    <button type="button" class="theme-toggle" onClick={cycle} title={`Theme: ${theme()} (click for ${NEXT[theme()]})`} aria-label={`Theme: ${theme()}`}>
      {LABEL[theme()]}
    </button>
  );
}
