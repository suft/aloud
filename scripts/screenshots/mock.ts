// Stands in for the Rust backend inside headless Chromium. Bundled and injected by capture.ts.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { appState, projectDetail, projectSummary, type Scenario, TRICKY, VOICES } from "./alice.ts";

const scenario = (new URLSearchParams(location.search).get("scenario") ?? "draft") as Scenario;

mockWindows("main");
mockIPC(
  (cmd) => {
    switch (cmd) {
      case "check_for_update":
        return null;
      case "get_app_state":
        return appState(scenario);
      case "list_projects":
        return [projectSummary(scenario)];
      case "get_project":
        return projectDetail(scenario);
      case "list_voices":
        return VOICES;
      case "get_tricky_words":
        return TRICKY;
      case "storage_size":
        return { books: 412_000_000, previews: 3_000_000 };
      case "project_cache_size":
        return 96_000_000;
      default:
        console.warn(`[mock] unhandled command ${cmd}`);
        return null;
    }
  },
  { shouldMockEvents: true },
);

// Only the Alice cover is ever a local image; serve it from the same origin (see capture.ts).
const internals = (window as unknown as { __TAURI_INTERNALS__: { convertFileSrc: () => string } })
  .__TAURI_INTERNALS__;
internals.convertFileSrc = () => "/__mock/cover";

(window as unknown as { __aloudMock: unknown }).__aloudMock = {
  emit: (event: string, payload: unknown) => emit(event, payload),
};
