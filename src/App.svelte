<script lang="ts">
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ModeWatcher, setMode } from "mode-watcher";
  import { onMount, untrack } from "svelte";
  import { toast } from "svelte-sonner";
  import { fade } from "svelte/transition";
  import { api, on } from "$lib/api";
  import BookScreen from "$lib/components/BookScreen.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import Header from "$lib/components/Header.svelte";
  import Library from "$lib/components/Library.svelte";
  import Onboarding from "$lib/components/Onboarding.svelte";
  import PlayerBar from "$lib/components/PlayerBar.svelte";
  import QueueSheet from "$lib/components/QueueSheet.svelte";
  import RenderPill from "$lib/components/RenderPill.svelte";
  import SettingsScreen from "$lib/components/SettingsScreen.svelte";
  import { Toaster } from "$lib/components/ui/sonner";
  import { dur } from "$lib/motion";
  import { app } from "$lib/stores/app.svelte";
  import { player } from "$lib/stores/player.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { attempt } from "$lib/toast";

  let dragging = $state(false);
  let quitOpen = $state(false);

  const canImport = $derived(app.ready && app.screen.name !== "onboarding");

  $effect(() => {
    if (!app.ready) return;
    const theme = app.settings.theme;
    // setMode reads mode-watcher state it then writes; tracking it re-runs this effect forever.
    untrack(() => setMode(theme));
  });

  onMount(() => {
    void app.boot().then(() => projects.refresh());

    const subscriptions = Promise.all([
      on.renderProgress((p) => render.setProgress(p)),
      on.queueUpdated((q) => {
        render.setQueue(q);
        void projects.refresh();
      }),
      on.projectUpdated(() => void projects.refresh()),
      on.modelDownload((p) => app.setDownloadProgress(p)),
      on.playerState((s) => (player.state = s)),
      on.notice(({ code }) => {
        if (code === "gpu-fallback") {
          toast.warning(
            "Graphics acceleration isn't available here — using the processor instead.",
          );
          void attempt(app.refresh());
        } else if (code === "no-audio-device") {
          toast.error("No speakers or headphones found");
        }
      }),
      on.confirmQuit(() => (quitOpen = true)),
      getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          dragging = canImport;
          return;
        }
        dragging = false;
        if (payload.type === "drop" && canImport) void projects.importPaths(payload.paths);
      }),
    ]);
    return () => void subscriptions.then((unlisten) => unlisten.forEach((fn) => fn()));
  });
</script>

<ModeWatcher />
<Toaster position="bottom-right" />

<div class="flex h-screen flex-col">
  <Header />

  <main class="min-h-0 flex-1 overflow-y-auto">
    {#if app.ready}
      {#key app.screen.name}
        <div in:fade={{ duration: dur(180) }}>
          {#if app.screen.name === "onboarding"}
            <Onboarding />
          {:else if app.screen.name === "library"}
            <Library />
          {:else if app.screen.name === "book"}
            <BookScreen id={app.screen.id} />
          {:else}
            <SettingsScreen />
          {/if}
        </div>
      {/key}
    {/if}
  </main>

  <RenderPill />
  <PlayerBar />
</div>

<QueueSheet />

<ConfirmDialog
  bind:open={quitOpen}
  title="Quit Aloud?"
  description="Aloud is still making your audiobook. Quit anyway? Finished parts are saved."
  confirmLabel="Quit"
  cancelLabel="Keep working"
  destructive
  onconfirm={() => attempt(api.quitApp())}
/>

{#if dragging}
  <div
    class="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-background/70 backdrop-blur-sm"
    transition:fade={{ duration: dur(150) }}
  >
    <div
      class="rounded-3xl border-2 border-dashed border-primary px-12 py-10 text-xl font-semibold tracking-tight"
    >
      Drop an EPUB to add it
    </div>
  </div>
{/if}
