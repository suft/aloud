import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AboutDoc } from "./bindings/AboutDoc";
import type { AppStateDto } from "./bindings/AppStateDto";
import type { DownloadProgress } from "./bindings/DownloadProgress";
import type { ModelVariant } from "./bindings/ModelVariant";
import type { ModelsStatus } from "./bindings/ModelsStatus";
import type { PlayerState } from "./bindings/PlayerState";
import type { ProjectDetail } from "./bindings/ProjectDetail";
import type { ProjectPatch } from "./bindings/ProjectPatch";
import type { ProjectSummary } from "./bindings/ProjectSummary";
import type { PronEntry } from "./bindings/PronEntry";
import type { PronScope } from "./bindings/PronScope";
import type { QueueState } from "./bindings/QueueState";
import type { RenderProgress } from "./bindings/RenderProgress";
import type { Settings } from "./bindings/Settings";
import type { SettingsPatch } from "./bindings/SettingsPatch";
import type { StorageSize } from "./bindings/StorageSize";
import type { TrickyWord } from "./bindings/TrickyWord";
import type { UpdateInfo } from "./bindings/UpdateInfo";
import type { UpdateProgress } from "./bindings/UpdateProgress";
import type { VoiceInfo } from "./bindings/VoiceInfo";

export type { AboutDoc } from "./bindings/AboutDoc";
export type { AppStateDto } from "./bindings/AppStateDto";
export type { BookMeta } from "./bindings/BookMeta";
export type { ChapterDetail } from "./bindings/ChapterDetail";
export type { ChapterKind } from "./bindings/ChapterKind";
export type { ChapterPatch } from "./bindings/ChapterPatch";
export type { ChapterProgress } from "./bindings/ChapterProgress";
export type { ChapterSel } from "./bindings/ChapterSel";
export type { DownloadProgress } from "./bindings/DownloadProgress";
export type { MetaPatch } from "./bindings/MetaPatch";
export type { ModelVariant } from "./bindings/ModelVariant";
export type { ModelsStatus } from "./bindings/ModelsStatus";
export type { Performance } from "./bindings/Performance";
export type { Phase } from "./bindings/Phase";
export type { PlayerState } from "./bindings/PlayerState";
export type { Project } from "./bindings/Project";
export type { ProjectDetail } from "./bindings/ProjectDetail";
export type { ProjectPatch } from "./bindings/ProjectPatch";
export type { ProjectStatus } from "./bindings/ProjectStatus";
export type { ProjectSummary } from "./bindings/ProjectSummary";
export type { PronEntry } from "./bindings/PronEntry";
export type { PronScope } from "./bindings/PronScope";
export type { QueueState } from "./bindings/QueueState";
export type { RenderProgress } from "./bindings/RenderProgress";
export type { SayAs } from "./bindings/SayAs";
export type { Settings } from "./bindings/Settings";
export type { SettingsPatch } from "./bindings/SettingsPatch";
export type { SpeedRange } from "./bindings/SpeedRange";
export type { StorageSize } from "./bindings/StorageSize";
export type { Theme } from "./bindings/Theme";
export type { TrickyWord } from "./bindings/TrickyWord";
export type { UpdateInfo } from "./bindings/UpdateInfo";
export type { UpdateProgress } from "./bindings/UpdateProgress";
export type { VoiceInfo } from "./bindings/VoiceInfo";

