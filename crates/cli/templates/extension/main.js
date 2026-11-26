// {{name}}
//
// `spacetify` is this extension's own API. Your editor suggests what it
// offers thanks to spacetify.d.ts; the guide is docs/extensions.md.
//
// Try it with `spacetify dev .` in this folder: Spotify starts with this
// extension, and reloads it each time you save a file.

// A button next to Spotify's back and forward arrows, that opens a popup.
spacetify.ui.topbarButton({
  label: "{{name}}",
  icon: '<svg viewBox="0 0 16 16"><path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8z"/><circle cx="8" cy="8" r="3"/></svg>',
  onClick() {
    const now = spacetify.player.state();
    const playing = now && now.track ? `${now.track.name} — ${now.track.artists.map((a) => a.name).join(", ")}` : "nothing";
    spacetify.ui.popup({
      title: "{{name}}",
      content: `${spacetify.settings.get("greeting")}\n\nNow playing: ${playing}`,
    });
  },
});

// Logs each new track in Spotify's console.
spacetify.player.onTrackChange((track) => {
  if (track) spacetify.log.info("now playing", track.name);
});
