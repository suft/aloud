<script lang="ts">
  import { api } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { app } from "$lib/stores/app.svelte";
  import { attempt } from "$lib/toast";

  let { onretry }: { onretry: () => Promise<void> } = $props();

  let retrying = $state(false);

  async function retry() {
    retrying = true;
    try {
      await onretry();
    } finally {
      retrying = false;
    }
  }
</script>

<div
  class="mx-auto flex min-h-[420px] max-w-md flex-col items-center justify-center gap-4 p-8 text-center"
>
  <div class="text-xl font-semibold tracking-tight">Aloud couldn't start</div>
  <p class="text-sm text-muted-foreground">{app.bootError}</p>
  <div class="flex gap-2">
    <Button disabled={retrying} onclick={retry}>{retrying ? "Trying…" : "Try again"}</Button>
    <Button variant="outline" onclick={() => attempt(api.openDataDir())}>Open data folder</Button>
  </div>
</div>
