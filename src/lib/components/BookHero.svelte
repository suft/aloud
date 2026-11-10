<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Loader from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, imageSrc } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";
  import { ACCENT_LABEL, GENDER_LABEL, formatDuration, parseAuthors } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";
  import type { BookView } from "$lib/stores/book.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { attempt } from "$lib/toast";
  import Cover from "./Cover.svelte";
  import EditableText from "./EditableText.svelte";
  import VoiceAvatar from "./VoiceAvatar.svelte";
  import VoicePicker from "./VoicePicker.svelte";

  let { book }: { book: BookView } = $props();

  let pickerOpen = $state(false);
  let previewing = $state(false);
  let draggingSpeed = $state<number | null>(null);

  const detail = $derived(book.detail!);
  const project = $derived(detail.project);
  const status = $derived(project.status);
  const voice = $derived(app.voices.find((v) => v.id === project.voice));
  const speed = $derived(draggingSpeed ?? project.speed);
  const selected = $derived(detail.chapters.filter((c) => c.included));
  const selectedSecs = $derived(selected.reduce((sum, c) => sum + c.estSecs, 0));

  const actionLabel = $derived.by(() => {
    switch (status.state) {
      case "draft":
      case "failed":
        return "Make audiobook";
      case "paused":
        return "Continue";
      case "done":
        return `Update audiobook · ${Math.ceil(detail.redoFraction * 100)}% to redo`;
      default:
        return null;
    }
  });

  $effect(() => {
    void app.loadVoices();
  });

  async function preview() {
    previewing = true;
    await attempt(api.previewVoice({ projectId: book.id, voice: project.voice, speed }));
    previewing = false;
  }

  async function commitSpeed(value: number) {
    await book.patch({ speed: Math.round(value * 100) / 100 });
    draggingSpeed = null;
  }

  async function enqueue() {
    const queued = await render.apply(api.enqueue(book.id));
    if (queued && status.state === "paused") await render.resume();
  }
</script>

<section class="relative isolate overflow-hidden">
  {#if detail.coverPath}
    <img
      src={imageSrc(detail.coverPath, book.coverVersion)}
      alt=""
      crossorigin="anonymous"
      class="absolute inset-0 -z-20 size-full scale-125 object-cover opacity-40 blur-3xl"
    />
  {:else}
    <div
      class="absolute inset-0 -z-20 opacity-40"
      style:background="linear-gradient(160deg, var(--book-accent), transparent)"
    ></div>
  {/if}
  <div
    class="absolute inset-0 -z-10 bg-gradient-to-b from-transparent via-background/30 to-background"
  ></div>

  <div class="mx-auto max-w-4xl px-8 pt-3 pb-8">
    <Button
      variant="ghost"
      size="sm"
      class="mb-3 -ml-3"
      onclick={() => app.go({ name: "library" })}
    >
      <ArrowLeft /> Your books
    </Button>

    <div class="flex min-h-[280px] gap-8">
      <div class="w-[180px] shrink-0">
        <div class="aspect-[2/3] overflow-hidden rounded-xl shadow-xl ring-1 ring-black/10">
          <Cover
            path={detail.coverPath}
            title={project.meta.title}
            accent={project.meta.accent}
            version={book.coverVersion}
          />
        </div>
      </div>

      <div class="min-w-0 flex-1 space-y-4">
        <div class="space-y-1">
          <EditableText
            value={project.meta.title}
            disabled={book.locked}
            placeholder="Untitled"
            class="block text-3xl font-semibold tracking-tight"
            onsave={(title) => title && book.patch({ meta: { title } })}
          />
          <EditableText
            value={project.meta.authors.join(", ")}
            disabled={book.locked}
            placeholder="Add author"
            class="block text-base text-muted-foreground"
            onsave={(text) => book.patch({ meta: { authors: parseAuthors(text) } })}
          />
        </div>

        <p class="text-sm text-muted-foreground">
          {selected.length} chapter{selected.length === 1 ? "" : "s"} selected · about {formatDuration(
            selectedSecs,
          )} · {detail.wordCount.toLocaleString()} words
        </p>

        <div class="flex items-center gap-3">
          <VoiceAvatar voice={project.voice} name={voice?.name ?? project.voice} />
          <div class="min-w-0 text-sm">
            {#if voice}
              <span class="font-medium">{voice.name}</span>
              <span class="text-muted-foreground">
                · {ACCENT_LABEL[voice.accent]} · {GENDER_LABEL[voice.gender]}
              </span>
            {:else}
              <span class="font-medium">{project.voice}</span>
            {/if}
          </div>
          <Button
            variant="outline"
            size="icon-sm"
            class="rounded-full"
            aria-label="Hear this voice"
            disabled={previewing}
            onclick={preview}
          >
            {#if previewing}<Loader class="animate-spin" />{:else}<Play />{/if}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={book.locked}
            onclick={() => (pickerOpen = true)}
          >
            Change voice
          </Button>
        </div>

        <div class="max-w-sm space-y-2">
          <div class="text-sm text-muted-foreground">Reading speed {speed.toFixed(2)}×</div>
          <Slider
            type="single"
            value={speed}
            min={0.8}
            max={1.3}
            step={0.05}
            disabled={book.locked}
            aria-label="Reading speed"
            onValueChange={(v) => (draggingSpeed = v)}
            onValueCommit={commitSpeed}
          />
        </div>

        {#if book.locked}
          <p class="text-xs text-muted-foreground">Pause first to change this.</p>
        {/if}

        <div class="flex flex-wrap items-center gap-3 pt-1">
          {#if status.state === "done" && !detail.stale}
            <Button
              size="lg"
              class="h-11 rounded-full bg-book px-6 text-white hover:bg-book/90"
              onclick={() => attempt(openPath(status.output))}
            >
              Open audiobook
            </Button>
            <Button
              variant="outline"
              size="lg"
              class="h-11 rounded-full px-5"
              onclick={() => attempt(revealItemInDir(status.output))}
            >
              Show in folder
            </Button>
          {:else if actionLabel}
            <Button
              size="lg"
              class="h-11 rounded-full bg-book px-6 text-white hover:bg-book/90"
              onclick={enqueue}
            >
              {actionLabel}
            </Button>
          {/if}
        </div>
      </div>
    </div>
  </div>
</section>

<VoicePicker {book} bind:open={pickerOpen} />
