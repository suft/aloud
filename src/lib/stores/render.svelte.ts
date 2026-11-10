import { attempt } from "$lib/toast";
import { api, type AppStateDto, type QueueState, type RenderProgress } from "$lib/api";

class RenderStore {
  queue = $state<QueueState>({ items: [], paused: false });
  /** Latest progress event per project id. */
  progress = $state<Record<string, RenderProgress>>({});
  /** True when the queue was already paused at launch, i.e. Aloud was closed mid-job. */
  restarted = $state(false);

  headId = $derived(this.queue.items[0] ?? null);
  running = $derived(this.queue.items.length > 0 && !this.queue.paused);
  headProgress = $derived(this.headId ? (this.progress[this.headId] ?? null) : null);

  boot(state: AppStateDto) {
    this.queue = state.queue;
    this.restarted = state.queue.paused && state.queue.items.length > 0;
    this.progress = state.active ? { [state.active.projectId]: state.active } : {};
  }

  setProgress(p: RenderProgress) {
    this.progress[p.projectId] = p;
  }

  setQueue(q: QueueState) {
    const before = new Set(this.queue.items);
    for (const id of Object.keys(this.progress)) {
      if (!q.items.includes(id) || !before.has(id)) delete this.progress[id];
    }
    this.queue = q;
    if (!q.paused || q.items.length === 0) this.restarted = false;
  }

  /** 0-based position in the queue, or -1. */
  position(id: string): number {
    return this.queue.items.indexOf(id);
  }

  /** Awaits a queue-changing command, adopting its result. Returns whether it succeeded. */
  async apply(request: Promise<QueueState>): Promise<boolean> {
    const next = await attempt(request);
    if (next) this.setQueue(next);
    return next !== undefined;
  }

  resume() {
    return this.apply(api.resumeQueue());
  }
}

export const render = new RenderStore();
