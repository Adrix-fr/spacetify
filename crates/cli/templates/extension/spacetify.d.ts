// Types of the `spacetify` API, for autocompletion in your editor.
//
// spacetify never reads this file: it only helps editors such as VS Code
// suggest and check the API while you write main.js. The full guide is
// docs/extensions.md in spacetify's repository.

/** The API given to this extension's main.js. */
declare const spacetify: Spacetify.Api;

declare namespace Spacetify {
  interface Api {
    /** Version of spacetify, such as "0.4.0". */
    readonly version: string;
    /** "file" after `spacetify apply`, "live" with `launch --live` or `spacetify dev`. */
    readonly mode: "file" | "live";
    /** This extension, from its manifest. */
    readonly extension: { readonly id: string; readonly name: string; readonly version: string };
    /** Writes to Spotify's console, prefixed with this extension's id. */
    readonly log: {
      info(...values: unknown[]): void;
      warn(...values: unknown[]): void;
      error(...values: unknown[]): void;
    };

    /** The first element matching `selector`, as soon as it exists. */
    waitFor(selector: string, options?: { timeout?: number }): Promise<Element>;
    /** Calls `callback` once for every element matching `selector`, now and later. */
    observe(selector: string, callback: (element: Element) => void): Stop;
    /** Calls `callback` with the new path when Spotify shows another page. */
    onNavigate(callback: (path: string) => void): Stop;
    /** Adds CSS after Spotify's own. Returns a function that removes it. */
    addStyle(css: string): Stop;

    /** The page Spotify shows, such as "/album/1XslIirSxfAhhxRdn4Li9t". */
    currentPath(): string;
    /** Shows a page: a path ("/search") or a Spotify URI ("spotify:album:…"). */
    navigate(target: string): void;

    /** The settings declared in extension.toml. */
    readonly settings: Settings;
    /** What plays, and how to control it. */
    readonly player: Player;
    /** The tracks of a playlist or of Liked Songs. */
    readonly library: Library;
    /** Buttons, right-click menu entries, popups and pages. */
    readonly ui: Ui;
    /** Keyboard shortcuts. */
    readonly keyboard: Keyboard;
  }

  /** Stops what was started: a listener, a style, a shortcut… */
  type Stop = () => void;

  type SettingValue = boolean | number | string;

  interface Settings {
    get(name: string): SettingValue;
    set(name: string, value: SettingValue): void;
    reset(name?: string): void;
    all(): Record<string, SettingValue>;
    onChange(callback: (name: string, value: SettingValue) => void): Stop;
    readonly definitions: Record<string, unknown>;
  }

  interface Track {
    /** Such as "spotify:track:…" or "spotify:episode:…". */
    readonly uri: string;
    readonly name: string;
    /** "track", "episode"… */
    readonly type: string;
    /** In milliseconds. */
    readonly duration: number;
    readonly artists: readonly { readonly uri: string; readonly name: string }[];
    readonly album: { readonly uri: string; readonly name: string } | null;
    /** Address of the cover, the largest available. */
    readonly image: string | null;
    readonly isLocal: boolean;
    readonly isExplicit: boolean;
  }

  interface PlayerState {
    /** null when nothing is loaded. */
    readonly track: Track | null;
    /** The album, playlist… being played. */
    readonly context: { readonly uri: string } | null;
    readonly isPlaying: boolean;
    readonly isPaused: boolean;
    /** In milliseconds, computed when `state()` is called. */
    readonly position: number;
    /** In milliseconds. */
    readonly duration: number;
    readonly shuffle: boolean;
    readonly repeat: "off" | "context" | "track";
    readonly speed: number;
  }

  interface Queue {
    readonly current: Track | null;
    /** Added with "Add to queue", played first. */
    readonly queued: readonly Track[];
    /** What comes next in the album or playlist. */
    readonly next: readonly Track[];
  }

  interface Player {
    /** False if this version of Spotify hides its player: then only togglePlay, next and previous work. */
    readonly available: boolean;
    /** Resolves once the player can be used (Spotify finds it a moment after starting). */
    ready(timeout?: number): Promise<void>;
    /** What plays now, or null before the player is ready. */
    state(): PlayerState | null;
    /** Called on every change: track, play or pause, position, shuffle… */
    onChange(callback: (state: PlayerState | null) => void): Stop;
    /** Called when another track starts. */
    onTrackChange(callback: (track: Track | null) => void): Stop;
    /** Called with the new volume, from 0 to 1. */
    onVolumeChange(callback: (volume: number) => void): Stop;

