import { toast } from "svelte-sonner";
import {
  api,
  errorMessage,
  type AppStateDto,
  type DownloadProgress,
  type ModelsStatus,
  type ModelVariant,
  type Settings,
  type SettingsPatch,
  type SpeedRange,
  type VoiceInfo,
} from "$lib/api";
import { render } from "./render.svelte";

export type Screen =
  | { name: "onboarding" }
  | { name: "library" }
  | { name: "book"; id: string }
  | { name: "settings" };

interface DownloadState {
  variant: ModelVariant | null;
  progress: DownloadProgress | null;
  error: string | null;
}

const STARTUP_NOTICES: Record<string, string> = {
  "settings-reset":
    "Aloud's settings couldn't be read, so they were reset. The old file was kept as settings.json.bak.",
  "queue-reset":
    "The list of books to make couldn't be read, so it was cleared. The old file was kept as queue.json.bak.",
};

class AppStore {
  ready = $state(false);
  bootError = $state<string | null>(null);
  #settings = $state<Settings | null>(null);
  #speedRange = $state<SpeedRange | null>(null);
  models = $state<ModelsStatus>({ standard: false, fast: false, voices: false });
  gpuSupported = $state(false);
  version = $state("");
  screen = $state<Screen>({ name: "library" });
  queueOpen = $state(false);
  voices = $state<VoiceInfo[]>([]);
  download = $state<DownloadState>({ variant: null, progress: null, error: null });
  #cancelled = false;
  #settingsSeq = 0;
  #settingsChain: Promise<unknown> = Promise.resolve();

  /** Valid once `ready` is true. */
  get settings(): Settings {
    if (!this.#settings) throw new Error("settings are not loaded yet");
    return this.#settings;
  }

  /** Valid once `ready` is true. */
  get speedRange(): SpeedRange {
    if (!this.#speedRange) throw new Error("speed range is not loaded yet");
    return this.#speedRange;
  }

  set settings(value: Settings) {
    this.#settingsSeq++;
    this.#settings = value;
  }

  downloading = $derived(this.download.variant !== null && this.download.error === null);

  /** Loads app state; on failure keeps the message in `bootError` and returns false. */
  async boot(): Promise<boolean> {
    this.bootError = null;
    let notices: string[];
    try {
      const state = await this.refresh();
      render.boot(state);
      notices = state.startupNotices;
    } catch (e) {
      this.bootError = errorMessage(e);
      return false;
    }
    const m = this.models;
    this.screen = (m.standard || m.fast) && m.voices ? { name: "library" } : { name: "onboarding" };
    this.ready = true;
    for (const code of notices) {
      const text = STARTUP_NOTICES[code];
      if (text) toast.warning(text);
    }
    return true;
  }

  /** Re-reads settings and voice-pack status from the backend. */
  async refresh(): Promise<AppStateDto> {
    const seq = this.#settingsSeq;
    const state = await api.getAppState();
    // A settings write since the request started makes this copy stale.
    if (seq === this.#settingsSeq) this.#settings = state.settings;
    this.#speedRange = state.speedRange;
    this.models = state.models;
    this.gpuSupported = state.gpuSupported;
    this.version = state.version;
    return state;
  }

  go(screen: Screen) {
    this.screen = screen;
  }

  /** Writes are chained: the backend may run concurrent writes out of order. */
  updateSettings(patch: SettingsPatch): Promise<void> {
    const run = this.#settingsChain.then(() => this.#applySettings(patch));
    this.#settingsChain = run.catch(() => {});
    return run;
  }

  async #applySettings(patch: SettingsPatch) {
    this.#settingsSeq++;
    try {
      const settings = await api.updateSettings(patch);
      this.#settingsSeq++;
      this.#settings = settings;
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async loadVoices() {
    if (this.voices.length > 0) return;
    try {
      this.voices = await api.listVoices();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  /** Downloads a voice pack and makes it the selected one. Returns true on success. */
  async startDownload(variant: ModelVariant): Promise<boolean> {
    this.#cancelled = false;
    this.download = { variant, progress: null, error: null };
    try {
      await api.downloadModels(variant);
      await this.refresh();
      await this.updateSettings({ model: variant });
      this.download = { variant: null, progress: null, error: null };
      return true;
    } catch (e) {
      this.download = {
        variant: this.#cancelled ? null : variant,
        progress: null,
        error: this.#cancelled ? null : errorMessage(e),
      };
      return false;
    }
  }

  setDownloadProgress(progress: DownloadProgress) {
    this.download.progress = progress;
  }

  async cancelDownload() {
    this.#cancelled = true;
    try {
      await api.cancelDownload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async removeModel(variant: ModelVariant) {
    try {
      this.models = await api.removeModel(variant);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }
}

export const app = new AppStore();
