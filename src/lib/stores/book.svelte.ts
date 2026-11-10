import { toast } from "svelte-sonner";
import {
  api,
  errorMessage,
  type PronEntry,
  type PronScope,
  type ProjectDetail,
  type ProjectPatch,
  type TrickyWord,
} from "$lib/api";
import { app } from "./app.svelte";

/** State and actions for the book screen of one project. */
export class BookView {
  readonly id: string;
  detail = $state<ProjectDetail | null>(null);
  /** `null` until the book has been scanned. */
  tricky = $state<TrickyWord[] | null>(null);
  /** Bumped when the cover file is replaced so the webview reloads it. */
  coverVersion = $state(0);
  /** Why the project couldn't be loaded, if it couldn't. */
  loadError = $state<string | null>(null);
  #trickyLoading = false;
  /** Orders `load`, `patch` and `setCover`: only the latest response may replace `detail`. */
  #seq = 0;

  /** Output-changing controls are frozen while the job runs. */
  locked = $derived(
    this.detail?.project.status.state === "rendering" ||
      this.detail?.project.status.state === "exporting",
  );

  constructor(id: string) {
    this.id = id;
  }

  /**
   * Loads the project. After a failure it does nothing, so a book that can't be read isn't
   * requested again on every event; the screen shows `loadError` instead.
   */
  async load() {
    if (this.loadError !== null) return;
    const seq = ++this.#seq;
    try {
      const detail = await api.getProject(this.id);
      if (seq === this.#seq) this.detail = detail;
    } catch (e) {
      if (seq !== this.#seq) return;
      this.loadError = errorMessage(e);
      // Without a detail the screen shows the error itself.
      if (this.detail !== null) toast.error(this.loadError);
    }
  }

  /** Applies a change; shows any failure as a toast and returns whether it succeeded. */
  async patch(patch: ProjectPatch): Promise<boolean> {
    const seq = ++this.#seq;
    try {
      const detail = await api.updateProject(this.id, patch);
      if (seq === this.#seq) this.detail = detail;
      return true;
    } catch (e) {
      toast.error(errorMessage(e));
      return false;
    }
  }

  async setCover(path: string) {
    const seq = ++this.#seq;
    try {
      const detail = await api.setCover(this.id, path);
      if (seq === this.#seq) this.detail = detail;
      this.coverVersion++;
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async loadTricky() {
    if (this.tricky !== null || this.#trickyLoading) return;
    this.#trickyLoading = true;
    try {
      this.tricky = await api.getTrickyWords(this.id);
    } catch (e) {
      toast.error(errorMessage(e));
      this.tricky = [];
    } finally {
      this.#trickyLoading = false;
    }
  }

  async dismissWord(word: string) {
    try {
      await api.dismissWord(this.id, word);
      this.dropTricky(word);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  dropTricky(word: string) {
    this.tricky = this.tricky?.filter((t) => t.word !== word) ?? null;
  }

  /** Saves a pronunciation. Rejects with the server's validation message. */
  async savePronunciation(scope: PronScope, entry: PronEntry) {
    const list = await api.upsertPronunciation(scope, entry, this.id);
    if (scope === "global") app.settings = { ...app.settings, pronunciations: list };
    await this.load();
  }

  async removePronunciation(scope: PronScope, entryId: string) {
    const list = await api.deletePronunciation(scope, entryId, this.id);
    if (scope === "global") app.settings = { ...app.settings, pronunciations: list };
    await this.load();
  }
}