    play(): Promise<void>;
    pause(): Promise<void>;
    togglePlay(): Promise<void>;
    next(): Promise<void>;
    previous(): Promise<void>;
    /** Goes to a position, in milliseconds. */
    seek(milliseconds: number): Promise<void>;
    setShuffle(on: boolean): Promise<void>;
    setRepeat(mode: "off" | "context" | "track"): Promise<void>;
    /** Plays a track, an album, a playlist… from its URI. */
    playUri(uri: string): Promise<void>;
    /** From 0 to 1. */
    getVolume(): Promise<number>;
    /** From 0 to 1. */
    setVolume(volume: number): Promise<void>;

    /**
     * Plays these tracks in this order, like a playlist of its own (10 000 at
     * most). Unlike the queue, which Spotify keeps to about 80 tracks, it
     * holds them all.
     */
    playTracks(uris: string[]): Promise<void>;

    queue(): Promise<Queue>;
    addToQueue(uris: string | string[]): Promise<void>;
    /** Takes tracks added with addToQueue (or "Add to queue") out of the queue. */
    removeFromQueue(uris: string | string[]): Promise<void>;
    clearQueue(): Promise<void>;
  }

  interface Library {
    /**
     * Every track of a playlist ("spotify:playlist:…") or of Liked Songs
     * ("spotify:collection:tracks"), in order. Tracks that cannot play here
     * are left out. Rejects for other URIs.
     */
    tracks(uri: string): Promise<readonly Track[]>;
  }

  interface ButtonOptions {
    /** Shown when the button is pointed at, and read by screen readers. */
    label: string;
    /** One <svg> element, drawn with currentColor. Without it, the label's first letter shows. */
    icon?: string;
    onClick(event: MouseEvent): void;
  }

  interface Button {
    readonly element: HTMLButtonElement;
    setLabel(label: string): void;
    /** Shows the button as on (in the theme's accent colour). */
    setActive(on: boolean): void;
    remove(): void;
  }

  /** What was right-clicked. */
  interface MenuTarget {
    readonly uri: string;
    /** Every selected item (several when tracks are selected together). */
    readonly uris: readonly string[];
    /** "track", "album", "playlist", "artist", "episode"… */
    readonly type: string;
    /** The album or playlist the item was in, if any. */
    readonly contextUri: string | null;
  }

  interface MenuItemOptions {
    label: string;
    icon?: string;
    /** The entry only shows when this returns true. */
    when?(target: MenuTarget): boolean;
    onClick(target: MenuTarget): void;
  }

  interface PopupOptions {
    title?: string;
    /** Text, or an element of your own. */
    content?: string | Node;
    /** In pixels, 560 by default. */
    width?: number;
    onClose?(): void;
  }

  interface PageOptions {
    /** Several pages: lowercase letters, digits and dashes. */
    name?: string;
    /** Draws the page into `container` each time it opens. May return a function called when it closes. */
    render(container: HTMLElement): void | (() => void);
  }

  interface Page {
    /** "/spacetify/<extension id>" or "/spacetify/<extension id>/<name>". */
    readonly path: string;
    readonly isOpen: boolean;
    open(): void;
    remove(): void;
  }

  interface Ui {
    /** A button in the top bar, after the back and forward arrows. */
    topbarButton(options: ButtonOptions): Button;
    /** A button in the player bar, before the lyrics button. */
    playbarButton(options: ButtonOptions): Button;
    /** An entry at the end of Spotify's right-click menu. */
    contextMenuItem(options: MenuItemOptions): { remove(): void };
    /** A window over Spotify, closed with its ×, Escape or a click outside. */
    popup(options: PopupOptions): { readonly element: HTMLElement; close(): void };
    /** A page of its own, shown in Spotify's main view. */
    page(options: PageOptions): Page;
  }

  interface Keyboard {
    /**
     * A shortcut such as "ctrl+shift+k", "alt+arrowleft" or "f2". It does not
     * fire while typing in a text field, unless `inInputs` is true.
     */
    add(shortcut: string, callback: (event: KeyboardEvent) => void, options?: { inInputs?: boolean }): Stop;
  }
}
