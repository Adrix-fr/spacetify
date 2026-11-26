// spacetify runtime: loads the active theme and extensions, and gives each
// extension its own `spacetify` API.
//
// Every call into an extension goes through a try/catch, so a broken
// extension only logs errors: it never stops Spotify or the other
// extensions. One that keeps failing is stopped.
//
// spacetify replaces the CONFIG placeholder below when it builds the payload,
// and the gallery one with assets/gallery.js.
(() => {
  "use strict";
  const CONFIG = /*@spacetify-config*/ null;
  const startGallery = /*@spacetify-gallery*/ null;
  if (!CONFIG || window.__spacetify) return;

  const MAX_LOGS = 300;
  const MAX_ERRORS = 25;
  const SCAN_DELAY_MS = 100;
  const DEFAULT_TIMEOUT_MS = 30000;
  const EXTENSION_PATH = /spacetify\/extensions\/([a-z0-9-]+)\.js/;

  const logs = [];
  const extensions = new Map();
  let subscriptions = [];
  let lastPath = location.pathname;
  let scanQueued = false;

  // ---------------------------------------------------------------- logging

  const format = (value) => {
    if (value instanceof Error) return value.stack || String(value);
    if (typeof value === "string") return value;
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  };

  const write = (level, source, args) => {
    logs.push({ time: Date.now(), level, source, message: args.map(format).join(" ") });
    if (logs.length > MAX_LOGS) logs.shift();
    const print = level === "error" ? console.error : level === "warn" ? console.warn : console.log;
    const tag = source ? `[spacetify:${source}]` : "[spacetify]";
    print.call(console, `%c${tag}`, "color:#ff3d7f;font-weight:bold", ...args);
  };

  // ------------------------------------------------------------- isolation

  const fail = (state, error, where) => {
    state.errors += 1;
    write("error", state.id, [`${where}:`, error]);
    if (state.errors >= MAX_ERRORS && state.status !== "stopped") {
      stop(state, `stopped after ${MAX_ERRORS} errors`);
    }
  };

  const guard = (state, where, fn) =>
    function guarded(...args) {
      if (state.status === "stopped") return undefined;
      try {
        const result = fn.apply(this, args);
        if (result && typeof result.then === "function") {
          result.then(undefined, (error) => fail(state, error, where));
        }
        return result;
      } catch (error) {
        fail(state, error, where);
        return undefined;
      }
    };

  const stop = (state, reason) => {
    state.status = "stopped";
    subscriptions = subscriptions.filter((sub) => sub.state !== state);
    for (const cleanup of state.cleanups.splice(0)) {
      try {
        cleanup();
      } catch {
        // Nothing more to do for a stopped extension.
      }
    }
    write("warn", state.id, [reason]);
  };

  const fromStack = (text) => {
    const match = EXTENSION_PATH.exec(String(text || ""));
    return match ? extensions.get(match[1]) : undefined;
  };

  window.addEventListener(
    "error",
    (event) => {
      const target = event.target;
      if (target && target !== window && target.src) {
        const state = fromStack(target.src);
        if (state) {
          state.status = "failed";
          write("error", state.id, ["could not load", target.src]);
        }
        return;
      }
      const state = fromStack(event.filename) || fromStack(event.error && event.error.stack);
      if (state) {
        if (state.status === "loading") state.status = "failed";
        fail(state, event.error || event.message, "uncaught error");
      }
    },
    true,
  );

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const state =
      (reason && extensions.get(reason.spacetifyExtension)) || fromStack(reason && reason.stack);
    if (state) fail(state, reason, "unhandled promise rejection");
  });

  // ------------------------------------------------------- watching the UI

  const onReady = (fn) => {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", fn, { once: true });
    } else {
      fn();
    }
  };

  const queueScan = () => {
    if (scanQueued) return;
    scanQueued = true;
    setTimeout(scan, SCAN_DELAY_MS);
  };

  const queryAll = (sub) => {
    try {
      return document.querySelectorAll(sub.selector);
    } catch (error) {
      remove(sub);
      fail(sub.state, error, `selector ${sub.selector}`);
      return [];
    }
  };

  function scan() {
    scanQueued = false;
    for (const sub of subscriptions.slice()) {
      if (sub.kind === "observe") {
        for (const element of queryAll(sub)) {
          if (sub.seen.has(element)) continue;
          sub.seen.add(element);
          sub.callback(element);
        }
      } else if (sub.kind === "wait") {
        const element = queryAll(sub)[0];
        if (element) {
          remove(sub);
          clearTimeout(sub.timer);
          sub.resolve(element);
        }
      }
    }
    if (location.pathname !== lastPath) {
      lastPath = location.pathname;
      for (const sub of subscriptions.slice()) {
        if (sub.kind === "navigate") sub.callback(lastPath);
      }
    }
  }

  const remove = (sub) => {
    subscriptions = subscriptions.filter((other) => other !== sub);
  };

  const subscribe = (sub) => {
    subscriptions.push(sub);
    queueScan();
    return () => remove(sub);
  };

  const checkSelector = (selector) => {
    if (typeof selector !== "string" || !selector.trim()) {
      throw new TypeError("a CSS selector string is required");
    }
    // Throws a SyntaxError right away for an invalid selector.
    document.createDocumentFragment().querySelector(selector);
  };

  const checkFunction = (fn) => {
    if (typeof fn !== "function") throw new TypeError("a callback function is required");
  };

  onReady(() => {
    new MutationObserver(queueScan).observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
    window.addEventListener("popstate", queueScan);
    queueScan();
  });

  // --------------------------------------------------------------- styles

  const addStyleElement = (css, attributes) => {
    const style = document.createElement("style");
    for (const [name, value] of Object.entries(attributes)) style.setAttribute(name, value);
    style.textContent = String(css);
    // At the end of <body>, like the file method, so it comes after Spotify's own styles.
    onReady(() => {
      if (!style.dataset.spacetifyRemoved) document.body.appendChild(style);
    });
    return style;
  };

  // -------------------------------------------------------------- settings

  const isValidSetting = (definition, value) => {
    switch (definition.type) {
      case "boolean":
        return typeof value === "boolean";
      case "number":
        return typeof value === "number" && Number.isFinite(value);
      case "string":
        return typeof value === "string";
      case "choice":
        return definition.choices.includes(value);
      default:
        return false;
    }
  };

  const createSettings = (state, definitions, configured) => {
    const storageKey = `spacetify:settings:${state.id}`;
    const configuredKey = `spacetify:configured:${state.id}`;
    const listeners = new Set();

    const load = () => {
      try {
        const stored = JSON.parse(localStorage.getItem(storageKey) || "{}");
        return stored && typeof stored === "object" ? stored : {};
      } catch {
        return {};
      }
    };
    const definition = (name) => {
      if (!Object.prototype.hasOwnProperty.call(definitions, name)) {
        throw new Error(`unknown setting "${name}" (declare it in extension.toml)`);
      }
      return definitions[name];
    };
    const save = (values) => localStorage.setItem(storageKey, JSON.stringify(values));
    const notify = (name, value) => {
      for (const listener of listeners) listener(name, value);
    };

    // Values chosen in the spacetify app win over those saved in Spotify,
    // once: only when they changed since the last start. A value the app
    // stopped setting goes back to the default.
    try {
      const chosen = configured && typeof configured === "object" ? configured : {};
      const snapshot = JSON.stringify(chosen);
      const previous = localStorage.getItem(configuredKey);
      if (previous !== snapshot) {
        let before = {};
        try {
          before = JSON.parse(previous || "{}") || {};
        } catch {
          before = {};
        }
        const values = load();
        for (const name of Object.keys(definitions)) {
          if (Object.prototype.hasOwnProperty.call(chosen, name)) {
            values[name] = chosen[name];
          } else if (Object.prototype.hasOwnProperty.call(before, name)) {
            delete values[name];
          }
        }
        save(values);
        localStorage.setItem(configuredKey, snapshot);
      }
    } catch {
      // Storage unavailable: the defaults and saved values still work.
    }

    const get = (name) => {
      const def = definition(name);
      const stored = load()[name];
      return isValidSetting(def, stored) ? stored : def.default;
    };

    return Object.freeze({
      get,
      set(name, value) {
        if (!isValidSetting(definition(name), value)) {
          throw new TypeError(`invalid value for setting "${name}"`);
        }
        const values = load();
        values[name] = value;
        save(values);
        notify(name, value);
      },
      reset(name) {
        const values = load();
        const names = name === undefined ? Object.keys(definitions) : [name];
        for (const key of names) {
          definition(key);
          delete values[key];
        }
        save(values);
        for (const key of names) notify(key, definitions[key].default);
      },
      all() {
        const values = {};
        for (const name of Object.keys(definitions)) values[name] = get(name);
        return values;
      },
      onChange(callback) {
        checkFunction(callback);
        const listener = guard(state, "settings.onChange callback", callback);
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      definitions,
    });
  };

  // ------------------------------------------------------------------ API

  const createApi = (state, meta) => {
    const log = (level) => (...args) => write(level, state.id, args);

    return Object.freeze({
      version: CONFIG.version,
      mode: CONFIG.mode,
      extension: Object.freeze({ id: meta.id, name: meta.name, version: meta.version }),
      log: Object.freeze({ info: log("info"), warn: log("warn"), error: log("error") }),

      waitFor(selector, options = {}) {
        checkSelector(selector);
        const timeout = options.timeout === undefined ? DEFAULT_TIMEOUT_MS : options.timeout;
        return new Promise((resolve, reject) => {
          const found = document.querySelector(selector);
          if (found) {
            resolve(found);
            return;
          }
          const sub = { state, kind: "wait", selector, resolve };
          if (timeout > 0) {
            sub.timer = setTimeout(() => {
              remove(sub);
              const error = new Error(`waitFor: nothing matched ${selector} after ${timeout} ms`);
              error.spacetifyExtension = state.id;
              reject(error);
            }, timeout);
          }
          subscribe(sub);
        });
      },

      observe(selector, callback) {
        checkSelector(selector);
        checkFunction(callback);
        return subscribe({
          state,
          kind: "observe",
          selector,
          seen: new WeakSet(),
          callback: guard(state, `observe ${selector}`, callback),
        });
      },

      onNavigate(callback) {
        checkFunction(callback);
        return subscribe({
          state,
          kind: "navigate",
          callback: guard(state, "onNavigate callback", callback),
        });
      },

      addStyle(css) {
        const style = addStyleElement(css, { "data-spacetify-extension": state.id });
        const removeStyle = () => {
          style.dataset.spacetifyRemoved = "true";
          style.remove();
        };
        state.cleanups.push(removeStyle);
        return removeStyle;
      },

      settings: createSettings(state, meta.settings || {}, meta.configured || {}),
    });
  };

  // ------------------------------------------------------------ extensions

  // Every enabled extension is known from the start, so one that cannot load
  // (a syntax error, a missing file) still shows up, as failed.
  for (const meta of CONFIG.extensions) {
    extensions.set(meta.id, {
      id: meta.id,
      name: meta.name,
      version: meta.version,
      status: "loading",
      errors: 0,
      cleanups: [],
      started: false,
    });
  }

  const notStarted = (state) => {
    if (state.started || state.status !== "loading") return;
    state.status = "failed";
    write("error", state.id, ["did not start: main.js could not run (look for a syntax error)"]);
  };

  const define = (meta, factory) => {
    const state = meta && extensions.get(meta.id);
    if (!state || state.started || typeof factory !== "function") return;
    state.started = true;
    onReady(() => {
      try {
        const api = createApi(state, meta);
        state.status = "running";
        const result = factory.call(undefined, api);
        if (result && typeof result.then === "function") {
          result.then(undefined, (error) => fail(state, error, "start"));
        }
      } catch (error) {
        state.status = "failed";
        fail(state, error, "start");
      }
    });
  };

  window.__spacetify = Object.freeze({
    version: CONFIG.version,
    mode: CONFIG.mode,
    theme: CONFIG.theme,
    define,
    logs: () => logs.slice(),
    extensions: () =>
      [...extensions.values()].map(({ id, name, version, status, errors }) => ({
        id,
        name,
        version,
        status,
        errors,
      })),
  });

  if (CONFIG.mode === "live" && CONFIG.css) {
    addStyleElement(CONFIG.css, { id: "spacetify-theme" });
  }
  if (CONFIG.mode === "file") {
    for (const state of extensions.values()) {
      const script = document.createElement("script");
      script.src = `/spacetify/extensions/${state.id}.js`;
      script.async = false;
      // Runs after the file has executed, even if it threw.
      script.addEventListener("load", () => notStarted(state));
      (document.head || document.documentElement).appendChild(script);
    }
  } else {
    // Live scripts are evaluated right after this one.
    onReady(() =>
      setTimeout(() => {
        for (const state of extensions.values()) notStarted(state);
      }, 1000),
    );
  }

  // The gallery page, which talks to the background service.
  if (CONFIG.gallery && typeof startGallery === "function") {
    onReady(() => {
      try {
        startGallery(CONFIG.gallery, (level, args) => write(level, "gallery", args));
      } catch (error) {
        write("error", "gallery", [error]);
      }
    });
  }

  const theme = CONFIG.theme ? `${CONFIG.theme.name} ${CONFIG.theme.version}` : "none";
  write("info", null, [
    `v${CONFIG.version} (${CONFIG.mode}) | theme: ${theme} | extensions: ${CONFIG.extensions.length}`,
  ]);
})();
