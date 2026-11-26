# Writing a theme

A spacetify theme changes how Spotify looks. It is a folder with a manifest
and either a colour file, a stylesheet, or both. A complete example lives in
[`examples/themes/midnight`](../examples/themes/midnight).

```text
midnight/
  theme.toml    required: name, version, author…
  colors.toml   optional: Spotify's colours, replaced
  theme.css     optional: any other CSS
  assets/       optional: images and fonts used by theme.css
  showcase/     optional: pictures and text for the gallery page
```

At least one of `colors.toml` and `theme.css` is needed.

## Try it

```sh
spacetify new theme my-theme   # makes the folder my-theme, ready to edit
spacetify dev my-theme         # shows it in Spotify, reloads it at each save
```

`spacetify dev` starts Spotify in live mode and reloads the theme in a
second or two each time you save a file, without restarting Spotify. Close
Spotify to stop. The [extension guide](extensions.md#start-one) gives the
details. Without it:

```sh
spacetify install path/to/my-theme   # copies it and makes it the active theme
spacetify doctor                     # checks it against your Spotify
spacetify apply                      # shows it in Spotify
```

After editing your files, run `spacetify install` again: it replaces the
installed copy. `spacetify theme none` goes back to Spotify's own look, and
`spacetify restore` removes everything from Spotify.

## Rule number one: only use stable hooks

Spotify's stylesheets are generated. Most class names look like
`.gyQ3lHdZrOLMpQhHd2rw` or `.e-10860-button`: they mean nothing and can change
with any update. Tools that rely on them break every few weeks.

We compared two real versions of Spotify (1.3.5.3058 and 1.3.6.1921) to see
what survives an update:

| Hook                                          | Count | Still there after the update | Use it?           |
| --------------------------------------------- | ----- | ---------------------------- | ----------------- |
| Colour variables (`--background-base`, …)     | 410   | 100 %                        | Yes, first choice |
| `data-testid` attributes                      | 321   | 99.7 %                       | Yes               |
| Readable classes (`.encore-dark-theme`, …)    | ~20   | 100 %                        | Yes               |
| Generated classes (`.gyQ3lHdZrOLMpQhHd2rw`)   | 5 476 | 99.4 %, with no guarantee    | No                |
| Versioned classes (`.e-10860-button`)         | 796   | 100 %, renamed together      | No                |
| `aria-label`                                  | 556   | translated into each language | No               |

So, in order of preference:

1. **Colour variables**, through `colors.toml`. Every colour of the interface
   comes from them.
2. **`data-testid` attributes**, such as
   `[data-testid="now-playing-bar"]`. Spotify uses them for its own tests,
   so they are kept on purpose.
3. **A few readable classes**: `encore-dark-theme`, the `encore-*-set`
   classes, `main-view-container`, `NowPlayingView`, `YourLibraryX`.

Avoid `aria-label` and `title` in selectors: their text changes with
Spotify's language, so a theme written for English would not work in French.

`spacetify doctor` checks every selector of your theme and tells you which
ones are missing, generated or impossible to check.

To find a `data-testid`, start Spotify with `spacetify launch --live -v`. The
log shows the debugging port; add `127.0.0.1:<port>` under *Configure* at
`chrome://inspect` in Chrome, and you can inspect Spotify's page like a web
page. The port only exists in live mode.

## theme.toml

```toml
id = "midnight"                 # unique, lowercase letters, digits and dashes
name = "Midnight"               # shown to the user
version = "1.0.0"               # MAJOR.MINOR.PATCH
author = "Your name"
description = "Deep blue backgrounds with a violet accent."   # optional
spacetify = ">=0.2.0"           # versions of spacetify this theme works with
```

Unknown fields are refused, to catch typos. The id `none` is reserved.

## colors.toml

Spotify's design system, Encore, gives every part of the interface a
*colour set*, and every set the same *colour tokens*. `colors.toml` replaces
tokens in the sets you name. Anything you leave out keeps Spotify's colour.

```toml
[base]
background-base = "#0f1020"
essential-bright-accent = "#a78bfa"

[bright-accent]
background-base = "#a78bfa"
```

Colours are written `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. The desktop
app always uses Spotify's dark theme, so these colours apply to it.

### Sets

| Set                                          | Used for                                        |
| -------------------------------------------- | ----------------------------------------------- |
| `base`                                       | almost everything: panels, text, icons          |
| `app-frame`                                  | the frame around the panels (black by default)  |
| `bright-accent`                              | accent buttons, such as the big play button     |
| `muted-accent`                               | quieter accent elements                         |
| `positive`, `positive-subdued`               | success messages                                |
| `negative`, `negative-subdued`               | errors                                          |
| `warning`, `warning-subdued`                 | warnings                                        |
| `announcement`, `announcement-subdued`       | information banners                             |
| `inverted`, `inverted-dark`, `inverted-light` | elements drawn in the opposite colours         |
| `over-media`                                 | controls drawn over pictures and videos         |

### Tokens

| Token                                                                             | What it colours                    |
| --------------------------------------------------------------------------------- | ---------------------------------- |
| `background-base`, `background-highlight`, `background-press`                     | backgrounds: normal, hover, click  |
| `background-elevated-base`, `-elevated-highlight`, `-elevated-press`              | cards, menus and pop-ups           |
| `background-tinted-base`, `-tinted-highlight`, `-tinted-press`                    | translucent backgrounds            |
| `text-base`, `text-subdued`                                                       | main and secondary text            |
| `text-bright-accent`, `text-positive`, `text-negative`, `text-warning`, `text-announcement` | coloured text             |
| `essential-base`, `essential-subdued`                                             | icons, borders, sliders            |
| `essential-bright-accent`, `essential-positive`, `essential-negative`, `essential-warning`, `essential-announcement` | coloured icons and bars |
| `decorative-base`, `decorative-subdued`                                           | decorations and separators         |

The `-elevated-` and `-tinted-` names above are short for
`background-elevated-base` and so on.

## theme.css

Any CSS, injected after Spotify's own styles. Use the variables above to
stay in tune with `colors.toml`:

```css
[data-testid="now-playing-bar"] {
  border-top: 1px solid var(--essential-bright-accent);
}
```

The file is checked before it is injected. These are refused, with the line
number: unclosed comments, strings or braces, and `@import` (put everything
in `theme.css`). Limit: 1 MiB.

You can define your own variables. `doctor` only checks the ones it does not
see you define.

### Images and fonts

Put them in `assets/` and refer to them with a relative path:

```css
@font-face {
  font-family: "My Font";
  src: url("assets/my-font.woff2");
}
.main-view-container {
  background-image: url("assets/stars.svg");
}
```

spacetify embeds them in the stylesheet, so they work the same with
`apply` and with `launch --live`. Supported: png, jpg, gif, webp, svg, woff,
woff2, ttf, otf. Limit: 2 MiB per file, 10 MiB for the whole theme. Remote
URLs (`https://…`) are left as they are, but Spotify may block them.

## Showing your mod in the gallery

The gallery shows every mod as a card, and a page of its own when it is
clicked. An optional `showcase/` folder fills them in. It works the same for
themes and extensions, and it is never put into Spotify: it is only shown in
the gallery.

```text
my-theme/
  showcase/
    1-home.png      pictures, in name order: the first is the card's cover
    2-playlist.png
    about.md        a longer description, in English
    about.fr.md     the same in French (optional)
    showcase.toml   tags = ["dark", "pastel"]
```

- **Pictures**: png, jpg, webp or gif, 6 at most, 1 MiB each. Wide
  pictures (16:9, such as 1600 × 900) look best. Clicking one shows it
  full size.
- **`about.md`**: up to 16 KiB of simple Markdown: `#` titles, paragraphs,
  lists, `**bold**`, `*italics*`, `` `code` ``, quotes and
  `[links](https://…)` (https only). HTML is shown as plain text, and
  pictures cannot go inside the text: put them in the folder. When Spotify
  is in French, `about.fr.md` is shown if it is there.
- **Tags**: up to 6, in lowercase letters, digits and `-`, 20 characters
  at most. People can search for them.

Mind what your pictures show: a screenshot of your Spotify shows your
library, your playlists and your account picture. Use a test account, or
crop them.

Every file of the showcase gets its SHA-256 in the gallery list, like the
mod's own files, and is checked when the gallery downloads it. The robot
reviews it like the rest of the mod: anything else in `showcase/` blocks the
request.

## Coming from Spicetify

`spacetify leave-spicetify` (or *Leave Spicetify* on the app's Home page)
imports the colours of your Spicetify themes for you, following the
mapping below. To carry a theme over by hand:

- `color.ini` maps well onto `colors.toml`: for example `main` →
  `background-base`, `text` → `text-base`, `subtext` → `text-subdued`,
  `button` → `essential-bright-accent`.
- `user.css` usually does not carry over. Its selectors, such as
  `.main-nowPlayingBar-container`, only exist because Spicetify rewrites
  Spotify's code to add them, and spacetify never rewrites Spotify's code.
  Replace them with `data-testid` selectors; `spacetify doctor` lists the
  ones that do not exist.
