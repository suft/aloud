# Aloud

Desktop app that turns an EPUB into an M4B audiobook with Kokoro, on ONNX Runtime. Rust core,
Tauri 2 shell, Svelte 5 UI.

## Layout

- `crates/aloud-core/src`: the engine, with no Tauri dependency.
  - `epub/`: import (container/OPF, TOC, chapter text and encodings, cover).
  - `plan.rs`, `pron.rs`, `g2p.rs`: text → pronunciation fixes → phonemes → a cached plan of segments.
  - `render/{mod,job,cache,preview}.rs`: queue, scheduler, workers, per-segment PCM cache, previews.
  - `tts/`: Kokoro on ONNX Runtime. `export.rs`: M4B writer. `models.rs`: voice-pack downloads.
  - `project.rs`: `ProjectStore` and `project.json`. `settings.rs`, `speed.rs`: settings and speed limits.
- `src-tauri/src`: `commands.rs` (Tauri commands and DTOs), `state.rs` (`AppState`, event bridge),
  `player.rs` (audio thread), `lib.rs` (setup, window/exit handling), `error.rs` (`AppError`, `run`).
- `src/lib`: `api.ts` (typed commands and events), `stores/` (Svelte 5 runes stores), `components/`.
- `scripts/`: setup, smoke test, screenshots and release helpers.

## Commands

- `mise install && mise run setup`: tools, JS deps, espeak-ng data, ONNX Runtime, licence notices.
- `mise run dev`: the app; set `ALOUD_DATA_DIR` to keep a dev data dir (`mise run models` fills it).
- `mise run test`: `cargo test --workspace --exclude aloud`, then `bun run check`.
- `mise run lint`, `mise run fmt`: rustfmt, clippy (`-D warnings`), svelte-check, prettier.
- `mise run smoke`: renders a tiny generated EPUB end to end with the fast model.
- `mise run audit`: `cargo deny` and `bun audit`. `mise run screenshots`: README screenshots.
- One test: `cargo test -p aloud-core <name>`.

## Rules

- The `aloud` crate (`src-tauri`) has no tests: its test executable hangs on Windows. Put
  logic that needs a test in `aloud-core`.
- Commands run blocking work through `error::run` and return `AppError`s that are plain sentences
  for non-developers. `aloud_core::Error::Invalid(msg)` shows `msg` unchanged.
- Never call a `ProjectStore` method inside the closure of `ProjectStore::update`; it holds the
  project's lock.
- Settings writes go through `AppState::update_settings`, which holds the settings lock.
- Reading-speed limits live only in `aloud_core::speed`; the UI gets them through `AppStateDto`.
- Model URLs, sizes and checksums live only in `crates/aloud-core/models.json`; `models.rs` and `scripts/lib.ts` read it.
- The UI's DTO types in `src/lib/bindings/` are generated from the Rust types with ts-rs: after
  changing a DTO run `mise run bindings` (`mise run lint` fails when they are stale) and never edit
  them by hand. A new DTO also goes into `src-tauri/src/bindings.rs`. A changed or new command must
  also change in `src/lib/api.ts` and in the backend mocks `scripts/screenshots/{mock,alice}.ts`.
- Events: `render-progress`, `project-updated`, `queue-updated`, `notice`, `player-state`,
  `model-download`, `update-progress`, `confirm-quit`.
- `src/lib/components/ui/` is generated shadcn-svelte code; don't restyle it beyond what is needed.
- Prettier ignores `src-tauri/` and `crates/`; rustfmt covers them.
- ONNX Runtime comes from the `onnxruntime-1.31.0` mirror release through `ORT_CACHE_DIR`
  (`scripts/fetch-onnxruntime.ts`). When `ort` is bumped, rerun `scripts/mirror-onnxruntime.sh` and
  update the table in `fetch-onnxruntime.ts`. The mirror is a pre-release: the updater reads
  `releases/latest`.
- Commits are Conventional, ending in `(#N)` for the issue. A user-visible change also gets a line
  under `## [Unreleased]` in `CHANGELOG.md`.

## Test helpers

- `render/tests.rs`: `fixture`, `service`, `service_with`, `wait_until` and the fake phonemizer and
  synth.
- `epub/tests.rs`: `epub_with`, `epub_with_spine`, `import_bytes`.
- `models.rs` tests: `serve_with` (a local HTTP server that can refuse Range requests).
