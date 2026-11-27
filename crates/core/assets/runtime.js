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
  let lastPath = null;
  let scanQueued = false;
  // Run on every scan, to put spacetify's own elements back in place.
  const placers = new Set();

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

  // ------------------------------------------------- Spotify's own services
  //
  // Spotify hands its services (player, queue, volume, navigation…) to its
  // interface through React. They are found by what they can do, never by
  // the generated names in Spotify's code, and nothing of Spotify is
  // rewritten. Extensions never touch them: they use spacetify's API below,
  // so if Spotify moves something, only this part has to follow.

  const INTERNALS_TIMEOUT_MS = 30000;
  const FIBER_STARTS = [
    '[data-testid="now-playing-bar"]',
    ".main-view-container",
    '[data-testid="top-bar-back-button"]',
  ];
  const internals = { platform: null, services: new Map(), ready: false, waiting: [] };

  const fiberOf = (element) => {
    const key = Object.keys(element).find((name) => name.startsWith("__reactFiber$"));
    return key ? element[key] : null;
  };

  // The object Spotify gives to its whole interface: it has getHistory and getRegistry.
  const findPlatform = () => {
    if (internals.platform) return internals.platform;
    for (const selector of FIBER_STARTS) {
      const element = document.querySelector(selector);
      if (!element) continue;
      for (let fiber = fiberOf(element), depth = 0; fiber && depth < 3000; fiber = fiber.return, depth++) {
        const value = fiber.type && fiber.type._context && fiber.memoizedProps && fiber.memoizedProps.value;
        if (
          value &&
          typeof value === "object" &&
          typeof value.getHistory === "function" &&
          typeof value.getRegistry === "function"
        ) {
          internals.platform = value;
          return value;
        }
      }
    }
    return null;
  };

  // A service by its name, such as "PlayerAPI": from the platform itself in
  // older versions, from its registry in newer ones.
  const service = (name) => {
    if (internals.services.has(name)) return internals.services.get(name);
    const platform = findPlatform();
    if (!platform) return null;
    let found = null;
    try {
      const getter = platform[`get${name}`];
      if (typeof getter === "function") found = getter.call(platform);
      if (!found) {
        const registry = platform.getRegistry();
        const map = registry && registry._map;
        if (map && typeof map.keys === "function") {
          for (const key of map.keys()) {
            const label = typeof key === "symbol" ? key.description : String(key);
            if (label === name) {
              found = registry.resolve(key);
              break;
            }
          }
        }
      }
    } catch (error) {
      write("warn", null, [`Spotify's ${name} could not be reached:`, error]);
    }
    if (found) internals.services.set(name, found);
    return found;
  };

  const spotifyHistory = () => {
    const platform = findPlatform();
    try {
      return platform ? platform.getHistory() : null;
    } catch {
      return null;
    }
  };

  // The page Spotify shows, such as "/album/1XslIirSxfAhhxRdn4Li9t". The
  // desktop app keeps its pages in memory: location.pathname stays /index.html.
  const currentPath = () => {
    const history = spotifyHistory();
    const path = history && history.location && history.location.pathname;
    return typeof path === "string" ? path : location.pathname;
  };

  // Calls fn once Spotify's services are found (right away if they are).
  const whenInternals = (fn) => {
    if (internals.ready) fn();
    else internals.waiting.push(fn);
  };

  const lookForInternals = () => {
    if (internals.ready || !findPlatform()) return;
    internals.ready = true;
    lastPath = currentPath();
    const history = spotifyHistory();
    if (history && typeof history.listen === "function") {
      try {
        history.listen(() => queueScan());
      } catch {
        // The scan still notices new pages, a little later.
      }
    }
    for (const fn of internals.waiting.splice(0)) {
      try {
        fn();
      } catch (error) {
        write("error", null, [error]);
      }
    }
  };

  setTimeout(() => {
    if (!internals.ready) {
      write("warn", null, [
        "Spotify's player and navigation were not found: spacetify.player works with what is on screen (no queue, no playUri)",
      ]);
    }
  }, INTERNALS_TIMEOUT_MS);

  const unavailable = (what) =>
    new Error(`${what} is not available in this version of Spotify (spacetify.player.available is false)`);

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
    lookForInternals();
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
    const path = currentPath();
    if (lastPath === null) {
      lastPath = path;
    } else if (path !== lastPath) {
      lastPath = path;
      for (const sub of subscriptions.slice()) {
        if (sub.kind === "navigate") sub.callback(path);
      }
    }
    for (const place of placers) {
      try {
        place();
      } catch (error) {
        write("error", null, [error]);
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

  // ------------------------------------------------------- shared helpers

  const isUri = (value) => typeof value === "string" && /^spotify:[a-z]+:/.test(value);

  const checkUris = (value) => {
    const uris = Array.isArray(value) ? value : [value];
    if (!uris.length || !uris.every(isUri)) {
      throw new TypeError('a Spotify URI ("spotify:track:…") or a list of them is required');
    }
    return uris;
  };

  const checkOptions = (options) => {
    if (!options || typeof options !== "object") throw new TypeError("an options object is required");
    return options;
  };

  // A listener of an extension, removed when it is stopped.
  const listenWith = (state, listeners, where, callback) => {
    checkFunction(callback);
    const listener = guard(state, where, callback);
    listeners.add(listener);
    const stopListening = () => listeners.delete(listener);
    state.cleanups.push(stopListening);
    return stopListening;
  };

  const emit = (listeners, ...args) => {
    for (const listener of [...listeners]) listener(...args);
  };

  // A promise rejected by the API carries the extension's id, so an
  // unhandled one is logged under its name.
  const tagged = (state, fn) =>
    async function api(...args) {
      try {
        return await fn(...args);
      } catch (error) {
        if (error && typeof error === "object") {
          try {
            error.spacetifyExtension = state.id;
          } catch {
            // A frozen error is still rejected.
          }
        }
        throw error;
      }
    };

  // ---------------------------------------------------------------- player

  const REPEAT = ["off", "context", "track"];
  const PLAY_TRACKS_MAX = 10000;
  const IMAGE_SIZES = ["xlarge", "large", "standard", "default", "small"];
  const BUTTONS = {
    playpause: '[data-testid="control-button-playpause"]',
    next: '[data-testid="control-button-skip-forward"]',
    previous: '[data-testid="control-button-skip-back"]',
  };

  const playerApi = () => service("PlayerAPI");
  const playbackApi = () => service("PlaybackAPI");

  const bestImage = (images) => {
    if (!Array.isArray(images) || !images.length) return null;
    for (const size of IMAGE_SIZES) {
      const image = images.find((candidate) => candidate && candidate.label === size && candidate.url);
      if (image) return image.url;
    }
    return images[0] && images[0].url ? images[0].url : null;
  };

  const trackOf = (item) => {
    if (!item || !item.uri) return null;
    return Object.freeze({
      uri: item.uri,
      name: item.name || "",
      type: item.type || item.uri.split(":")[1],
      duration: (item.duration && item.duration.milliseconds) || 0,
      artists: Object.freeze(
        (item.artists || []).map((artist) => Object.freeze({ uri: artist.uri, name: artist.name })),
      ),
      album: item.album
        ? Object.freeze({ uri: item.album.uri, name: item.album.name || "" })
        : null,
      image: bestImage(item.images) || bestImage(item.album && item.album.images),
      isLocal: !!item.isLocal,
      isExplicit: !!item.isExplicit,
    });
  };

  const snapshot = () => {
    const api = playerApi();
    const raw = api && api.getState();
    if (!raw) return null;
    const track = trackOf(raw.item);
    const duration = raw.duration || (track ? track.duration : 0);
    let position = raw.positionAsOfTimestamp || 0;
    if (!raw.isPaused && !raw.isBuffering && raw.timestamp) {
      position += (Date.now() - raw.timestamp) * (raw.speed || 1);
    }
    return Object.freeze({
      track,
      context: raw.context && raw.context.uri ? Object.freeze({ uri: raw.context.uri }) : null,
      isPlaying: !!track && !raw.isPaused,
      isPaused: !!raw.isPaused,
      position: Math.max(0, Math.min(Math.round(position), duration || Infinity)),
      duration,
      shuffle: !!raw.shuffle,
      repeat: REPEAT[raw.repeat] || "off",
      speed: raw.speed || 1,
    });
  };

  const changeListeners = new Set();
  const trackListeners = new Set();
  const volumeListeners = new Set();
  let lastTrackUri;

  whenInternals(() => {
    const player = playerApi();
    const events = player && player.getEvents && player.getEvents();
    if (events && typeof events.addListener === "function") {
      lastTrackUri = (snapshot() || {}).track?.uri;
      events.addListener("update", () => {
        const now = snapshot();
        emit(changeListeners, now);
        const uri = now && now.track ? now.track.uri : undefined;
        if (uri !== lastTrackUri) {
          lastTrackUri = uri;
          emit(trackListeners, now ? now.track : null);
        }
      });
    }
    const playback = playbackApi();
    const volumeEvents = playback && playback.getEvents && playback.getEvents();
    if (volumeEvents && typeof volumeEvents.addListener === "function") {
      volumeEvents.addListener("volume", () => {
        Promise.resolve(playback.getVolume()).then(
          (volume) => emit(volumeListeners, volume),
          () => {},
        );
      });
    }
  });

  // ------------------------------------------- plan B: the player on screen
  //
  // If Spotify ever hides its services, the player still works through what
  // is on screen: its buttons and bars, found by their data-testid. Less
  // can be done (no queue, no playing a URI), but nothing breaks.

  const SCREEN = {
    bar: '[data-testid="now-playing-bar"]',
    controls: '[data-testid="player-controls"]',
    repeat: '[data-testid="control-button-repeat"]',
    position: '[data-testid="playback-position"]',
    duration: '[data-testid="playback-duration"]',
    progress: '[data-testid="playback-progressbar"] [data-testid="progress-bar"]',
    progressInput: '[data-testid="playback-progressbar"] input[type="range"]',
    volume: '[data-testid="volume-bar"] [data-testid="progress-bar"]',
    volumeInput: '[data-testid="volume-bar"] input[type="range"]',
    title: '[data-testid="now-playing-bar"] [data-testid="context-item-link"]',
    artists: '[data-testid="now-playing-bar"] a[href*="/artist/"]',
    cover: '[data-testid="now-playing-bar"] [data-testid="cover-art-image"]',
  };
  const MOVING_MS = 1600;
  const screen = { positionText: null, movedAt: 0, signature: null, trackKey: null, volume: null };

  const press = (what) => {
    const button = document.querySelector(BUTTONS[what]);
    if (!button) throw unavailable(`the ${what} button`);
    button.click();
  };

  // A click at `fraction` of a bar's width, as a person would do.
  const pressBar = (selector, fraction) => {
    const bar = document.querySelector(selector);
    const rect = bar && bar.getBoundingClientRect();
    if (!rect || !rect.width) throw unavailable(`the bar ${selector}`);
    const x = rect.left + rect.width * Math.max(0, Math.min(1, fraction));
    const init = {
      bubbles: true,
      cancelable: true,
      clientX: x,
      clientY: rect.top + rect.height / 2,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      buttons: 1,
    };
    const up = { ...init, buttons: 0 };
    bar.dispatchEvent(new PointerEvent("pointerdown", init));
    bar.dispatchEvent(new MouseEvent("mousedown", init));
    document.dispatchEvent(new PointerEvent("pointermove", init));
    bar.dispatchEvent(new PointerEvent("pointerup", up));
    bar.dispatchEvent(new MouseEvent("mouseup", up));
    document.dispatchEvent(new PointerEvent("pointerup", up));
    bar.dispatchEvent(new MouseEvent("click", up));
  };

  // "3:47", "-3:46" or "1:02:03" in milliseconds.
  const clock = (text) => {
    const parts = String(text || "")
      .replace(/[^0-9:]/g, "")
      .split(":")
      .filter(Boolean)
      .map(Number);
    return parts.length ? parts.reduce((total, part) => total * 60 + part, 0) * 1000 : null;
  };

  // "/intl-fr/album/ID" → "spotify:album:ID".
  const uriOfLink = (href) => {
    const match = /\/(album|artist|playlist|track|show|episode)\/([A-Za-z0-9]+)/.exec(String(href || ""));
    return match ? `spotify:${match[1]}:${match[2]}` : null;
  };

  const nearestUri = (element) => {
    for (let fiber = fiberOf(element), depth = 0; fiber && depth < 40; fiber = fiber.return, depth++) {
      const props = fiber.memoizedProps;
      if (props && isUri(props.uri)) return props.uri;
    }
    return null;
  };

  // The shuffle button has no data-testid: it comes just before "previous".
  const shuffleButton = () => {
    const controls = document.querySelector(SCREEN.controls);
    const buttons = controls ? [...controls.querySelectorAll("button")] : [];
    const previous = buttons.findIndex((button) => button.matches(BUTTONS.previous));
    return previous > 0 ? buttons[previous - 1] : null;
  };

  // On, the shuffle button gets a dot under it and Spotify's readable
  // "bright accent" class (its colour depends on the theme, so not that).
  const shuffleOn = () => {
    const shuffle = shuffleButton();
    if (!shuffle) return false;
    const dot = getComputedStyle(shuffle, "::after").content;
    return (
      (dot !== "none" && dot !== "normal" && dot !== "") ||
      /\bencore-internal-color-text-bright-accent\b/.test(shuffle.className)
    );
  };

  const repeatMode = () => {
    const checked = document.querySelector(SCREEN.repeat)?.getAttribute("aria-checked");
    return checked === "true" ? "context" : checked === "mixed" ? "track" : "off";
  };

  const numberOf = (selector, attribute) => {
    const value = Number(document.querySelector(selector)?.[attribute]);
    return Number.isFinite(value) ? value : null;
  };

  const screenSnapshot = () => {
    if (!document.querySelector(SCREEN.bar)) return null;
    const title = document.querySelector(SCREEN.title);
    let position = numberOf(SCREEN.progressInput, "value");
    if (position === null) position = clock(document.querySelector(SCREEN.position)?.textContent) || 0;
    let duration = numberOf(SCREEN.progressInput, "max");
    if (!duration) {
      const text = document.querySelector(SCREEN.duration)?.textContent || "";
      const shown = clock(text) || 0;
      duration = text.trim().startsWith("-") ? position + shown : shown;
    }
    let track = null;
    if (title) {
      const uri = nearestUri(title) || "";
      const album = uriOfLink(title.getAttribute("href"));
      track = Object.freeze({
        uri,
        name: title.textContent.trim(),
        type: uri ? uri.split(":")[1] : "track",
        duration,
        artists: Object.freeze(
          [...document.querySelectorAll(SCREEN.artists)].map((link) =>
            Object.freeze({ uri: uriOfLink(link.getAttribute("href")), name: link.textContent.trim() }),
          ),
        ),
        album: album && album.startsWith("spotify:album:") ? Object.freeze({ uri: album, name: "" }) : null,
        image: document.querySelector(SCREEN.cover)?.getAttribute("src") || null,
        isLocal: false,
        isExplicit: false,
      });
    }
    const playing = !!track && Date.now() - screen.movedAt < MOVING_MS;
    return Object.freeze({
      track,
      context: null,
      isPlaying: playing,
      isPaused: !playing,
      position: Math.round(position),
      duration,
      shuffle: shuffleOn(),
      repeat: repeatMode(),
      speed: 1,
    });
  };

  // Plays the part of Spotify's events while its services are missing.
  placers.add(() => {
    if (playerApi()) return;
    // The time shown moves while a track plays.
    const positionText = document.querySelector(SCREEN.position)?.textContent ?? null;
    if (positionText !== screen.positionText) {
      if (screen.positionText !== null) screen.movedAt = Date.now();
      screen.positionText = positionText;
    }
    if (!(changeListeners.size || trackListeners.size || volumeListeners.size)) return;
    const now = screenSnapshot();
    const signature = now
      ? `${now.track && now.track.name}|${now.track && now.track.uri}|${now.isPaused}|${now.position}|${now.shuffle}|${now.repeat}`
      : "";
    if (signature !== screen.signature) {
      screen.signature = signature;
      emit(changeListeners, now);
    }
    const trackKey = now && now.track ? `${now.track.uri}|${now.track.name}` : "";
    if (trackKey !== screen.trackKey) {
      if (screen.trackKey !== null) emit(trackListeners, now ? now.track : null);
      screen.trackKey = trackKey;
    }
    const volume = numberOf(SCREEN.volumeInput, "value");
    if (volume !== screen.volume) {
      if (screen.volume !== null && volume !== null) emit(volumeListeners, volume);
      screen.volume = volume;
    }
  });

  // Bars and buttons redraw a moment after a click.
  const settle = () => new Promise((resolve) => setTimeout(resolve, 200));

  const playerState = () => (playerApi() ? snapshot() : screenSnapshot());

  const needPlayer = (what) => {
    const api = playerApi();
    if (!api) throw unavailable(what);
    return api;
  };

  const queueItems = (items) => (items || []).map(trackOf).filter(Boolean);

  const createPlayer = (state) => {
    const run = (fn) => tagged(state, fn);
    return Object.freeze({
      get available() {
        return !!playerApi();
      },
      ready: run(
        (timeout = INTERNALS_TIMEOUT_MS) =>
          new Promise((resolve, reject) => {
            if (internals.ready && playerApi()) {
              resolve();
              return;
            }
            // Without Spotify's services, the player on screen will do (plan B).
            const timer =
              timeout > 0
                ? setTimeout(() => {
                    if (document.querySelector(SCREEN.bar)) resolve();
                    else reject(unavailable("the player"));
                  }, timeout)
                : null;
            whenInternals(() => {
              clearTimeout(timer);
              if (playerApi()) resolve();
              else reject(unavailable("the player"));
            });
          }),
      ),
      state: () => playerState(),
      onChange: (callback) => listenWith(state, changeListeners, "player.onChange callback", callback),
      onTrackChange: (callback) =>
        listenWith(state, trackListeners, "player.onTrackChange callback", callback),
      onVolumeChange: (callback) =>
        listenWith(state, volumeListeners, "player.onVolumeChange callback", callback),

      play: run(async () => {
        const api = playerApi();
        if (api) await api.resume();
        else if (screenSnapshot()?.isPaused) press("playpause");
      }),
      pause: run(async () => {
        const api = playerApi();
        if (api) await api.pause();
        else if (screenSnapshot()?.isPlaying) press("playpause");
      }),
      togglePlay: run(async () => {
        const api = playerApi();
        if (!api) return press("playpause");
        const now = api.getState();
        await (now && now.isPaused ? api.resume() : api.pause());
      }),
      next: run(async () => {
        const api = playerApi();
        if (!api) return press("next");
        await api.skipToNext();
      }),
      previous: run(async () => {
        const api = playerApi();
        if (!api) return press("previous");
        await api.skipToPrevious();
      }),
      seek: run(async (milliseconds) => {
        if (typeof milliseconds !== "number" || !Number.isFinite(milliseconds) || milliseconds < 0) {
          throw new TypeError("seek needs a position in milliseconds");
        }
        const api = playerApi();
        if (api) {
          await api.seekTo(Math.round(milliseconds));
          return;
        }
        const duration = screenSnapshot()?.duration;
        if (!duration) throw unavailable("seek");
        pressBar(SCREEN.progress, milliseconds / duration);
        await settle();
      }),
      setShuffle: run(async (on) => {
        if (typeof on !== "boolean") throw new TypeError("setShuffle needs true or false");
        const api = playerApi();
        if (api) {
          await api.setShuffle(on);
          return;
        }
        const button = shuffleButton();
        if (!button) throw unavailable("the shuffle button");
        if (shuffleOn() !== on) button.click();
        await settle();
      }),
      setRepeat: run(async (mode) => {
        const index = REPEAT.indexOf(mode);
        if (index < 0) throw new TypeError('setRepeat needs "off", "context" or "track"');
        const api = playerApi();
        if (api) {
          await api.setRepeat(index);
          return;
        }
        const button = document.querySelector(SCREEN.repeat);
        if (!button) throw unavailable("the repeat button");
        // The button goes off → context → track → off.
        for (let i = 0; i < 3 && repeatMode() !== mode; i++) {
          button.click();
          await settle();
        }
      }),
      playUri: run(async (uri) => {
        checkUris(uri);
        if (Array.isArray(uri)) throw new TypeError("playUri plays one URI");
        await needPlayer("playUri").play({ uri }, { featureIdentifier: "spacetify" });
      }),
      // A list of tracks played in this order, like a playlist of its own.
      // Unlike the queue, which Spotify keeps to about 80 tracks, it holds
      // thousands.
      playTracks: run(async (uris) => {
        const list = checkUris(uris);
        if (list.length > PLAY_TRACKS_MAX) throw new TypeError(`playTracks plays ${PLAY_TRACKS_MAX} tracks at most`);
        await needPlayer("playTracks").play(
          { uri: "spotify:internal:spacetify", pages: [{ items: list.map((uri) => ({ uri })) }] },
          { featureIdentifier: "spacetify" },
        );
      }),
      getVolume: run(async () => {
        const api = playbackApi();
        if (api) return api.getVolume();
        const volume = numberOf(SCREEN.volumeInput, "value");
        if (volume === null) throw unavailable("the volume");
        return volume;
      }),
      setVolume: run(async (volume) => {
        if (typeof volume !== "number" || !Number.isFinite(volume)) {
          throw new TypeError("setVolume needs a number from 0 to 1");
        }
        const level = Math.max(0, Math.min(1, volume));
        const api = playbackApi();
        if (api) {
          await api.setVolume(level);
          return;
        }
        pressBar(SCREEN.volume, level);
        await settle();
      }),

      queue: run(async () => {
        const queue = await needPlayer("the queue").getQueue();
        return Object.freeze({
          current: trackOf(queue && queue.current),
          queued: Object.freeze(queueItems(queue && queue.queued)),
          next: Object.freeze(queueItems(queue && queue.nextUp)),
        });
      }),
      addToQueue: run(async (uris) => {
        const list = checkUris(uris);
        await needPlayer("the queue").addToQueue(list.map((uri) => ({ uri })));
      }),
      removeFromQueue: run(async (uris) => {
        const wanted = new Set(checkUris(uris));
        const api = needPlayer("the queue");
        const queue = await api.getQueue();
        const items = ((queue && queue.queued) || [])
          .filter((item) => wanted.has(item.uri))
          .map((item) => ({ uri: item.uri, uid: item.uid }));
        if (items.length) await api.removeFromQueue(items);
      }),
      clearQueue: run(async () => {
        await needPlayer("the queue").clearQueue();
      }),
    });
  };

  // ------------------------------------------------------------ library

  // Every track of a playlist or of Liked Songs, read page by page from the
  // services Spotify's own pages use. Tracks that cannot play here are left out.
  const LIBRARY_PAGE = 500;
  const LIBRARY_MAX = 20000;
  const isLikedSongs = (uri) =>
    /^spotify:(user:[^:]+:)?collection(:tracks)?$/.test(uri) || uri === service("LibraryLikedSongsUri");

  const readPages = async (readPage) => {
    const tracks = [];
    for (let offset = 0; offset < LIBRARY_MAX; offset += LIBRARY_PAGE) {
      const page = await readPage(offset);
      const items = (page && page.items) || [];
      for (const item of items) {
        if (item && item.isPlayable !== false && !item.isBanned) {
          const track = trackOf(item);
          if (track) tracks.push(track);
        }
      }
      const total = page && Number.isFinite(page.totalLength) ? page.totalLength : 0;
      if (!items.length || offset + items.length >= total) break;
    }
    return Object.freeze(tracks);
  };

  const createLibrary = (state) =>
    Object.freeze({
      tracks: tagged(state, async (uri) => {
        if (!isUri(uri) && !/^spotify:collection$/.test(String(uri))) {
          throw new TypeError("library.tracks needs a Spotify URI");
        }
        if (isLikedSongs(uri)) {
          const library = service("LibraryAPI");
          if (!library) throw unavailable("Liked Songs");
          return readPages((offset) => library.getTracks({ offset, limit: LIBRARY_PAGE }));
        }
        if (uri.startsWith("spotify:playlist:")) {
          const playlists = service("PlaylistAPI");
          if (!playlists) throw unavailable("the playlist");
          return readPages((offset) => playlists.getContents(uri, { offset, limit: LIBRARY_PAGE }));
        }
        throw new TypeError("library.tracks reads playlists and Liked Songs");
      }),
    });

  // ------------------------------------------------------------ navigation

  // "spotify:album:ID" → "/album/ID"; a path is kept as it is.
  const pathOf = (target) => {
    if (typeof target === "string" && target.startsWith("/")) return target;
    if (isUri(target)) {
      const parts = target.split(":").slice(1);
      return `/${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
    }
    throw new TypeError('navigate needs a path ("/album/…") or a Spotify URI ("spotify:album:…")');
  };

  const navigate = (target) => {
    const path = pathOf(target);
    const history = spotifyHistory();
    if (!history) throw unavailable("navigation");
    history.push(path);
  };

  // -------------------------------------------------------- interface: CSS

  const UI_STYLE = `
.spacetify-button{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0;width:32px;height:32px;
  padding:0;margin:0 2px;border:0;border-radius:50%;background:transparent;color:var(--text-subdued,#b3b3b3);
  cursor:pointer;align-self:center;-webkit-app-region:no-drag;transition:color .15s,transform .15s}
.spacetify-button:hover{color:var(--text-base,#fff);transform:scale(1.06)}
.spacetify-button[aria-pressed="true"]{color:var(--text-bright-accent,#1ed760)}
.spacetify-button svg{display:block;width:16px;height:16px;fill:currentColor}
.spacetify-button .spacetify-letter{font-size:13px;font-weight:700;line-height:1}
.spacetify-topbar-button{background:var(--background-elevated-base,#1f1f1f);margin:0 4px}
.spacetify-topbar-button svg{width:18px;height:18px}
[data-spacetify-menu-item]{cursor:pointer}
[data-spacetify-menu-item]:hover,[data-spacetify-menu-item]:focus-visible{background-color:var(--background-tinted-highlight,rgba(255,255,255,.1));outline:none}
[data-spacetify-menu-item].spacetify-plain{display:flex;align-items:center;gap:12px;padding:12px 8px 12px 12px;border-radius:2px;
  color:var(--text-base,#fff);font-size:14px}
[data-spacetify-menu-item] svg{width:16px;height:16px;fill:currentColor}
.spacetify-popup-overlay{position:fixed;inset:0;z-index:2147483100;display:flex;align-items:center;justify-content:center;
  background:rgba(0,0,0,.6);-webkit-app-region:no-drag}
.spacetify-popup{display:flex;flex-direction:column;width:min(var(--spacetify-popup-width,560px),calc(100vw - 48px));
  max-height:calc(100vh - 96px);border-radius:8px;background:var(--background-elevated-base,#282828);color:var(--text-base,#fff);
  box-shadow:0 16px 48px rgba(0,0,0,.6);overflow:hidden}
.spacetify-popup-head{display:flex;align-items:center;gap:16px;padding:20px 20px 12px 24px}
.spacetify-popup-head h2{flex:1;margin:0;font-size:20px;line-height:28px;font-weight:700}
.spacetify-popup-close{width:32px;height:32px;border:0;border-radius:50%;background:transparent;color:var(--text-subdued,#b3b3b3);
  font-size:22px;line-height:1;cursor:pointer}
.spacetify-popup-close:hover{color:var(--text-base,#fff);background:var(--background-elevated-highlight,#3e3e3e)}
.spacetify-popup-body{padding:4px 24px 24px;overflow:auto;font-size:14px;line-height:22px;font-weight:400;white-space:pre-wrap}
.spacetify-page{position:absolute;inset:0;z-index:5;overflow:auto;padding:24px 32px;box-sizing:border-box;
  background:var(--background-base,#121212);color:var(--text-base,#fff)}
`;

  let uiStyle = null;
  const useUiStyle = () => {
    if (!uiStyle) uiStyle = addStyleElement(UI_STYLE, { id: "spacetify-ui-style" });
  };

  const parseIcon = (markup) => {
    if (markup === undefined || markup === null || markup === "") return null;
    const template = document.createElement("template");
    template.innerHTML = String(markup).trim();
    const svg = template.content.firstElementChild;
    if (!svg || svg.tagName.toLowerCase() !== "svg" || template.content.children.length !== 1) {
      throw new TypeError('icon must be one <svg> element, like \'<svg viewBox="0 0 16 16">…</svg>\'');
    }
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    return svg;
  };

  // ------------------------------------------------ interface: buttons

  const FORWARD = '[data-testid="top-bar-forward-button"]';
  const PLAYBAR_ANCHORS = [
    '[data-testid="now-playing-bar"] [data-testid="lyrics-button"]',
    '[data-testid="now-playing-bar"] [data-testid="control-button-queue"]',
  ];
  const topbarButtons = [];
  const playbarButtons = [];

  // Right after the back and forward arrows (and the gallery's logo).
  placers.add(() => {
    if (!topbarButtons.length) return;
    const forward = document.querySelector(FORWARD);
    if (!forward || !forward.parentElement) return;
    const gallery = document.getElementById("spacetify-gallery-button");
    let previous = gallery && gallery.parentElement === forward.parentElement ? gallery : forward;
    for (const button of topbarButtons) {
      if (previous.nextSibling !== button) previous.after(button);
      previous = button;
    }
  });

  // In the player bar, just before the lyrics button.
  placers.add(() => {
    if (!playbarButtons.length) return;
    let anchor = null;
    for (const selector of PLAYBAR_ANCHORS) {
      anchor = document.querySelector(selector);
      if (anchor) break;
    }
    if (!anchor || !anchor.parentElement) return;
    let next = anchor;
    for (const button of [...playbarButtons].reverse()) {
      if (button.nextSibling !== next) next.before(button);
      next = button;
    }
  });

  const createButton = (state, kind, list, options) => {
    checkOptions(options);
    checkFunction(options.onClick);
    useUiStyle();
    const button = document.createElement("button");
    button.type = "button";
    button.className = `spacetify-button spacetify-${kind}-button`;
    button.dataset.spacetifyExtension = state.id;
    const setLabel = (text) => {
      const label = String(text || state.name);
      button.title = label;
      button.setAttribute("aria-label", label);
    };
    setLabel(options.label);
    const svg = parseIcon(options.icon);
    if (svg) {
      button.append(svg);
    } else {
      const letter = document.createElement("span");
      letter.className = "spacetify-letter";
      letter.textContent = String(options.label || state.name).trim().charAt(0).toUpperCase() || "?";
      button.append(letter);
    }
    button.addEventListener("click", guard(state, `${kind} button`, (event) => options.onClick(event)));
    list.push(button);
    const removeButton = () => {
      const index = list.indexOf(button);
      if (index >= 0) list.splice(index, 1);
      button.remove();
    };
    state.cleanups.push(removeButton);
    queueScan();
    return Object.freeze({
      element: button,
      setLabel,
      setActive(on) {
        button.setAttribute("aria-pressed", on ? "true" : "false");
      },
      remove: removeButton,
    });
  };

  // ---------------------------------------------- interface: right-click menu

  const MENU_ROOT = '[data-testid="context-menu"]';
  const menuItems = [];
  const seenMenus = new WeakSet();

  // What was right-clicked: the closest Spotify URI above the menu.
  const menuTarget = (menu) => {
    let uri = null;
    let uris = null;
    let contextUri = null;
    for (let fiber = fiberOf(menu), depth = 0; fiber && depth < 150 && !uri; fiber = fiber.return, depth++) {
      const props = fiber.memoizedProps;
      if (!props || typeof props !== "object") continue;
      if (!uris && Array.isArray(props.uris) && props.uris.length && props.uris.every(isUri)) {
        uris = props.uris.slice();
      }
      if (!contextUri && isUri(props.contextUri)) contextUri = props.contextUri;
      if (isUri(props.uri)) uri = props.uri;
    }
    if (!uri && uris) uri = uris[0];
    if (!uri) return null;
    if (!uris || !uris.includes(uri)) uris = [uri];
    const parts = uri.split(":");
    return Object.freeze({
      uri,
      uris: Object.freeze(uris),
      type: parts.length > 3 ? parts[parts.length - 2] : parts[1],
      contextUri,
    });
  };

  const closeMenu = (menu) => {
    menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    setTimeout(() => {
      if (menu.isConnected) {
        document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
        document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      }
    }, 50);
  };

  // A row that looks like Spotify's own: a copy of one of its rows, emptied.
  const menuRow = (menu, item) => {
    const rows = menu.querySelectorAll('[role="menuitem"]');
    const model =
      [...rows].find((row) => !row.hasAttribute("aria-haspopup") && row.tagName !== "A") || rows[0];
    let row;
    if (model) {
      row = model.cloneNode(false);
      for (const name of [
        "id",
        "data-key",
        "data-react-aria-pressable",
        "aria-keyshortcuts",
        "aria-haspopup",
        "aria-expanded",
        "data-has-submenu",
        "data-collection",
        "href",
      ]) {
        row.removeAttribute(name);
      }
      // Spotify's rows are: an icon slot (empty for rows without an icon),
      // the text in its box, then a shortcut hint. The first two are copied
      // empty, so the label lines up with Spotify's.
      const textModel = model.querySelector('[data-encore-id="text"]');
      const textBox = textModel
        ? [...model.children].find((child) => child.contains(textModel)) || null
        : null;
      const iconModel = textBox && model.firstElementChild !== textBox ? model.firstElementChild : null;
      const svg = parseIcon(item.icon);
      if (iconModel) {
        const iconSlot = iconModel.cloneNode(false);
        if (svg) iconSlot.append(svg);
        row.append(iconSlot);
      } else if (svg) {
        row.append(svg);
      }
      const text = textModel ? textModel.cloneNode(false) : document.createElement("span");
      text.textContent = item.label;
      if (textBox && textBox !== textModel) {
        const box = textBox.cloneNode(false);
        box.append(text);
        row.append(box);
      } else {
        row.append(text);
      }
    } else {
      row = document.createElement("div");
      row.className = "spacetify-plain";
      const svg = parseIcon(item.icon);
      if (svg) row.append(svg);
      row.append(document.createTextNode(item.label));
    }
    row.setAttribute("role", "menuitem");
    row.setAttribute("tabindex", "-1");
    row.dataset.spacetifyMenuItem = item.state.id;
    return { row, parent: rows.length ? rows[rows.length - 1].parentElement : menu };
  };

  placers.add(() => {
    if (!menuItems.length) return;
    for (const root of document.querySelectorAll(MENU_ROOT)) {
      const menu = root.querySelector('[role="menu"]');
      if (!menu || seenMenus.has(menu)) continue;
      seenMenus.add(menu);
      const target = menuTarget(menu);
      if (!target) continue;
      for (const item of menuItems) {
        if (item.state.status === "stopped") continue;
        if (item.when && !item.when(target)) continue;
        const { row, parent } = menuRow(menu, item);
        const choose = (event) => {
          event.preventDefault();
          event.stopPropagation();
          closeMenu(menu);
          item.onClick(target);
        };
        row.addEventListener("click", choose);
        row.addEventListener("keydown", (event) => {
          if (event.key === "Enter" || event.key === " ") choose(event);
        });
        parent.append(row);
      }
    }
  });

  const addMenuItem = (state, options) => {
    checkOptions(options);
    checkFunction(options.onClick);
    if (options.when !== undefined) checkFunction(options.when);
    if (typeof options.label !== "string" || !options.label.trim()) {
      throw new TypeError("contextMenuItem needs a label");
    }
    parseIcon(options.icon);
    useUiStyle();
    const item = {
      state,
      label: options.label,
      icon: options.icon,
      onClick: guard(state, "contextMenuItem onClick", options.onClick),
      when: options.when ? guard(state, "contextMenuItem when", options.when) : null,
    };
    menuItems.push(item);
    const removeItem = () => {
      const index = menuItems.indexOf(item);
      if (index >= 0) menuItems.splice(index, 1);
      for (const row of document.querySelectorAll(`[data-spacetify-menu-item="${state.id}"]`)) row.remove();
    };
    state.cleanups.push(removeItem);
    return Object.freeze({ remove: removeItem });
  };

  // ----------------------------------------------------- interface: popup

  const openPopup = (state, options) => {
    checkOptions(options);
    if (options.onClose !== undefined) checkFunction(options.onClose);
    useUiStyle();
    const overlay = document.createElement("div");
    overlay.className = "spacetify-popup-overlay";
    overlay.dataset.spacetifyExtension = state.id;
    const box = document.createElement("div");
    box.className = "spacetify-popup";
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-modal", "true");
    if (typeof options.width === "number" && options.width > 0) {
      box.style.setProperty("--spacetify-popup-width", `${options.width}px`);
    }
    const head = document.createElement("div");
    head.className = "spacetify-popup-head";
    const title = document.createElement("h2");
    title.textContent = String(options.title || state.name);
    box.setAttribute("aria-label", title.textContent);
    const closeButton = document.createElement("button");
    closeButton.type = "button";
    closeButton.className = "spacetify-popup-close";
    closeButton.setAttribute("aria-label", "Close");
    closeButton.textContent = "×";
    head.append(title, closeButton);
    const body = document.createElement("div");
    body.className = "spacetify-popup-body";
    if (options.content instanceof Node) body.append(options.content);
    else if (options.content !== undefined && options.content !== null) body.textContent = String(options.content);
    box.append(head, body);
    overlay.append(box);

    let closed = false;
    const onClose = options.onClose ? guard(state, "popup onClose", options.onClose) : null;
    const onKey = (event) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close();
      }
    };
    function close() {
      if (closed) return;
      closed = true;
      overlay.remove();
      document.removeEventListener("keydown", onKey, true);
      if (onClose) onClose();
    }
    closeButton.addEventListener("click", close);
    overlay.addEventListener("mousedown", (event) => {
      if (event.target === overlay) close();
    });
    document.addEventListener("keydown", onKey, true);
    state.cleanups.push(close);
    onReady(() => {
      if (!closed) document.body.appendChild(overlay);
      closeButton.focus();
    });
    return Object.freeze({ element: body, close });
  };

  // ------------------------------------------------------- keyboard shortcuts

  const KEY_NAMES = {
    control: "ctrl",
    cmd: "meta",
    command: "meta",
    super: "meta",
    win: "meta",
    option: "alt",
    esc: "escape",
    space: " ",
    left: "arrowleft",
    right: "arrowright",
    up: "arrowup",
    down: "arrowdown",
    del: "delete",
    plus: "+",
  };
  const shortcuts = new Set();

  const parseShortcut = (combo) => {
    if (typeof combo !== "string" || !combo.trim()) {
      throw new TypeError('a shortcut like "ctrl+shift+k" is required');
    }
    const parts = combo
      .toLowerCase()
      .split("+")
      .map((part) => part.trim())
      .map((part) => KEY_NAMES[part] || part);
    const wanted = { ctrl: false, shift: false, alt: false, meta: false, key: null };
    for (const part of parts) {
      if (part in wanted && part !== "key") wanted[part] = true;
      else if (wanted.key === null && part) wanted.key = part;
      else throw new TypeError(`"${combo}" must have exactly one key besides ctrl, shift, alt and meta`);
    }
    if (wanted.key === null) throw new TypeError(`"${combo}" has no key besides ctrl, shift, alt and meta`);
    return wanted;
  };

  // Letters and digits are also matched by their place on the keyboard, so
  // "ctrl+shift+1" works whatever character shift gives.
  const sameKey = (event, key) => {
    const pressed = String(event.key || "").toLowerCase();
    if (pressed === key) return true;
    if (/^[a-z]$/.test(key)) return event.code === `Key${key.toUpperCase()}`;
    if (/^[0-9]$/.test(key)) return event.code === `Digit${key}`;
    return false;
  };

  const isTyping = (target) =>
    !!target &&
    (target.isContentEditable ||
      ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));

  window.addEventListener(
    "keydown",
    (event) => {
      if (!shortcuts.size) return;
      for (const shortcut of [...shortcuts]) {
        const want = shortcut.keys;
        if (
          event.ctrlKey !== want.ctrl ||
          event.shiftKey !== want.shift ||
          event.altKey !== want.alt ||
          event.metaKey !== want.meta ||
          !sameKey(event, want.key)
        ) {
          continue;
        }
        if (isTyping(event.target) && !shortcut.inInputs) continue;
        event.preventDefault();
        event.stopPropagation();
        shortcut.callback(event);
      }
    },
    true,
  );

  const addShortcut = (state, combo, callback, options = {}) => {
    const keys = parseShortcut(combo);
    checkFunction(callback);
    const shortcut = {
      keys,
      inInputs: !!(options && options.inInputs),
      callback: guard(state, `shortcut ${combo}`, callback),
    };
    shortcuts.add(shortcut);
    const removeShortcut = () => shortcuts.delete(shortcut);
    state.cleanups.push(removeShortcut);
    return removeShortcut;
  };

  // ---------------------------------------------------------------- pages

  const MAIN_VIEW = ".main-view-container";
  const pages = new Map();

  const hidePage = (page) => {
    if (!page.container) return;
    const { container, cleanup, host, hostPosition } = page;
    page.container = null;
    page.cleanup = null;
    container.remove();
    if (host && hostPosition !== null) host.style.position = hostPosition;
    if (cleanup) cleanup();
  };

  // A page shows while Spotify is on its path, over Spotify's main view:
  // the sidebar, the top bar and the player bar stay.
  placers.add(() => {
    if (!pages.size) return;
    const path = currentPath();
    for (const page of pages.values()) {
      if (path !== page.path) {
        hidePage(page);
        continue;
      }
      if (page.container && page.container.isConnected) continue;
      hidePage(page);
      const host = document.querySelector(MAIN_VIEW);
      if (!host) continue;
      useUiStyle();
      page.host = host;
      page.hostPosition = null;
      if (getComputedStyle(host).position === "static") {
        page.hostPosition = host.style.position;
        host.style.position = "relative";
      }
      const container = document.createElement("div");
      container.className = "spacetify-page";
      container.dataset.spacetifyExtension = page.state.id;
      container.dataset.spacetifyPage = page.path;
      host.append(container);
      page.container = container;
      const result = page.render(container);
      if (typeof result === "function") page.cleanup = guard(page.state, "page cleanup", result);
    }
  });

  const addPage = (state, options) => {
    checkOptions(options);
    checkFunction(options.render);
    const name = options.name === undefined ? "" : options.name;
    if (typeof name !== "string" || (name && !/^[a-z0-9-]+$/.test(name))) {
      throw new TypeError("a page name is made of lowercase letters, digits and dashes");
    }
    const path = `/spacetify/${state.id}${name ? `/${name}` : ""}`;
    if (pages.has(path)) throw new Error(`there is already a page at ${path}`);
    const page = {
      state,
      path,
      render: guard(state, "page render", options.render),
      container: null,
      cleanup: null,
    };
    pages.set(path, page);
    const removePage = () => {
      if (pages.get(path) !== page) return;
      pages.delete(path);
      hidePage(page);
    };
    state.cleanups.push(removePage);
    queueScan();
    return Object.freeze({
      path,
      open: () => navigate(path),
      get isOpen() {
        return currentPath() === path;
      },
      remove: removePage,
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

      player: createPlayer(state),

      library: createLibrary(state),

      navigate(target) {
        navigate(target);
      },

      currentPath,

      ui: Object.freeze({
        topbarButton: (options) => createButton(state, "topbar", topbarButtons, options),
        playbarButton: (options) => createButton(state, "playbar", playbarButtons, options),
        contextMenuItem: (options) => addMenuItem(state, options),
        popup: (options) => openPopup(state, options),
        page: (options) => addPage(state, options),
      }),

      keyboard: Object.freeze({
        add: (combo, callback, options) => addShortcut(state, combo, callback, options),
      }),
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

  if (CONFIG.mode === "live") {
    // Live mode wins over mods applied to the file: their runtime stops at
    // once (window.__spacetify is taken) and their theme is taken out.
    onReady(() => {
      for (const link of document.querySelectorAll('link[href*="spacetify/inject.css"]')) link.remove();
    });
    if (CONFIG.css) addStyleElement(CONFIG.css, { id: "spacetify-theme" });
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
        startGallery(CONFIG.gallery, (level, args) => write(level, "gallery", args), currentPath);
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
