<script lang="ts">
  import ListMusic from "@lucide/svelte/icons/list-music";
  import CircleArrowDown from "@lucide/svelte/icons/circle-arrow-down";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import { Button } from "$lib/components/ui/button";
  import { app } from "$lib/stores/app.svelte";
  import { update } from "$lib/stores/update.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { cn } from "$lib/utils";

  const isMac = navigator.userAgent.includes("Mac");
  const RADIUS = 15;
  const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

  const onboarding = $derived(app.screen.name === "onboarding");
  const fraction = $derived(render.headProgress?.fraction ?? 0);
</script>

<header
  data-tauri-drag-region
  class={cn(
    "flex h-12 shrink-0 items-center justify-between border-b border-border/40 pr-3 pl-5",
    isMac && "pl-20",
  )}
>
  <button
    type="button"
    class="text-lg font-semibold tracking-tight disabled:cursor-default"
    disabled={onboarding}
    onclick={() => app.go({ name: "library" })}
  >
    aloud
  </button>

  {#if app.ready && !onboarding}
    <div class="flex items-center gap-1">
      {#if update.available}
        <Button
          variant="outline"
          size="sm"
          title="Aloud {update.available.version} is available"
          onclick={() => (update.dialogOpen = true)}
        >
          <CircleArrowDown /> Update
        </Button>
      {/if}
      <Button
        variant="ghost"
        size="icon"
        class="relative"
        aria-label="Queue"
        title="Queue"
        onclick={() => (app.queueOpen = true)}
      >
        {#if render.queue.items.length > 0}
          <svg
            class="absolute size-8 -rotate-90"
            viewBox="0 0 36 36"
            fill="none"
            aria-hidden="true"
          >
            <circle cx="18" cy="18" r={RADIUS} class="stroke-border" stroke-width="2.5" />
            <circle
              cx="18"
              cy="18"
              r={RADIUS}
              class={render.running ? "stroke-primary" : "stroke-muted-foreground"}
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-dasharray={CIRCUMFERENCE}
              stroke-dashoffset={CIRCUMFERENCE * (1 - fraction)}
              style="transition: stroke-dashoffset 250ms linear"
            />
          </svg>
        {/if}
        <ListMusic />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Settings"
        title="Settings"
        onclick={() => app.go({ name: "settings" })}
      >
        <SettingsIcon />
      </Button>
    </div>
  {/if}
</header>
