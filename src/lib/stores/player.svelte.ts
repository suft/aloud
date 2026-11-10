import type { PlayerState } from "$lib/api";

class PlayerStore {
  state = $state<PlayerState | null>(null);

  /** A chapter (not a short voice preview) is loaded in the player bar. */
  loaded = $derived(!!this.state?.chapterId);
  /** A voice or pronunciation preview is currently audible. */
  previewPlaying = $derived(!!this.state?.playing && !this.state.chapterId);
}

export const player = new PlayerStore();