/** Pull a user-facing message out of whatever `invoke` rejected with. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e && typeof e.message === "string") {
    return e.message;
  }
  return String(e);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(errorMessage(e));
  }
}
export const api = {
  getAppState: () => call<AppStateDto>("get_app_state"),
  updateSettings: (patch: SettingsPatch) => call<Settings>("update_settings", { patch }),
  downloadModels: (variant: ModelVariant) => call<void>("download_models", { variant }),
  cancelDownload: () => call<void>("cancel_download"),
  removeModel: (variant: ModelVariant) => call<ModelsStatus>("remove_model", { variant }),

  listProjects: () => call<ProjectSummary[]>("list_projects"),
  importEpub: (path: string) => call<ProjectSummary>("import_epub", { path }),
  getProject: (id: string) => call<ProjectDetail>("get_project", { id }),
  updateProject: (id: string, patch: ProjectPatch) =>
    call<ProjectDetail>("update_project", { id, patch }),
  setCover: (id: string, path: string) => call<ProjectDetail>("set_cover", { id, path }),
  deleteProject: (id: string) => call<void>("delete_project", { id }),

  listVoices: () => call<VoiceInfo[]>("list_voices"),
  previewVoice: (args: { projectId?: string; voice: string; speed: number; text?: string }) =>
    call<void>("preview_voice", args),

  getTrickyWords: (id: string) => call<TrickyWord[]>("get_tricky_words", { id }),
  dismissWord: (id: string, word: string) => call<void>("dismiss_word", { id, word }),
  upsertPronunciation: (scope: PronScope, entry: PronEntry, projectId?: string) =>
    call<PronEntry[]>("upsert_pronunciation", { scope, projectId, entry }),
  deletePronunciation: (scope: PronScope, entryId: string, projectId?: string) =>
    call<PronEntry[]>("delete_pronunciation", { scope, projectId, entryId }),
  movePronunciation: (projectId: string, entryId: string, to: PronScope) =>
    call<PronEntry[]>("move_pronunciation", { projectId, entryId, to }),
  testPronunciation: (projectId: string, text: string) =>
    call<void>("test_pronunciation", { projectId, text }),

  enqueue: (id: string) => call<QueueState>("enqueue", { id }),
  cancelRender: (id: string) => call<QueueState>("cancel_render", { id }),
  pauseQueue: () => call<QueueState>("pause_queue"),
  resumeQueue: () => call<QueueState>("resume_queue"),
  moveInQueue: (id: string, direction: "up" | "down") =>
    call<QueueState>("move_in_queue", { id, direction }),

  playerPlayChapter: (projectId: string, chapterId: string, startMs: number) =>
    call<void>("player_play_chapter", { projectId, chapterId, startMs }),
  playerToggle: () => call<void>("player_toggle"),
  playerStop: () => call<void>("player_stop"),
  playerSeek: (ms: number) => call<void>("player_seek", { ms }),

  projectCacheSize: (id: string) => call<number>("project_cache_size", { id }),
  clearProjectCache: (id: string) => call<void>("clear_project_cache", { id }),
  storageSize: () => call<StorageSize>("storage_size"),
  clearPreviews: () => call<void>("clear_previews"),
  openDataDir: () => call<void>("open_data_dir"),
  openAbout: (doc: AboutDoc) => call<void>("open_about", { doc }),
  openOutput: (id: string) => call<void>("open_output", { id }),

  checkForUpdate: (force: boolean) => call<UpdateInfo | null>("check_for_update", { force }),
  skipUpdate: (version: string) => call<void>("skip_update", { version }),
  installUpdate: () => call<void>("install_update"),
  quitApp: () => call<void>("quit_app"),
};

function subscribe<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export const on = {
  renderProgress: (cb: (p: RenderProgress) => void) =>
    subscribe<RenderProgress>("render-progress", cb),
  projectUpdated: (cb: (p: { id: string }) => void) => subscribe("project-updated", cb),
  queueUpdated: (cb: (q: QueueState) => void) => subscribe("queue-updated", cb),
  modelDownload: (cb: (p: DownloadProgress) => void) => subscribe("model-download", cb),
  playerState: (cb: (s: PlayerState) => void) => subscribe("player-state", cb),
  notice: (cb: (n: { code: string }) => void) => subscribe("notice", cb),
  updateProgress: (cb: (p: UpdateProgress) => void) =>
    subscribe<UpdateProgress>("update-progress", cb),
  confirmQuit: (cb: () => void) => subscribe<null>("confirm-quit", () => cb()),
};

/** Webview URL for a local image. `version` busts the cache after the file is replaced. */
export function imageSrc(path: string, version?: number): string {
  const src = convertFileSrc(path);
  return version === undefined ? src : `${src}?v=${version}`;
}
