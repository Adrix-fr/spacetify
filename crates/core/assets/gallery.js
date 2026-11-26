// spacetify gallery: a page inside Spotify to browse, install and turn on
// themes and extensions from the official list.
//
// The page cannot write files: everything goes through the background
// service, on 127.0.0.1, with the secret spacetify put into the payload
// (see crates/core/src/gallery/server.rs). This file is one function, put
// into runtime.js when the payload is built. It must never break Spotify:
// every handler is guarded, and the page only uses its own elements.
(function startGallery(link, log) {
  "use strict";

  const ROOT_ID = "spacetify-gallery";
  const BUTTON_ID = "spacetify-gallery-button";
  const LIVE_THEME_ID = "spacetify-theme-gallery";
  // A stable hook in Spotify's interface: the button goes right after the
  // back and forward arrows. Without it the button floats in a corner and
  // the page covers the whole window, so the gallery still works.
  const FORWARD = '[data-testid="top-bar-forward-button"]';
  const FLOAT_AFTER_MS = 10000;

  // ------------------------------------------------------------ helpers

  const french = () =>
    String(document.documentElement.lang || navigator.language || "")
      .toLowerCase()
      .startsWith("fr");
  const t = (en, fr) => (french() ? fr : en);
  const textOf = (text) => (text ? (french() ? text.fr : text.en) || text.en || "" : "");

  const safe = (fn) =>
    function guarded(...args) {
      try {
        const result = fn.apply(this, args);
        if (result && typeof result.then === "function") {
          result.then(undefined, (error) => log("error", [error]));
        }
        return result;
      } catch (error) {
        log("error", [error]);
        return undefined;
      }
    };

  const h = (tag, props, ...children) => {
    const el = document.createElement(tag);
    for (const [key, value] of Object.entries(props || {})) {
      if (value === undefined || value === null || value === false) continue;
      if (key === "class") el.className = value;
      else if (key.startsWith("on")) el.addEventListener(key.slice(2), safe(value));
      else el.setAttribute(key, value === true ? "" : String(value));
    }
    for (const child of children.flat(Infinity)) {
      if (child === undefined || child === null || child === false) continue;
      el.append(child instanceof Node ? child : String(child));
    }
    return el;
  };

  // Only for the constant drawings below, never for data.
  const icon = (markup) => {
    const span = document.createElement("span");
    span.className = "stg-icon";
    span.setAttribute("aria-hidden", "true");
    span.innerHTML = markup;
    return span;
  };
  const ICONS = {
    logo:
      '<svg width="20" height="20" viewBox="0 0 64 64" fill="none"><ellipse cx="32" cy="33" rx="29" ry="9" transform="rotate(-15 32 33)" stroke="#ff6b81" stroke-width="4"/><circle cx="32" cy="33" r="18" fill="#ff6b81"/><path d="M21 28c7-3 16-2.6 23 1M22.5 34c6-2.3 13-1.9 19 .8M24.5 39.6c5-1.6 10-1.2 13.5.3" stroke="#0e0e0f" stroke-width="3.2" stroke-linecap="round"/></svg>',
    search:
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>',
    download:
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11M7 10.5l5 5 5-5M5 20h14"/></svg>',
    check:
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
    plus:
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>',
    back:
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 18l-6-6 6-6"/></svg>',
    close:
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg>',
    restart:
      '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#e5a83b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 4v5h-5"/></svg>',
    power:
      '<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#e5a83b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18.4 6.6A9 9 0 1 1 5.6 6.6"/><path d="M12 2v10"/></svg>',
    offline:
      '<svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#a1a1a6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 8.5a15 15 0 0 1 20 0M5.5 12a10 10 0 0 1 13 0M9 15.5a5 5 0 0 1 6 0M12 19v.01M3 3l18 18"/></svg>',
    code:
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18l-6-6 6-6M15 6l6 6-6 6"/></svg>',
  };

  // ------------------------------------------------------------- styles

  const STYLE = `
#${BUTTON_ID}{display:inline-flex;align-items:center;justify-content:center;width:32px;height:32px;padding:0;
  margin:0 4px 0 8px;border:0;background:transparent;cursor:pointer;flex-shrink:0;opacity:.85;
  position:relative;top:0;align-self:center;vertical-align:middle;line-height:0;
  -webkit-app-region:no-drag;transition:opacity .15s,transform .15s}
#${BUTTON_ID}:hover{opacity:1;transform:scale(1.08)}
#${BUTTON_ID}[aria-pressed="true"]{opacity:1}
#${BUTTON_ID} svg{display:block;width:26px;height:26px}
#${BUTTON_ID}.stg-floating{position:fixed;top:auto !important;right:16px;bottom:104px;z-index:2147483000;width:44px;height:44px;
  border-radius:22px;background:#1f1f22;box-shadow:0 8px 24px rgba(0,0,0,.5)}
#${ROOT_ID}{position:fixed;z-index:2147482000;background:#0e0e0f;color:#ededed;border-radius:8px;overflow:hidden;
  display:flex;flex-direction:column;font-size:14px;line-height:20px;-webkit-font-smoothing:antialiased;
  font-variant-numeric:tabular-nums}
#${ROOT_ID} *{box-sizing:border-box}
#${ROOT_ID} button{font:inherit;color:inherit;cursor:pointer}
#${ROOT_ID} button:disabled{cursor:default;opacity:.6}
#${ROOT_ID} :focus{outline:none}
#${ROOT_ID} :focus-visible{outline:2px solid #ff6b81;outline-offset:2px}
.stg-icon{display:inline-flex;flex-shrink:0}
.stg-scroll{flex:1;overflow-y:auto;padding:8px 48px 120px}
.stg-scroll::-webkit-scrollbar{width:10px}
.stg-scroll::-webkit-scrollbar-thumb{background:rgba(237,237,237,.14);border-radius:5px}
.stg-topbar{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:16px 24px 8px}
.stg-leave{height:36px;padding:0 16px 0 10px;border-radius:18px;border:0;background:#1f1f22;color:#ededed;
  font-weight:700;font-size:13px;display:inline-flex;align-items:center;gap:6px}
.stg-leave:hover{background:#2a2a2e}
.stg-close{width:36px;height:36px;border-radius:18px;border:0;background:#1f1f22;color:#a1a1a6;display:flex;
  align-items:center;justify-content:center}
.stg-close:hover{color:#ededed;background:#2a2a2e}
.stg-head{display:flex;align-items:flex-end;justify-content:space-between;gap:24px;flex-wrap:wrap}
.stg-brand{display:flex;align-items:center;gap:8px;color:#a1a1a6;font-size:13px;font-weight:700}
.stg-title{margin:6px 0 4px;font-size:30px;line-height:36px;font-weight:700;letter-spacing:-.01em}
.stg-sub{margin:0;color:#a1a1a6}
.stg-actions{display:flex;align-items:center;gap:12px;flex-wrap:wrap}
.stg-search{width:280px;max-width:100%;height:40px;border-radius:8px;background:#161618;border:1px solid rgba(237,237,237,.08);
  display:flex;align-items:center;gap:10px;padding:0 12px;color:#85858b}
.stg-search input{flex:1;min-width:0;background:transparent;border:0;outline:none;color:#ededed;font:inherit;font-size:13px}
.stg-hidden{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.stg-tabs{display:flex;gap:4px;margin-top:24px;padding:4px;background:#161618;border-radius:10px;width:fit-content;
  border:1px solid rgba(237,237,237,.08)}
.stg-tab{height:34px;padding:0 16px;border-radius:7px;border:0;background:transparent;color:#a1a1a6;font-weight:700;font-size:13px}
.stg-tab[aria-pressed="true"]{background:#2a2a2e;color:#ededed}
.stg-notice{margin-top:20px;display:flex;align-items:center;gap:10px;padding:10px 14px;border-radius:8px;width:fit-content;max-width:100%}
.stg-notice.stg-good{background:rgba(76,195,138,.1);color:#bfe9d3}
.stg-notice.stg-good .stg-icon{color:#4cc38a}
.stg-notice.stg-warn{background:rgba(229,168,59,.1);color:#f1d6a4}
.stg-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:16px;margin-top:20px}
.stg-card{background:#161618;border:1px solid rgba(237,237,237,.08);border-radius:12px;overflow:hidden;display:flex;flex-direction:column}
.stg-card:hover{border-color:rgba(237,237,237,.16)}
.stg-preview{height:132px;padding:10px;display:grid;grid-template-columns:52px 1fr;grid-template-rows:1fr 18px;gap:6px}
.stg-preview.stg-big{height:340px;padding:14px;grid-template-columns:120px 1fr;grid-template-rows:1fr 36px;gap:10px;border-radius:12px}
.stg-pane{border-radius:6px;padding:9px;display:flex;flex-direction:column;gap:6px;overflow:hidden}
.stg-bar{height:8px;border-radius:4px}
.stg-tiles{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:6px;margin-top:4px}
.stg-tile{height:34px;border-radius:4px}
.stg-big .stg-tile{height:84px}
.stg-play{grid-column:1/3;display:flex;align-items:center;justify-content:center}
.stg-play span{width:14px;height:14px;border-radius:7px}
.stg-big .stg-play span{width:22px;height:22px;border-radius:11px}
.stg-body{padding:16px;display:flex;flex-direction:column;gap:12px;flex:1}
.stg-row{display:flex;gap:12px;align-items:flex-start}
.stg-letter{width:40px;height:40px;border-radius:10px;background:rgba(255,107,129,.1);color:#ff6b81;display:flex;
  align-items:center;justify-content:center;font-weight:700;font-size:16px;flex-shrink:0}
.stg-name{font-weight:700;font-size:15px;color:#ededed;background:none;border:0;padding:0;text-align:left}
button.stg-name{text-decoration:underline;text-decoration-color:rgba(237,237,237,.3);text-underline-offset:3px}
.stg-meta{color:#85858b;font-size:12px}
.stg-desc{margin:0;color:#a1a1a6;font-size:13px;line-height:19px;flex:1}
.stg-foot{display:flex;align-items:center;justify-content:space-between;gap:12px;min-height:36px}
.stg-primary{height:36px;padding:0 16px;border-radius:8px;border:0;background:#ff6b81;color:#0e0e0f !important;
  font-weight:700;font-size:13px;display:inline-flex;align-items:center;gap:8px}
.stg-primary:hover:not(:disabled){filter:brightness(1.1)}
.stg-secondary{height:36px;padding:0 16px;border-radius:8px;border:1px solid rgba(237,237,237,.14);background:#1f1f22;
  font-weight:700;font-size:13px;display:inline-flex;align-items:center;gap:8px;color:#ededed;text-decoration:none}
.stg-quiet{height:40px;padding:0 16px;border-radius:8px;border:0;background:transparent;color:#a1a1a6;font-weight:700;font-size:13px}
.stg-chip{height:36px;padding:0 14px;border-radius:8px;background:rgba(255,107,129,.1);color:#ff8a9b;font-weight:700;
  font-size:13px;display:inline-flex;align-items:center;gap:8px}
.stg-switch{display:flex;align-items:center;gap:10px;color:#a1a1a6;font-size:13px}
.stg-track{width:40px;height:24px;border-radius:12px;border:0;padding:0;position:relative;background:#3a3a3f}
.stg-track span{position:absolute;top:3px;left:3px;width:18px;height:18px;border-radius:9px;background:#a1a1a6;transition:left .2s}
.stg-track[aria-checked="true"]{background:#ff6b81}
.stg-track[aria-checked="true"] span{left:19px;background:#0e0e0f}
.stg-error{margin:0;color:#ff9a9e;font-size:12px;line-height:17px}
.stg-pending{position:absolute;left:32px;right:32px;bottom:20px;display:flex;align-items:center;gap:16px;
  padding:14px 16px 14px 20px;border-radius:12px;background:#1f1f22;border:1px solid rgba(237,237,237,.14);
  box-shadow:0 12px 32px rgba(0,0,0,.5);flex-wrap:wrap}
.stg-pending-text{flex:1;min-width:200px;display:flex;flex-direction:column}
.stg-cover{position:absolute;inset:0;background:rgba(14,14,15,.86);display:flex;align-items:center;justify-content:center;z-index:3}
.stg-dialog{width:420px;max-width:calc(100% - 32px);padding:28px;border-radius:12px;background:#161618;
  border:1px solid rgba(237,237,237,.14);display:flex;flex-direction:column;gap:12px}
.stg-progress{height:6px;border-radius:3px;background:#2a2a2e;overflow:hidden;margin-top:4px}
.stg-progress span{display:block;height:6px;border-radius:3px;background:#ff6b81;width:0;animation:stg-fill 6s cubic-bezier(.22,1,.36,1) forwards}
@keyframes stg-fill{to{width:100%}}
.stg-empty{margin:48px auto 0;max-width:420px;display:flex;flex-direction:column;align-items:center;gap:14px;text-align:center}
.stg-empty-icon{width:56px;height:56px;border-radius:28px;background:#1f1f22;display:flex;align-items:center;justify-content:center}
.stg-empty-title{font-size:18px;font-weight:700}
.stg-back{display:inline-flex;align-items:center;gap:6px;color:#a1a1a6;font-weight:700;font-size:13px;background:none;border:0;padding:0}
.stg-detail{display:flex;gap:32px;align-items:flex-start;margin-top:24px;flex-wrap:wrap}
.stg-detail .stg-preview{flex:1 1 340px;max-width:520px;height:300px}
.stg-detail-text{flex:1 1 240px;min-width:240px;display:flex;flex-direction:column;gap:16px}
.stg-checks{background:#161618;border:1px solid rgba(237,237,237,.08);border-radius:12px;padding:16px 18px;
  display:flex;flex-direction:column;gap:12px}
.stg-checks .stg-row{gap:10px;color:#c8c8cc}
.stg-checks .stg-icon{color:#4cc38a;margin-top:3px}
.stg-shot{display:block;width:100%;height:100%;object-fit:cover;background:#1f1f22}
.stg-coverart{display:block;width:100%;height:150px;padding:0;border:0;background:#1f1f22;overflow:hidden;cursor:pointer}
.stg-coverart .stg-shot{transition:transform .3s}
.stg-card:hover .stg-coverart .stg-shot{transform:scale(1.03)}
.stg-tags{display:flex;gap:6px;flex-wrap:wrap}
.stg-tag{height:22px;padding:0 9px;border-radius:11px;background:#232327;color:#a1a1a6;font-size:11px;font-weight:700;
  display:inline-flex;align-items:center}
.stg-shots{flex:1 1 380px;max-width:640px;display:flex;flex-direction:column;gap:10px}
.stg-shot-main{display:block;width:100%;aspect-ratio:16/9;padding:0;border:0;border-radius:12px;overflow:hidden;
  background:#1f1f22;cursor:zoom-in}
.stg-thumbs{display:flex;gap:8px;flex-wrap:wrap}
.stg-thumb{width:88px;height:52px;padding:0;border:2px solid transparent;border-radius:8px;overflow:hidden;background:#1f1f22;opacity:.7}
.stg-thumb:hover{opacity:1}
.stg-thumb[aria-pressed="true"]{border-color:#ff6b81;opacity:1}
.stg-about{margin-top:36px;max-width:760px;color:#c8c8cc;line-height:22px}
.stg-about h2{font-size:20px;line-height:28px;margin:0 0 12px;color:#ededed}
.stg-about h3,.stg-about h4,.stg-about h5{color:#ededed;margin:20px 0 8px}
.stg-about p{margin:0 0 12px}
.stg-about ul,.stg-about ol{margin:0 0 12px;padding-left:22px}
.stg-about li{margin-bottom:4px}
.stg-about code{background:#1f1f22;border-radius:4px;padding:1px 5px;font-size:12px}
.stg-about pre{background:#161618;border:1px solid rgba(237,237,237,.08);border-radius:8px;padding:12px;overflow:auto;
  font-size:12px;line-height:18px;margin:0 0 12px}
.stg-about blockquote{margin:0 0 12px;padding-left:12px;border-left:3px solid #3a3a3f;color:#a1a1a6}
.stg-about a{color:#ff8a9b}
.stg-lightbox{position:absolute;inset:0;z-index:4;background:rgba(14,14,15,.94);display:flex;align-items:center;
  justify-content:center;padding:32px;cursor:zoom-out}
.stg-lightbox .stg-shot{width:auto;height:auto;max-width:100%;max-height:100%;object-fit:contain;border-radius:8px;background:none}
`;

  // -------------------------------------------------------------- state

  const state = {
    open: false,
    tab: "themes",
    query: "",
    detail: null,
    // Picture shown large on the detail page, and the one shown full size.
    detailImage: 0,
    lightbox: null,
    loading: false,
    data: null,
    serviceDown: false,
    busy: new Set(),
    errors: new Map(),
    notice: null,
    // An extension changed: Spotify must restart for it.
    restartNeeded: false,
    restarting: false,
  };

  const call = async (path, body) => {
    const headers = { "x-spacetify-token": link.token };
    if (body !== undefined) headers["content-type"] = "application/json";
    let response;
    try {
      response = await fetch(link.url + path, {
        method: body === undefined ? "GET" : "POST",
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        cache: "no-store",
      });
    } catch {
      const error = new Error("the spacetify service cannot be reached");
      error.serviceDown = true;
      throw error;
    }
    const data = await response.json().catch(() => ({}));
    if (!response.ok || data.error) {
      const error = new Error("gallery request failed");
      error.text = data.error && typeof data.error === "object" ? data.error : null;
      throw error;
    }
    return data;
  };

  const load = async (refresh) => {
    state.loading = true;
    render();
    try {
      state.data = await call(refresh ? "/v1/state?refresh=1" : "/v1/state");
      state.serviceDown = false;
    } catch (error) {
      if (error.serviceDown) state.serviceDown = true;
      else state.notice = { tone: "warn", text: textOf(error.text) || String(error.message) };
    } finally {
      state.loading = false;
      render();
    }
  };

  // Shows a theme at once, without waiting for Spotify to restart.
  const showTheme = (css) => {
    for (const el of document.querySelectorAll('link[href$="spacetify/inject.css"]')) {
      el.disabled = true;
    }
    const live = document.getElementById("spacetify-theme");
    if (live) live.textContent = "";
    let style = document.getElementById(LIVE_THEME_ID);
    if (!style) {
      style = document.createElement("style");
      style.id = LIVE_THEME_ID;
      document.body.appendChild(style);
    }
    style.textContent = css;
  };

  const act = async (mod, path) => {
    state.busy.add(mod.id);
    state.errors.delete(mod.id);
    state.notice = null;
    render();
    try {
      const answer = await call(path, { id: mod.id });
      if (answer.state) state.data = answer.state;
      if (typeof answer.theme_css === "string") {
        showTheme(answer.theme_css);
        state.notice = {
          tone: "good",
          text: answer.theme_css
            ? t(`“${mod.name}” is on, no need to restart Spotify.`, `« ${mod.name} » est appliqué tout de suite, sans relancer Spotify.`)
            : t("Back to Spotify's own look.", "Retour au look de Spotify."),
        };
      } else {
        state.restartNeeded = true;
      }
    } catch (error) {
      if (error.serviceDown) state.serviceDown = true;
      else state.errors.set(mod.id, textOf(error.text) || t("Something went wrong.", "Un problème est survenu."));
    } finally {
      state.busy.delete(mod.id);
      render();
    }
  };

  // Opens the spacetify app on its "Submit" page, which checks the mod and
  // prepares the request.
  const submitMod = async () => {
    try {
      await call("/v1/open-submit", {});
      state.notice = {
        tone: "good",
        text: t("The spacetify app opens on “Submit a mod”.", "L'appli spacetify s'ouvre sur « Proposer un mod »."),
      };
    } catch (error) {
      if (error.serviceDown) state.serviceDown = true;
      else
        state.notice = {
          tone: "warn",
          text:
            textOf(error.text) ||
            t("Open the spacetify app, then go to Submit.", "Ouvre l'appli spacetify, puis va sur Proposer."),
        };
    }
    render();
  };

  const restart = async () => {
    state.restarting = true;
    render();
    try {
      await call("/v1/restart", {});
    } catch (error) {
      state.restarting = false;
      if (error.serviceDown) state.serviceDown = true;
      else state.notice = { tone: "warn", text: textOf(error.text) };
      render();
    }
  };

  // ----------------------------------------------------------- showcase

  // Pictures and texts of a mod's showcase. The service downloads each file
  // once, checks its fingerprint and gives pictures as data: URLs, so the
  // page never loads anything from the web itself.
  const showcaseFiles = new Map();
  const showcaseFile = (mod, name) => {
    const key = `${mod.id}@${mod.version}/${name}`;
    let answer = showcaseFiles.get(key);
    if (!answer) {
      answer = call(`/v1/showcase?id=${encodeURIComponent(mod.id)}&file=${encodeURIComponent(name)}`);
      showcaseFiles.set(key, answer);
      answer.catch(() => showcaseFiles.delete(key));
    }
    return answer;
  };

  // Card pictures are only fetched once they come near the screen: after
  // each change of the page and while it scrolls, the waiting ones that are
  // close to the window are started.
  const waiting = new Map();
  const startVisible = () => {
    for (const [img, start] of waiting) {
      if (!img.isConnected) {
        waiting.delete(img);
        continue;
      }
      const rect = img.getBoundingClientRect();
      if (rect.top < window.innerHeight + 300 && rect.bottom > -300) {
        waiting.delete(img);
        start();
      }
    }
  };
  let lookQueued = false;
  const lookSoon = () => {
    if (lookQueued) return;
    lookQueued = true;
    setTimeout(
      safe(() => {
        lookQueued = false;
        startVisible();
      }),
      60,
    );
  };

  const picture = (mod, name, later) => {
    const img = h("img", { class: "stg-shot", alt: "", decoding: "async", draggable: "false" });
    const start = () =>
      showcaseFile(mod, name).then(
        safe((answer) => {
          if (answer && typeof answer.data === "string" && answer.data.startsWith("data:image/")) img.src = answer.data;
        }),
        () => {},
      );
    if (later) {
      waiting.set(img, start);
      lookSoon();
    } else {
      start();
    }
    return img;
  };

  const imagesOf = (mod) => (mod.showcase && Array.isArray(mod.showcase.images) ? mod.showcase.images : []);
  const tagsOf = (mod) => (mod.showcase && Array.isArray(mod.showcase.tags) ? mod.showcase.tags : []);

  const tags = (mod, max) => {
    const list = tagsOf(mod).slice(0, max);
    return list.length ? h("div", { class: "stg-tags" }, list.map((tag) => h("span", { class: "stg-tag" }, tag))) : null;
  };

  // Bold, italics, code and https links. Everything is built as elements:
  // the text of a mod is never read as HTML.
  const inline = (text) => {
    const out = [];
    const pattern = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\((https:\/\/[^)\s]+)\)|\*([^*\s][^*]*)\*|_([^_\s][^_]*)_/g;
    let last = 0;
    let match;
    while ((match = pattern.exec(text))) {
      if (match.index > last) out.push(text.slice(last, match.index));
      if (match[1]) out.push(h("strong", {}, match[1]));
      else if (match[2]) out.push(h("code", {}, match[2]));
      else if (match[3])
        out.push(h("a", { href: match[4], target: "_blank", rel: "noopener noreferrer", title: match[4] }, match[3]));
      else out.push(h("em", {}, match[5] || match[6]));
      last = pattern.lastIndex;
    }
    if (last < text.length) out.push(text.slice(last));
    return out;
  };

  // A small part of Markdown: titles, paragraphs, lists, quotes and code.
  const markdown = (text) => {
    const blocks = [];
    let paragraph = [];
    let list = null;
    let code = null;
    const endParagraph = () => {
      if (paragraph.length) blocks.push(h("p", {}, inline(paragraph.join(" "))));
      paragraph = [];
    };
    for (const raw of String(text).replace(/\r/g, "").split("\n")) {
      if (code) {
        if (raw.trim().startsWith("```")) {
          blocks.push(h("pre", {}, code.join("\n")));
          code = null;
        } else code.push(raw);
        continue;
      }
      const line = raw.trim();
      if (line.startsWith("```")) {
        endParagraph();
        list = null;
        code = [];
        continue;
      }
      if (!line) {
        endParagraph();
        list = null;
        continue;
      }
      const heading = /^(#{1,3})\s+(.*)$/.exec(line);
      if (heading) {
        endParagraph();
        list = null;
        blocks.push(h(`h${heading[1].length + 2}`, {}, inline(heading[2])));
        continue;
      }
      const item = /^(?:[-*+]|\d+[.)])\s+(.*)$/.exec(line);
      if (item) {
        endParagraph();
        if (!list) {
          list = h(/^\d/.test(line) ? "ol" : "ul");
          blocks.push(list);
        }
        list.append(h("li", {}, inline(item[1])));
        continue;
      }
      if (line.startsWith(">")) {
        endParagraph();
        list = null;
        blocks.push(h("blockquote", {}, inline(line.replace(/^>\s?/, ""))));
        continue;
      }
      list = null;
      paragraph.push(line);
    }
    if (code) blocks.push(h("pre", {}, code.join("\n")));
    endParagraph();
    return blocks;
  };

  const about = (mod) => {
    const showcase = mod.showcase || {};
    const name = french() && showcase.about_fr ? "about.fr.md" : showcase.about ? "about.md" : showcase.about_fr ? "about.fr.md" : null;
    if (!name) return null;
    const body = h("div", {}, h("p", { style: "color:#85858b" }, t("Loading…", "Chargement…")));
    showcaseFile(mod, name).then(
      safe((answer) => body.replaceChildren(...markdown(answer && typeof answer.text === "string" ? answer.text : ""))),
      () => body.replaceChildren(h("p", { style: "color:#85858b" }, t("This text cannot be shown right now.", "Ce texte ne peut pas s'afficher pour l'instant."))),
    );
    return h("section", { class: "stg-about" }, h("h2", {}, t("About", "À propos")), body);
  };

  const day = (text) => {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(text || "")) return "";
    const date = new Date(`${text}T00:00:00Z`);
    return Number.isNaN(date.getTime())
      ? ""
      : date.toLocaleDateString(french() ? "fr-FR" : "en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
  };

  // -------------------------------------------------------------- views

  const preview = (colors, big) => {
    const pane = (...children) => {
      const el = h("div", { class: "stg-pane" }, ...children);
      el.style.background = colors.background;
      return el;
    };
    const paint = (el, color, extra) => {
      el.style.background = color;
      if (extra) Object.assign(el.style, extra);
      return el;
    };
    const bar = (color, width, opacity) =>
      paint(h("span", { class: "stg-bar" }), color, { width: width || "", opacity: opacity || "" });
    const root = h(
      "div",
      { class: big ? "stg-preview stg-big" : "stg-preview", "aria-hidden": "true" },
      pane(bar(colors.highlight), bar(colors.highlight), bar(colors.highlight)),
      pane(
        bar(colors.highlight, "58%"),
        bar(colors.text, "36%", "0.55"),
        h("div", { class: "stg-tiles" }, [0, 1, 2, 3].map(() => paint(h("span", { class: "stg-tile" }), colors.highlight))),
      ),
      h("div", { class: "stg-play" }, paint(h("span"), colors.accent)),
    );
    root.style.background = colors.frame;
    return root;
  };

  const description = (mod) => (french() && mod.description_fr) || mod.description || "";

  const isTheme = (mod) => mod.kind === "theme";

  const actions = (mod) => {
    const busy = state.busy.has(mod.id);
    const installing = t("Installing…", "Installation…");
    if (!mod.installed || mod.update) {
      return h(
        "button",
        { class: "stg-primary", disabled: busy, onclick: () => act(mod, "/v1/install") },
        icon(ICONS.download),
        busy ? installing : mod.installed ? t("Update", "Mettre à jour") : t("Install", "Installer"),
      );
    }
    if (isTheme(mod)) {
      return mod.active
        ? h("span", { class: "stg-chip" }, icon(ICONS.check), t("Current theme", "Thème actuel"))
        : h(
            "button",
            { class: "stg-secondary", disabled: busy, onclick: () => act(mod, "/v1/enable") },
            t("Use this theme", "Utiliser ce thème"),
          );
    }
    return h(
      "div",
      { class: "stg-switch" },
      h("span", {}, mod.active ? t("On", "Activée") : t("Off", "Désactivée")),
      h(
        "button",
        {
          class: "stg-track",
          role: "switch",
          "aria-checked": mod.active ? "true" : "false",
          "aria-label": `${t("Turn on", "Activer")} ${mod.name}`,
          disabled: busy,
          onclick: () => act(mod, mod.active ? "/v1/disable" : "/v1/enable"),
        },
        h("span"),
      ),
    );
  };

  const kindLabel = (mod) =>
    isTheme(mod)
      ? mod.installed ? t("Theme · installed", "Thème · installé") : t("Theme", "Thème")
      : mod.installed ? t("Extension · installed", "Extension · installée") : t("Extension", "Extension");

  const card = (mod) => {
    const openDetail = () => {
      state.detail = mod.id;
      state.detailImage = 0;
      render();
    };
    const error = state.errors.get(mod.id);
    const cover = imagesOf(mod)[0];
    return h(
      "div",
      { class: "stg-card" },
      cover
        ? h(
            "button",
            { class: "stg-coverart", "aria-label": `${t("See", "Voir")} ${mod.name}`, onclick: openDetail },
            picture(mod, cover, true),
          )
        : isTheme(mod) && mod.preview
          ? preview(mod.preview, false)
          : null,
      h(
        "div",
        { class: "stg-body" },
        h(
          "div",
          { class: "stg-row" },
          isTheme(mod) || cover ? null : h("span", { class: "stg-letter", "aria-hidden": "true" }, (mod.name || "?").charAt(0)),
          h(
            "div",
            { style: "flex:1;min-width:0;display:flex;flex-direction:column;gap:2px" },
            h(
              "div",
              { style: "display:flex;align-items:baseline;gap:8px;flex-wrap:wrap" },
              mod.in_gallery || isTheme(mod)
                ? h("button", { class: "stg-name", onclick: openDetail }, mod.name)
                : h("span", { class: "stg-name" }, mod.name),
              h("span", { class: "stg-meta" }, `v${mod.version}`),
            ),
            h("span", { class: "stg-meta" }, `${t("by", "par")} ${mod.author}`),
          ),
        ),
        h("p", { class: "stg-desc" }, description(mod)),
        tags(mod, 3),
        error ? h("p", { class: "stg-error", role: "alert" }, error) : null,
        h("div", { class: "stg-foot" }, h("span", { class: "stg-meta" }, kindLabel(mod)), actions(mod)),
      ),
    );
  };

  // The pictures of the detail page: one large, the others small below.
  const shots = (mod) => {
    const images = imagesOf(mod);
    if (!images.length) return mod.preview ? preview(mod.preview, true) : null;
    const index = Math.min(state.detailImage, images.length - 1);
    return h(
      "div",
      { class: "stg-shots" },
      h(
        "button",
        {
          class: "stg-shot-main",
          "aria-label": t("Show this picture larger", "Voir cette image en grand"),
          onclick: () => {
            state.lightbox = { mod, name: images[index] };
            render();
          },
        },
        picture(mod, images[index], false),
      ),
      images.length > 1
        ? h(
            "div",
            { class: "stg-thumbs" },
            images.map((name, i) =>
              h(
                "button",
                {
                  class: "stg-thumb",
                  "aria-pressed": i === index ? "true" : "false",
                  "aria-label": `${t("Picture", "Image")} ${i + 1}`,
                  onclick: () => {
                    state.detailImage = i;
                    render();
                  },
                },
                picture(mod, name, false),
              ),
            ),
          )
        : null,
    );
  };

  const detailView = (mod) =>
    h(
      "div",
      {},
      h(
        "button",
        {
          class: "stg-back",
          onclick: () => {
            state.detail = null;
            render();
          },
        },
        icon(ICONS.back),
        t("Gallery", "Galerie"),
      ),
      h(
        "div",
        { class: "stg-detail" },
        shots(mod),
        h(
          "div",
          { class: "stg-detail-text" },
          h(
            "div",
            { style: "display:flex;flex-direction:column;gap:4px" },
            h("span", { class: "stg-meta", style: "font-weight:700" }, isTheme(mod) ? t("Theme", "Thème") : t("Extension", "Extension")),
            h("h2", { class: "stg-title", style: "margin:0" }, mod.name),
            h(
              "span",
              { style: "color:#a1a1a6" },
              `${t("by", "par")} ${mod.author} · ${t("version", "version")} ${mod.version}`,
              day(mod.updated) ? ` · ${t("updated", "mis à jour le")} ${day(mod.updated)}` : "",
            ),
          ),
          tags(mod, 6),
          h("p", { style: "margin:0;color:#c8c8cc;line-height:22px" }, description(mod)),
          state.errors.get(mod.id) ? h("p", { class: "stg-error", role: "alert" }, state.errors.get(mod.id)) : null,
          h(
            "div",
            { style: "display:flex;gap:12px;flex-wrap:wrap;align-items:center" },
            actions(mod),
            mod.code_url
              ? h(
                  "a",
                  { class: "stg-secondary", href: mod.code_url, target: "_blank", rel: "noopener noreferrer" },
                  icon(ICONS.code),
                  t("See the code", "Voir le code"),
                )
              : null,
          ),
          mod.in_gallery
            ? h(
                "div",
                { class: "stg-checks" },
                h("span", { style: "font-weight:700" }, t("Before installing, spacetify checks that:", "Avant d'installer, spacetify vérifie que :")),
                [
                  isTheme(mod)
                    ? t("this theme is in spacetify's official list;", "ce thème fait partie de la liste officielle de spacetify ;")
                    : t("this extension is in spacetify's official list;", "cette extension fait partie de la liste officielle de spacetify ;"),
                  t(
                    "the files received are exactly the ones that were accepted (their fingerprint is compared);",
                    "les fichiers reçus sont exactement ceux qui ont été acceptés (leur empreinte est comparée) ;",
                  ),
                  isTheme(mod)
                    ? t("it is a valid spacetify theme.", "c'est un thème spacetify valide.")
                    : t("it is a valid spacetify extension.", "c'est une extension spacetify valide."),
                ].map((line) => h("div", { class: "stg-row" }, icon(ICONS.check), h("span", {}, line))),
                h(
                  "span",
                  { class: "stg-meta", style: "line-height:18px" },
                  t(
                    "If a check fails, nothing is installed and the gallery says why.",
                    "Si une vérification échoue, rien n'est installé et la galerie te dit pourquoi.",
                  ),
                ),
              )
            : null,
        ),
      ),
      about(mod),
    );

  const empty = (iconMarkup, title, text, retry) =>
    h(
      "div",
      { class: "stg-empty" },
      h("span", { class: "stg-empty-icon" }, icon(iconMarkup)),
      h("span", { class: "stg-empty-title" }, title),
      h("span", { style: "color:#a1a1a6" }, text),
      retry ? h("button", { class: "stg-secondary", onclick: () => load(true) }, t("Try again", "Réessayer")) : null,
    );

  const header = (mods) => {
    const installed = mods.filter((mod) => mod.installed).length;
    const tabs = [
      ["themes", t("Themes", "Thèmes")],
      ["extensions", t("Extensions", "Extensions")],
      ["installed", `${t("Installed", "Installés")} (${installed})`],
    ];
    const search = h("input", {
      type: "search",
      value: state.query,
      placeholder: t("Search themes and extensions", "Chercher un thème ou une extension"),
      oninput: (event) => {
        state.query = event.target.value;
        renderList();
      },
    });
    return [
      h(
        "div",
        { class: "stg-head" },
        h(
          "div",
          {},
          h("div", { class: "stg-brand" }, icon(ICONS.logo), "spacetify"),
          h("h1", { class: "stg-title" }, t("Gallery", "Galerie")),
          h(
            "p",
            { class: "stg-sub" },
            t(
              "Themes and extensions picked for spacetify. Every file is checked before it is installed.",
              "Des thèmes et des extensions choisis pour spacetify. Chaque fichier est vérifié avant d'être installé.",
            ),
          ),
        ),
        h(
          "div",
          { class: "stg-actions" },
          h(
            "label",
            { class: "stg-search" },
            icon(ICONS.search),
            h("span", { class: "stg-hidden" }, t("Search the gallery", "Chercher dans la galerie")),
            search,
          ),
          h(
            "button",
            {
              class: "stg-secondary",
              style: "height:40px",
              title: t("Send one of your mods to the gallery", "Envoyer un de tes mods à la galerie"),
              onclick: submitMod,
            },
            icon(ICONS.plus),
            t("Submit a mod", "Proposer un mod"),
          ),
        ),
      ),
      h(
        "div",
        { class: "stg-tabs" },
        tabs.map(([id, label]) =>
          h(
            "button",
            {
              class: "stg-tab",
              "aria-pressed": state.tab === id ? "true" : "false",
              onclick: () => {
                state.tab = id;
                state.notice = null;
                render();
              },
            },
            label,
          ),
        ),
      ),
    ];
  };

  let listBox = null;

  const visibleMods = () => {
    const mods = (state.data && state.data.mods) || [];
    const query = state.query.trim().toLowerCase();
    return mods.filter((mod) => {
      if (state.tab === "themes" && !(isTheme(mod) && mod.in_gallery)) return false;
      if (state.tab === "extensions" && !(mod.kind === "extension" && mod.in_gallery)) return false;
      if (state.tab === "installed" && !mod.installed) return false;
      if (!query) return true;
      return `${mod.name} ${mod.author} ${description(mod)} ${tagsOf(mod).join(" ")}`.toLowerCase().includes(query);
    });
  };

  // Only the list, so typing in the search field keeps its focus.
  const renderList = () => {
    if (!listBox) return;
    listBox.replaceChildren();
    const data = state.data;
    if (!data) return;
    const onlineTab = state.tab !== "installed";
    if (onlineTab && data.gallery_error && !data.offline_list) {
      listBox.append(
        empty(
          ICONS.offline,
          t("The gallery cannot be reached", "Impossible de joindre la galerie"),
          t(
            "The “Installed” tab still works: you can turn mods on and off, and change theme.",
            "L'onglet « Installés » reste disponible : tu peux toujours activer, couper ou changer de thème.",
          ),
          true,
        ),
      );
      return;
    }
    const mods = visibleMods();
    if (!mods.length) {
      listBox.append(
        h(
          "p",
          { style: "margin-top:24px;color:#a1a1a6" },
          state.query
            ? t("Nothing matches this search.", "Rien ne correspond à cette recherche.")
            : state.tab === "installed"
              ? t("Nothing installed yet.", "Rien d'installé pour l'instant.")
              : t("Nothing here yet.", "Rien ici pour l'instant."),
        ),
      );
      return;
    }
    listBox.append(h("div", { class: "stg-grid" }, mods.map(card)));
  };

  let root = null;

  const render = () => {
    if (!root) return;
    const focused = document.activeElement && root.contains(document.activeElement);
    root.replaceChildren();
    listBox = null;
    root.append(
      h(
        "div",
        { class: "stg-topbar" },
        h("button", { class: "stg-leave", onclick: close }, icon(ICONS.back), t("Back to Spotify", "Retour à Spotify")),
        h(
          "button",
          { class: "stg-close", "aria-label": t("Close the gallery", "Fermer la galerie"), onclick: close },
          icon(ICONS.close),
        ),
      ),
    );
    const scroll = h("div", { class: "stg-scroll", onscroll: lookSoon });
    root.append(scroll);

    if (state.serviceDown) {
      scroll.append(
        empty(
          ICONS.power,
          t("The gallery needs spacetify", "La galerie a besoin de spacetify"),
          t(
            "spacetify's background service downloads and installs the mods. Open the spacetify app: it starts it again. Your installed themes and extensions still work.",
            "C'est spacetify, en arrière-plan, qui télécharge et installe les mods. Ouvre l'appli spacetify : elle le remet en route. Tes thèmes et extensions déjà installés marchent toujours.",
          ),
          true,
        ),
      );
      return;
    }
    if (!state.data) {
      scroll.append(h("p", { style: "color:#a1a1a6" }, t("Loading the gallery…", "Chargement de la galerie…")));
      return;
    }
    const mods = state.data.mods || [];
    const shown = state.detail && mods.find((mod) => mod.id === state.detail);
    if (shown) {
      scroll.append(detailView(shown));
    } else {
      state.detail = null;
      scroll.append(...header(mods));
      if (state.notice && state.notice.text) {
        scroll.append(
          h(
            "div",
            { class: `stg-notice stg-${state.notice.tone}`, role: "status" },
            state.notice.tone === "good" ? icon(ICONS.check) : null,
            state.notice.text,
          ),
        );
      } else if (state.data.offline_list && state.tab !== "installed") {
        scroll.append(
          h(
            "div",
            { class: "stg-notice stg-warn", role: "status" },
            t(
              "GitHub cannot be reached right now: this is the last list seen.",
              "GitHub ne répond pas pour l'instant : voici la dernière liste connue.",
            ),
          ),
        );
      }
      listBox = h("div");
      scroll.append(listBox);
      renderList();
    }

    if (state.restartNeeded && !state.restarting) {
      root.append(
        h(
          "div",
          { class: "stg-pending", role: "status" },
          icon(ICONS.restart),
          h(
            "div",
            { class: "stg-pending-text" },
            h(
              "span",
              { style: "font-weight:700" },
              t(
                "Your extensions change the next time Spotify starts",
                "Tes extensions changent au prochain démarrage de Spotify",
              ),
            ),
            h(
              "span",
              { style: "color:#a1a1a6;font-size:13px" },
              t(
                "Spotify closes and opens again by itself, in a few seconds.",
                "Spotify se ferme et se rouvre tout seul, en quelques secondes.",
              ),
            ),
          ),
          h(
            "button",
            {
              class: "stg-quiet",
              onclick: () => {
                state.restartNeeded = false;
                state.notice = {
                  tone: "good",
                  text: t(
                    "It will be done the next time you close Spotify.",
                    "Ce sera fait la prochaine fois que tu fermeras Spotify.",
                  ),
                };
                render();
              },
            },
            t("Later", "Plus tard"),
          ),
          h("button", { class: "stg-primary", style: "height:40px", onclick: restart }, t("Restart Spotify", "Relancer Spotify")),
        ),
      );
    }
    if (state.lightbox && shown) {
      const hide = () => {
        state.lightbox = null;
        render();
      };
      root.append(
        h(
          "div",
          { class: "stg-lightbox", role: "dialog", "aria-label": t("Picture", "Image"), onclick: hide },
          picture(state.lightbox.mod, state.lightbox.name, false),
        ),
      );
    } else {
      state.lightbox = null;
    }
    if (state.restarting) {
      root.append(
        h(
          "div",
          { class: "stg-cover" },
          h(
            "div",
            { class: "stg-dialog", role: "status" },
            h("span", { style: "font-size:18px;font-weight:700" }, t("Spotify is restarting…", "Spotify redémarre…")),
            h(
              "span",
              { style: "color:#a1a1a6" },
              t(
                "spacetify closes Spotify, adds your extensions and opens it again. About 5 seconds.",
                "spacetify ferme Spotify, ajoute tes extensions et le rouvre. Environ 5 secondes.",
              ),
            ),
            h("div", { class: "stg-progress" }, h("span")),
          ),
        ),
      );
    }
    if (focused) {
      const again = root.querySelector(".stg-search input");
      if (again && state.query) again.focus();
    }
  };

  // ------------------------------------------------------- open / close

  // Bottom of Spotify's top bar: the row that holds the arrows and spans
  // the window. It stays visible, so the window can still be moved,
  // minimised or closed.
  const topBarBottom = () => {
    let el = document.querySelector(FORWARD);
    while (el && el !== document.body) {
      const rect = el.getBoundingClientRect();
      if (rect.width > window.innerWidth * 0.9 && rect.height >= 40 && rect.height < 120) {
        return rect.bottom;
      }
      el = el.parentElement;
    }
    return 64;
  };

  // Over all of Spotify but its top bar, so the gallery shows large.
  const place = () => {
    if (!root) return;
    const top = Math.max(0, Math.min(topBarBottom(), 120));
    Object.assign(root.style, {
      top: `${top}px`,
      left: "8px",
      width: `${window.innerWidth - 16}px`,
      height: `${window.innerHeight - top - 8}px`,
    });
    lookSoon();
  };

  const onKey = (event) => {
    if (event.key !== "Escape" || !state.open || state.restarting) return;
    if (state.lightbox) {
      state.lightbox = null;
      render();
    } else close();
  };

  let openedAt = "";

  function open() {
    if (state.open) return;
    state.open = true;
    openedAt = location.pathname;
    root = h("div", { id: ROOT_ID, role: "dialog", "aria-label": t("spacetify gallery", "Galerie spacetify") });
    document.body.appendChild(root);
    place();
    window.addEventListener("resize", place);
    document.addEventListener("keydown", onKey);
    button.setAttribute("aria-pressed", "true");
    render();
    load(false);
  }

  function close() {
    if (!state.open) return;
    state.open = false;
    state.detail = null;
    state.lightbox = null;
    window.removeEventListener("resize", place);
    document.removeEventListener("keydown", onKey);
    if (root) root.remove();
    root = null;
    listBox = null;
    button.setAttribute("aria-pressed", "false");
  }

  // ------------------------------------------------------------ button

  const style = document.createElement("style");
  style.id = "spacetify-gallery-style";
  style.textContent = STYLE;
  document.head.appendChild(style);

  // Just the logo. Its label is set when it is pointed at, since Spotify
  // sets its language after this runs.
  const label = () => {
    const text = t("spacetify gallery", "Galerie spacetify");
    button.title = text;
    button.setAttribute("aria-label", text);
  };
  const button = h(
    "button",
    {
      id: BUTTON_ID,
      type: "button",
      "aria-pressed": "false",
      onmouseenter: () => label(),
      onfocus: () => label(),
      onclick: () => (state.open ? close() : open()),
    },
    icon(ICONS.logo),
  );
  label();
  const started = Date.now();

  // Right after the back and forward arrows when they are there, floating
  // otherwise. Spotify redraws its top bar now and then, so this runs again
  // on changes.
  // Puts the logo's middle exactly on the arrows' middle, whatever the
  // top bar's own alignment.
  const align = () => {
    const anchor = document.querySelector(FORWARD);
    if (!anchor || !button.isConnected || button.classList.contains("stg-floating")) return;
    const current = parseFloat(button.style.top) || 0;
    const a = anchor.getBoundingClientRect();
    const b = button.getBoundingClientRect();
    if (!a.height || !b.height) return;
    const shift = current + (a.top + a.height / 2) - (b.top + b.height / 2);
    if (Math.abs(shift - current) >= 0.5) button.style.top = `${Math.round(shift * 2) / 2}px`;
  };

  const placeButton = () => {
    const anchor = document.querySelector(FORWARD);
    if (anchor && anchor.parentElement) {
      if (anchor.nextSibling !== button) anchor.after(button);
      button.classList.remove("stg-floating");
      requestAnimationFrame(safe(align));
    } else if (!button.isConnected && Date.now() - started > FLOAT_AFTER_MS) {
      button.classList.add("stg-floating");
      document.body.appendChild(button);
    }
  };

  let queued = false;
  const watch = safe(() => {
    queued = false;
    if (!button.isConnected || button.classList.contains("stg-floating")) placeButton();
    else align();
    if (state.open && location.pathname !== openedAt) close();
    else if (state.open) place();
  });
  new MutationObserver(() => {
    if (queued) return;
    queued = true;
    setTimeout(watch, 250);
  }).observe(document.body, { childList: true, subtree: true });
  setTimeout(watch, FLOAT_AFTER_MS + 100);
  window.addEventListener("resize", safe(align));
  placeButton();
  log("info", ["ready"]);
})
