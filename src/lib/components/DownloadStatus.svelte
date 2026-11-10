<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Progress } from "$lib/components/ui/progress";
  import { formatBytes, formatRemaining, formatSpeed } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";

  let { onretry }: { onretry: () => void } = $props();

  const progress = $derived(app.download.progress);
  const summary = $derived.by(() => {
    if (!progress) return "Starting download…";
    const parts = [`${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)}`];
    if (progress.bytesPerSec > 0) {
      const left = (progress.total - progress.downloaded) / progress.bytesPerSec;
      parts.push(formatSpeed(progress.bytesPerSec), `about ${formatRemaining(left)} left`);
    }
    return parts.join(" · ");
  });
</script>

{#if app.download.error}
  <div class="space-y-3 rounded-xl bg-destructive/10 p-4 text-sm text-destructive">
    <p>{app.download.error}</p>
    <Button variant="outline" size="sm" onclick={onretry}>Try again</Button>
  </div>
{:else}
  <div class="space-y-2">
    <Progress value={progress?.downloaded ?? 0} max={progress?.total || 1} />
    <div class="flex items-center justify-between gap-3 text-sm text-muted-foreground">
      <span>{summary}</span>
      <Button variant="ghost" size="sm" onclick={() => app.cancelDownload()}>Cancel</Button>
    </div>
  </div>
{/if}
