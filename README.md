<p align="center">
  <img src="assets/icon.svg" width="128" height="128" alt="Aloud app icon">
</p>

<h1 align="center">Aloud</h1>

<p align="center">
  <a href="https://github.com/suft/aloud/actions/workflows/ci.yml"><img src="https://github.com/suft/aloud/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/suft/aloud/releases/latest"><img src="https://img.shields.io/github/v/release/suft/aloud?sort=semver" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)%20%7C%20Windows%20%7C%20Linux-informational" alt="Platforms: macOS (Apple Silicon), Windows, Linux">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue" alt="License: GPL-3.0-or-later"></a>
</p>

Turn a DRM-free English EPUB into a chaptered `.m4b` audiobook. Everything runs on your
computer with the [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M) voice model; nothing is uploaded.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/book-dark.png">
  <img src="docs/screenshots/book-light.png" alt="Aloud making an audiobook of Alice's Adventures in Wonderland">
</picture>

- Resumable renders: closing the app never loses finished parts.
- Voice picker with spoken preview, reading speed, spoken title/author introduction.
- Per-chapter selection, editable title/author/cover.
- Pronunciation dictionary and a "words to check" finder; changing a word re-renders only the sentences that contain it.
- Built-in player, ETA, pause/cancel, multi-book queue, keep-awake and a "leave some power for me" mode.
- Optional (experimental) graphics-card acceleration.

English books only; output is always M4B (AAC).

## Screenshots

<table>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/library-dark.png">
        <img width="100%" src="docs/screenshots/library-light.png" alt="Library with Alice's Adventures in Wonderland being made">
      </picture>
      <br>Library
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/voices-dark.png">
        <img width="100%" src="docs/screenshots/voices-light.png" alt="Choosing a voice for Alice's Adventures in Wonderland">
      </picture>
      <br>Voice picker
    </td>
  </tr>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/pronunciations-dark.png">
        <img width="100%" src="docs/screenshots/pronunciations-light.png" alt="Words to check and pronunciation fixes">
      </picture>
      <br>Pronunciations
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/settings-dark.png">
        <img width="100%" src="docs/screenshots/settings-light.png" alt="Settings">
      </picture>
      <br>Settings
    </td>
  </tr>
</table>

## Install

Download the installer for your system from the [Releases](../../releases) page.

The builds are not code-signed:

- **macOS** (Apple Silicon):
  1. Open the `.dmg` and drag Aloud to Applications.
  2. Open Aloud. When macOS blocks it, click **Done**.
  3. In **System Settings → Privacy & Security**, click **Open Anyway** next to the message about Aloud and confirm.

  On macOS 14 and earlier, right-click the app and choose **Open** instead. If macOS says Aloud "is damaged and
  can't be opened", run `xattr -dr com.apple.quarantine /Applications/Aloud.app` in Terminal and open it again.
  Intel Macs are not supported because ONNX Runtime ships no Intel macOS build.

- **Windows**: when SmartScreen appears, click **More info → Run anyway**.
- **Linux** (x86_64): AppImage or `.deb`. Needs glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40 or later).
  The AppImage bundles WebKitGTK; the `.deb` depends on `libwebkit2gtk-4.1-0`, which apt installs. Releases are
  built on Ubuntu 24.04.

On first launch Aloud offers two voice packs: **Natural** (354 MB download, best sound) and **Fast** (121 MB).
Your Mac's or PC's processor does the work; the voice packs are downloaded once from GitHub and checked against fixed checksums.

Aloud checks GitHub for a newer release at launch (at most once a day) and shows an **Update** button in the header
when one is available; updates are signed and verified before they are installed. Turn the automatic check off, or
check by hand, in **Settings → About**.

## Develop

Tools are managed with [mise](https://mise.jdx.dev). System packages on Linux: `webkit2gtk-4.1`, `alsa-lib`,
`libclang`, `libc++`.

```sh
mise install && mise run setup && mise run dev
```

| Task                    | What it does                                                                 |
| ----------------------- | ---------------------------------------------------------------------------- |
| `mise run dev`          | Run the app with hot reload (`ALOUD_DATA_DIR=<dir>` keeps dev data separate) |
| `mise run build`        | Build installers into `target/release/bundle/`                               |
| `mise run test`         | Rust tests and `svelte-check`                                                |
| `mise run lint` / `fmt` | clippy, rustfmt, prettier                                                    |
| `mise run models`       | Download the models and a public-domain test book into `.dev-data/`          |
| `mise run e2e`          | Render two chapters of the test book with the CLI example                    |
| `mise run smoke`        | Render a tiny generated EPUB end to end (fast model; CI runs this)           |
| `mise run audit`        | cargo-deny (advisories, licences, sources) and bun audit                     |
| `mise run notices`      | Regenerate THIRD-PARTY-NOTICES.txt (cargo-about + npm licences)              |
| `mise run screenshots`  | Regenerate the README screenshots (headless Chromium, mocked backend)        |

The Rust core (`crates/aloud-core`) has no UI dependency. `crates/aloud-core/examples/aloud.rs` is a CLI over the
same render service (`--list`, `--chapters 1-3`, `--voice`, `--pron "word=sounds like"`, `--bench`).

## License

Aloud is GPL-3.0-or-later ([LICENSE](LICENSE)). espeak-ng, which is compiled in, is GPL-3.0-or-later too.

It also uses:

- Kokoro-82M (Apache-2.0), downloaded on first launch
- misaki-rs (MIT)
- ONNX Runtime (MIT)
- Inter (OFL-1.1)

The installers include `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt` (every Rust crate and JavaScript package
with its licence text). `mise run setup` generates the notices; `mise run notices` regenerates them.

The AAC encoder, rusty_aac 2.x, is Apache-2.0. Its authors state that it was written from ISO/IEC 14496-3
and that FFmpeg appears only in comments and its test harness. It grants no patent rights.
