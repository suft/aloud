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
  #trickyLoading = false;

  /** Output-changing controls are frozen while the job runs. */
  locked = $derived(
    this.detail?.project.status.state === "rendering" ||
      this.detail?.project.status.state === "exporting",
  );

  constructor(id: string) {
    this.id = id;
  }

  async load() {
    try {
      this.detail = await api.getProject(this.id);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  /** Applies a change; shows any failure as a toast and returns whether it succeeded. */
  async patch(patch: ProjectPatch): Promise<boolean> {
    try {
      this.detail = await api.updateProject(this.id, patch);
      return true;
    } catch (e) {
      toast.error(errorMessage(e));
      return false;
    }
  }

  async setCover(path: string) {
    try {
      this.detail = await api.setCover(this.id, path);
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
