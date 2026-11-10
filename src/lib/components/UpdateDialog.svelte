<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Progress } from "$lib/components/ui/progress";
  import { formatBytes } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { update } from "$lib/stores/update.svelte";
</script>

{#if update.available}
  <Dialog.Root bind:open={update.dialogOpen}>
    <Dialog.Content showCloseButton={false}>
      <Dialog.Header>
        <Dialog.Title>Aloud {update.available.version} is available</Dialog.Title>
        <Dialog.Description>You have {app.version}.</Dialog.Description>
      </Dialog.Header>
      {#if update.available.notes?.trim()}
        <div class="max-h-60 overflow-y-auto text-sm whitespace-pre-wrap text-muted-foreground">
          {update.available.notes}
        </div>
      {/if}
      {#if update.installing}
        {@const downloaded = update.progress?.downloaded ?? 0}
        {@const total = update.progress?.total ?? null}
        <div class="space-y-2">
          <Progress value={downloaded} max={total ?? 100} aria-label="Update download" />
          <p class="text-sm text-muted-foreground">
            {#if total}
              Downloading… {formatBytes(downloaded)} of {formatBytes(total)}
            {:else}
              Downloading…
            {/if}
          </p>
        </div>
      {:else}
        {#if render.running}
          <p class="text-sm text-muted-foreground">Pause the queue first to install.</p>
        {/if}
        <Dialog.Footer>
          <Button variant="ghost" onclick={() => update.skip()}>Skip this version</Button>
          <Button variant="ghost" onclick={() => (update.dialogOpen = false)}>Later</Button>
          <Button disabled={render.running} onclick={() => update.install()}>
            Install and restart
          </Button>
        </Dialog.Footer>
      {/if}
    </Dialog.Content>
  </Dialog.Root>
{/if}
