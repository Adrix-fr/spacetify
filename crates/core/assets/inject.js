// Phase 1 proof of injection: a badge and a console line.
(() => {
  "use strict";
  if (window.__spacetify) return;
  window.__spacetify = { phase: 1 };

  console.log("%c[spacetify]%c hello world, injection works", "color:#ff3d7f;font-weight:bold", "");

  const showBadge = () => {
    if (document.getElementById("spacetify-badge")) return;
    const badge = document.createElement("div");
    badge.id = "spacetify-badge";
    badge.textContent = navigator.language.startsWith("fr") ? "Injecté ✓" : "Injected ✓";
    document.body.appendChild(badge);
  };

  if (document.body) showBadge();
  else document.addEventListener("DOMContentLoaded", showBadge, { once: true });
})();
