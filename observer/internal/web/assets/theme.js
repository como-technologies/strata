// The choice of a theme: auto follows the system, and light or dark sets data-theme on the root. It
// runs in the head and before the first paint, so a page in a dark room does not flash white. It
// keeps the choice in this browser only, and it sends it nowhere.
(() => {
  const key = "strata-observer-theme";
  const read = () => {
    try {
      const v = localStorage.getItem(key);
      return v === "light" || v === "dark" ? v : "auto";
    } catch {
      return "auto";
    }
  };
  const apply = v => {
    if (v === "auto") {
      document.documentElement.removeAttribute("data-theme");
    } else {
      document.documentElement.setAttribute("data-theme", v);
    }
    document.querySelectorAll("[data-theme-choice]").forEach(b => {
      b.setAttribute("aria-pressed", String(b.dataset.themeChoice === v));
    });
  };
  apply(read());
  addEventListener("DOMContentLoaded", () => {
    apply(read());
    document.addEventListener("click", e => {
      const b = e.target.closest("[data-theme-choice]");
      if (!b) return;
      const v = b.dataset.themeChoice;
      try {
        if (v === "auto") localStorage.removeItem(key);
        else localStorage.setItem(key, v);
      } catch {
        // A browser that refuses storage still gets the choice for this page.
      }
      apply(v);
    });
  });
})();
