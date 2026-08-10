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

- **Your actual library** — scans your music folders and groups albums and compilations on its own; wrong tags can be fixed, and your files are never rewritten
- **Gapless playback** — no silence between tracks, so an album meant to be heard whole stays whole
- **Loudness normalization** — evens out volume across tracks at playback time, leaving the files untouched
- **Output device** — pick the sound card or headphones the music goes to, right in the app, without touching system settings
- **Favorites and playlists** — what you collect stays collected; reorganizing folders, retagging, or rescanning won't lose it
- **Library browsing** — switch freely between an album grid and a detail list
- **Floating play bar** — playback controls, progress, volume, shuffle, and repeat at a glance
- **Light / Dark / Follow system** — one-tap cycling, with a dark palette tuned as its own set
- **Multilingual** — Simplified Chinese and English, switchable anytime
- **Thoughtful interactions** — keyboard-navigable context menus, gentle motion that never shouts

> Still in development, with no official release yet. Every interface page is
> built to the design spec, and local library scanning and stereo playback both
> work today; multichannel and spatial audio, along with more decoders, are still
> on the way. See the roadmap below.

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
- [x] Local library scanning (stable track IDs, album and compilation grouping, cover art, metadata editing)
- [x] Real audio playback (ALAC / FLAC / AAC / MP3 / WAV / AIFF / Vorbis and more, stereo path)
- [x] Gapless playback, loudness normalization, output device selection
- [x] Favorites and playlists that survive a restart
- [ ] More decoders and link-based importing
- [ ] Exclusive output mode
- [ ] Multichannel and spatial audio (downmix and spatialization left to the OS)

## Contributing

Contributions are welcome. See the [development guide](docs/DEVELOPMENT.md) for
the stack, build steps, and code conventions, and the [changelog](CHANGELOG.md)
for what has changed.

## License

[GNU Affero General Public License v3.0](LICENSE).
