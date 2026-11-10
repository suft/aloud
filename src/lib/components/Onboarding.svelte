<script lang="ts">
  import AudioLines from "@lucide/svelte/icons/audio-lines";
  import type { ModelVariant } from "$lib/api";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as RadioGroup from "$lib/components/ui/radio-group";
  import { MODEL_INFO, MODEL_VARIANTS } from "$lib/models";
  import { app } from "$lib/stores/app.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { cn } from "$lib/utils";
  import DownloadStatus from "./DownloadStatus.svelte";

  let variant = $state<ModelVariant>("standard");

  async function start() {
    if (await app.startDownload(variant)) {
      await projects.refresh();
      app.go({ name: "library" });
    }
  }
</script>

<div class="flex min-h-full items-center justify-center p-6">
  <div
    class="w-full max-w-[560px] space-y-7 rounded-2xl border border-border/60 bg-card/80 p-8 shadow-sm backdrop-blur"
  >
    <div class="space-y-3 text-center">
      <div
        class="mx-auto grid size-14 place-items-center rounded-2xl bg-gradient-to-br from-[#7c5cff] to-[#4f46e5] text-white shadow-md"
      >
        <AudioLines class="size-7" />
      </div>
      <h1 class="text-2xl font-semibold tracking-tight">Turn any EPUB into an audiobook</h1>
      <p class="text-sm text-muted-foreground">
        Everything happens on your computer. Nothing is uploaded.
      </p>
    </div>

    <RadioGroup.Root bind:value={variant} disabled={app.downloading} class="gap-3">
      {#each MODEL_VARIANTS as v (v)}
        {@const info = MODEL_INFO[v]}
        <label
          class={cn(
            "flex cursor-pointer items-start gap-3 rounded-xl border border-border/60 p-4 transition-colors hover:bg-muted/50",
            variant === v && "border-primary bg-primary/5",
            app.downloading && "cursor-default opacity-70",
          )}
        >
          <RadioGroup.Item value={v} class="mt-0.5" />
          <div class="space-y-1">
            <div class="flex items-center gap-2 font-medium">
              {info.name}
              {#if info.recommended}<Badge>Recommended</Badge>{/if}
            </div>
            <p class="text-sm text-muted-foreground">{info.blurb} · {info.size} download</p>
          </div>
        </label>
      {/each}
    </RadioGroup.Root>

    {#if app.download.variant}
      <DownloadStatus onretry={start} />
    {:else}
      <Button size="lg" class="h-11 w-full rounded-full" onclick={start}>
        Download and continue
      </Button>
    {/if}
  </div>
</div>
