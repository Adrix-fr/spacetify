# {{name}}

A spacetify extension.

## Try it

```sh
spacetify dev .
```

Spotify starts with this extension. Each time you save a file, spacetify
checks it and reloads Spotify's interface, without restarting Spotify.
Close Spotify to stop.

## Files

- `extension.toml`: the name, version, settings… of the extension.
- `main.js`: its code.
- `spacetify.d.ts` and `jsconfig.json`: only for your editor, so it suggests
  the `spacetify` API as you type. spacetify does not use them.

## Share it

`spacetify gallery pack .` checks the extension and prepares it for the
gallery's request form (or use the *Submit* page of the spacetify app).
