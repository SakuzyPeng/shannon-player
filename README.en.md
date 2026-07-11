<div align="center">

# Shannon Player

**A warm, understated desktop player devoted to your local music.**

An apricot, paper-textured interface with the breathing room of serif type,
a fully realized light / dark theme, and internationalization from day one
(Simplified Chinese and English available today).

[简体中文](README.md) · English

</div>

---

## What it is

Shannon Player is a work-in-progress local music player, made for people who
cherish their own music files — no tracking, no streaming, no noise. It simply
presents your library in a comfortable way.

The interface is built on a complete, original design system that aims for a
quiet, lasting everyday feel.

## Features

- **Library browsing** — switch freely between an album grid and a detail list
- **Floating play bar** — playback controls, progress, volume, shuffle, and repeat at a glance
- **Light / Dark / Follow system** — one-tap cycling, with a dark palette tuned as its own set
- **Multilingual** — Simplified Chinese and English, switchable anytime
- **Thoughtful interactions** — keyboard-navigable context menus, gentle motion that never shouts

> The project is in early development: every interface page is now built to the
> design spec. Next up is local library scanning and real audio playback (the app
> currently runs on seed data). See the roadmap below.

## Installation

No official release yet. To try it early, build from source — see the
[development guide](docs/DEVELOPMENT.md).

## Roadmap

- [x] Main library screen (album grid / list, floating play bar, theming and i18n)
- [x] Album / Artist / Playlist detail pages and Songs page
- [x] Lyrics page (word-by-word rendering via AMLL)
- [x] Global search and Favorites page
- [x] Settings page
- [x] First-run onboarding (welcome / scanning / done)
- [ ] Local library scanning and real audio playback (Rust backend)

## Contributing

Contributions are welcome. See the [development guide](docs/DEVELOPMENT.md) for
the stack, build steps, and code conventions, and the [changelog](CHANGELOG.md)
for what has changed.

## License

[GNU Affero General Public License v3.0](LICENSE).
