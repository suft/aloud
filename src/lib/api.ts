import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ModelVariant = "standard" | "fast";
export type Performance = "full" | "balanced";
export type Theme = "system" | "light" | "dark";

export interface PronEntry {
  id: string;
  word: string;
  sayAs: { kind: "text" | "phonemes"; value: string };
  matchCase: boolean;
}

export interface Settings {
  model: ModelVariant;
  outputDir: string;
  performance: Performance;
  gpu: boolean;
  keepAwake: boolean;
  notifyOnFinish: boolean;
  checkUpdates: boolean;
  defaultVoice: string;
  defaultSpeed: number;
  pronunciations: PronEntry[];
  theme: Theme;
}
export type SettingsPatch = Partial<Settings>;

export interface UpdateInfo {
  version: string;
  notes: string | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export interface ModelsStatus {
  standard: boolean;
  fast: boolean;
  voices: boolean;
}

export interface DownloadProgress {
  file: string;
  downloaded: number;
  total: number;
  bytesPerSec: number;
}

export interface QueueState {
  /** Project ids; the head is the active one. */
  items: string[];
  paused: boolean;
}

export type Phase = "planning" | "rendering" | "exporting";

export interface RenderProgress {
  projectId: string;
  phase: Phase;
  doneUnits: number;
  totalUnits: number;
  fraction: number;
  chapters: { id: string; doneUnits: number; totalUnits: number }[];
  etaSecs: number | null;
  realtimeFactor: number | null;
}

export type ProjectStatus =
  | { state: "draft" }
  | { state: "queued" }
  | { state: "rendering" }
  | { state: "paused" }
  | { state: "exporting" }
  | { state: "done"; output: string; finishedAt: number; inputsHash: string }
  | { state: "failed"; message: string };

export interface ProjectSummary {
  id: string;
  title: string;
  authors: string[];
  coverPath: string | null;
  /** "#rrggbb" */
  accent: string | null;
  status: ProjectStatus;
  /** Fraction (0..1) of included audio already rendered. */
  progress: number;
  updatedAt: number;
}

export interface BookMeta {
  title: string;
  authors: string[];
  language: string | null;
  publisher: string | null;
  date: string | null;
  description: string | null;
  coverFile: string | null;
  accent: string | null;
}

export interface Project {
  id: string;
  sourceFileName: string;
  createdAt: number;
  updatedAt: number;
  meta: BookMeta;
  chapters: { id: string; title: string; included: boolean }[];
  voice: string;
  speed: number;
  intro: boolean;
  pronunciations: PronEntry[];
  dismissedWords: string[];
  status: ProjectStatus;
}

export type ChapterKind = "body" | "frontMatter" | "backMatter";

export interface ChapterDetail {
  id: string;
  title: string;
  kind: ChapterKind;
  included: boolean;
  words: number;
  estSecs: number;
  renderedFraction: number;
  playable: boolean;
}

export interface ProjectDetail {
  project: Project;
  coverPath: string | null;
  wordCount: number;
  estimatedSecs: number;
  chapters: ChapterDetail[];
  stale: boolean;
  /** 0..1 */
  redoFraction: number;
  /** Preview of the exported file name. */
  outputFileName: string;
}

export interface ProjectPatch {
  meta?: {
    title?: string;
    authors?: string[];
    date?: string | null;
    description?: string | null;
  };
  voice?: string;
  speed?: number;
  intro?: boolean;
  chapters?: { id: string; title?: string; included?: boolean }[];
}

export interface VoiceInfo {
  id: string;
  name: string;
  accent: "us" | "uk";
  gender: "female" | "male";
  grade: string;
  recommended: boolean;
}

export interface TrickyWord {
  word: string;
  count: number;
  sample: string;
}

export interface PlayerState {
  projectId?: string | null;
  chapterId?: string | null;
  title: string;
  positionMs: number;
  durationMs: number;
  playing: boolean;
}

export interface AppStateDto {
  settings: Settings;
  models: ModelsStatus;
  queue: QueueState;
  active: RenderProgress | null;
  gpuSupported: boolean;
  version: string;
}

export type PronScope = "book" | "global";

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
  storageSize: () => call<number>("storage_size"),
  openDataDir: () => call<void>("open_data_dir"),
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
  modelDownloadDone: (cb: (p: { ok: boolean; message?: string }) => void) =>
    subscribe("model-download-done", cb),
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
