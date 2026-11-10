import { toast } from "svelte-sonner";
import { api, errorMessage, type ProjectSummary } from "$lib/api";
import { app } from "./app.svelte";

class ProjectsStore {
  list = $state<ProjectSummary[]>([]);
  loaded = $state(false);
  /** Orders responses: only the latest request may update the list. */
  #seq = 0;
  /** A failure was already toasted; stays set until a refresh succeeds. */
  #failing = false;

  async refresh() {
    const seq = ++this.#seq;
    try {
      const list = await api.listProjects();
      if (seq !== this.#seq) return;
      this.list = list;
      this.#failing = false;
      this.loaded = true;
    } catch (e) {
      if (seq !== this.#seq) return;
      if (!this.#failing) toast.error(errorMessage(e));
      this.#failing = true;
      this.loaded = true;
    }
  }

  find(id: string | null | undefined): ProjectSummary | undefined {
    return id ? this.list.find((p) => p.id === id) : undefined;
  }

  /** Imports every .epub in `paths`, opens the last one, and toasts about anything else. */
  async importPaths(paths: string[]) {
    const epubs = paths.filter((p) => p.toLowerCase().endsWith(".epub"));
    if (epubs.length < paths.length) toast.error("Only EPUB files can be added.");
    if (epubs.length === 0) return;

    const busy = toast.loading(epubs.length > 1 ? "Adding books…" : "Adding book…");
    let last: ProjectSummary | null = null;
    for (const path of epubs) {
      try {
        last = await api.importEpub(path);
      } catch (e) {
        toast.error(errorMessage(e));
      }
    }
    toast.dismiss(busy);
    await this.refresh();
    if (last) app.go({ name: "book", id: last.id });
  }
}

export const projects = new ProjectsStore();
