# Branding

> **Title:** *ASTROMANCY* — a deterministic RTS of **space and magic**. The
> wordmark is one line of text in `logo.svg`, trivially re-lettered (see
> [Renaming](#renaming)).

## Concept

A deterministic RTS of **space and magic**. The emblem is the **Astromancy
mission patch** (NRO/NASA style): a hooded Philosophia magus standing over a
world, conjuring a starship that rises straight up on a plume of arcane sparkles,
ringed by the motto *SCIENTIA EST MAGIA*. Pure vector, so it scales from favicon
to banner. (Sibling patches — the kraken-and-ship **Astromancy** badge and
**Draco** — live one level up in [`../`](../) as `logo*.svg`.)

## Files

| File | What it is | Use for |
|---|---|---|
| `emblem.svg` / `.png` | The mission patch alone (480² / 1024²) | **Default logo** — README header, favicon, Discord/app icon |
| `logo-square.svg` / `.png` | Patch + ASTROMANCY wordmark, 1:1 (1024²) | Avatar, app/store icon, square social posts |
| `social-card.svg` / `.png` | Share card, 1200×630 | Link previews (Open Graph) + GitHub **Social Preview** |
| `logo.svg` / `.png` | Patch + wordmark, wide (1600×520 / 2000px) | Wide banner / title-screen header |
| `emblem-preview.png` | Patch on a dark disc | Quick preview |

The wide/square/card SVGs pull the patch in via `<image href="emblem.png">`, so
re-render `emblem.png` first, then the rest.

The PNGs are generated from the SVGs — edit the SVGs, then re-render (below).

## Palette

| Role | Hex |
|---|---|
| Deep space (bg) | `#05080f` → `#0d1730` |
| Sun core → rim | `#ffffff` · `#ffe6a6` · `#ff9a3c` · `#ff6a1e` |
| Ember (fault line, accent) | `#ff5a2c` / `#ff7a3c` |
| Steel (orbits, reticle) | `#33507a` · `#5b81b3` · `#9bbce6` |
| Faction A (cool) | `#2f74da` |
| Faction B (warm) | `#ff5a2c` |
| Wordmark | `#eaf1fb` (text) · `#ffb454` (amber accent) · `#8aa3cc` (tagline) |

## Re-rendering the PNGs

No system renderer needed — uses the self-contained `@resvg/resvg-js`:

```bash
npm install @resvg/resvg-js
node - <<'JS'
const { Resvg } = require('@resvg/resvg-js');
const fs = require('fs');
const out = (svg, png, width, bg) => {
  const opts = { fitTo: { mode: 'width', value: width }, font: { loadSystemFonts: true } };
  if (bg) opts.background = bg;
  fs.writeFileSync(png, new Resvg(fs.readFileSync(svg), opts).render().asPng());
};
out('emblem.svg',      'emblem.png',      1024);          // render first — the rest embed it
out('logo.svg',        'logo.png',        2000, '#070b14');
out('logo-square.svg', 'logo-square.png', 1024);
out('social-card.svg', 'social-card.png', 1200);
JS
```

> The wordmark uses a system sans (DejaVu/Arial fallback). For final art, swap in a
> condensed technical typeface (e.g. Eurostile / Bahnschrift / DIN Condensed) by
> setting `font-family` in `logo.svg`, or convert the text to paths so it renders
> identically everywhere.

## Renaming

The wordmark lives in `logo.svg` as a single `<text>`:

```xml
<text ... fill="url(#wm)">ASTROMANCY</text>
```

Change the text (and the tagline/motto `<text>`s below it), re-render, done. The
emblem is pulled in via `<image href="emblem.png">`, so swapping the patch is a
one-file change.
