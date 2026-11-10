import { toast } from "svelte-sonner";
import { api, errorMessage, type UpdateInfo, type UpdateProgress } from "$lib/api";
import { attempt } from "$lib/toast";

class UpdateStore {
  available = $state<UpdateInfo | null>(null);
  dialogOpen = $state(false);
  checking = $state(false);
  installing = $state(false);
  progress = $state<UpdateProgress | null>(null);

  /** Launch check; the backend applies the opt-out, 24h throttle and skipped version. */
  async checkInBackground() {
    try {
      this.available = await api.checkForUpdate(false);
    } catch {
      /* stay quiet at launch */
    }
  }

  async checkNow() {
    this.checking = true;
    try {
      this.available = await api.checkForUpdate(true);
      if (this.available) this.dialogOpen = true;
      else toast.success("Aloud is up to date.");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      this.checking = false;
    }
  }

  async skip() {
    const v = this.available?.version;
    if (!v) return;
    this.dialogOpen = false;
    this.available = null;
    await attempt(api.skipUpdate(v));
  }

  async install() {
    this.installing = true;
    this.progress = null;
    try {
      await api.installUpdate();
    } catch (e) {
      toast.error(errorMessage(e));
      this.installing = false;
    }
  }
}

export const update = new UpdateStore();
