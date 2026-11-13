// The colour theme: System, Light or Dark. The choice is stored under the same key the docs use,
// so it carries over between the landing page and /docs/. Loaded in <head> so the stored theme
// applies before the first paint.
(() => {
  const KEY = "wiki-theme";
  const root = document.documentElement;

  const read = () => {
    try {
      const value = localStorage.getItem(KEY);
      return value === "light" || value === "dark" ? value : "system";
    } catch {
      return "system";
    }
  };

  const apply = (choice) => {
    if (choice === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", choice);
    for (const button of document.querySelectorAll("[data-theme-choice]")) {
      button.setAttribute("aria-pressed", String(button.dataset.themeChoice === choice));
    }
  };

  apply(read());

  document.addEventListener("DOMContentLoaded", () => {
    apply(read());
    for (const button of document.querySelectorAll("[data-theme-choice]")) {
      button.addEventListener("click", () => {
        const choice = button.dataset.themeChoice;
        try {
          if (choice === "system") localStorage.removeItem(KEY);
          else localStorage.setItem(KEY, choice);
        } catch {
          // Storage can be blocked; the choice then lasts until the page is left.
        }
        apply(choice);
      });
    }
  });
})();
