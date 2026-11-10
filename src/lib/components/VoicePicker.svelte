<script lang="ts">
  import Loader from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import Star from "@lucide/svelte/icons/star";
  import { toast } from "svelte-sonner";
  import { api, type VoiceInfo } from "$lib/api";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as RadioGroup from "$lib/components/ui/radio-group";
  import * as Sheet from "$lib/components/ui/sheet";
  import { Textarea } from "$lib/components/ui/textarea";
  import * as ToggleGroup from "$lib/components/ui/toggle-group";
  import { ACCENT_LABEL, GENDER_LABEL } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";
  import type { BookView } from "$lib/stores/book.svelte";
  import { player } from "$lib/stores/player.svelte";
  import { attempt } from "$lib/toast";
  import { cn } from "$lib/utils";
  import VoiceAvatar from "./VoiceAvatar.svelte";

  const SAMPLE_TEXT =
    "Once upon a time, in a quiet town by the sea, there lived a girl who loved stories more than anything in the world.";
  const GRADE_ORDER = [
    "A+",
    "A",
    "A-",
    "B+",
    "B",
    "B-",
    "C+",
    "C",
    "C-",
    "D+",
    "D",
    "D-",
    "F+",
    "F",
  ];

  let { book, open = $bindable(false) }: { book: BookView; open?: boolean } = $props();

  let text = $state(SAMPLE_TEXT);
  let gender = $state("all");
  let accent = $state("all");
  let pendingId = $state<string | null>(null);
  let lastPreviewId = $state<string | null>(null);

  const project = $derived(book.detail!.project);

  const rank = (v: VoiceInfo) => {
    const i = GRADE_ORDER.indexOf(v.grade);
    return i < 0 ? GRADE_ORDER.length : i;
  };

  const visible = $derived(
    app.voices
      .filter(
        (v) =>
          (!gender || gender === "all" || v.gender === gender) &&
          (!accent || accent === "all" || v.accent === accent),
      )
      .sort((a, b) => Number(b.recommended) - Number(a.recommended) || rank(a) - rank(b)),
  );

  async function preview(voice: VoiceInfo) {
    pendingId = voice.id;
    await attempt(
      api.previewVoice({
        projectId: book.id,
        voice: voice.id,
        speed: project.speed,
        text: text.trim() || undefined,
      }),
    );
    lastPreviewId = voice.id;
    pendingId = null;
  }

  async function makeDefault() {
    await app.updateSettings({ defaultVoice: project.voice });
    toast.success("Saved — new books will use this voice.");
  }
</script>

<Sheet.Root bind:open>
  <Sheet.Content class="data-[side=right]:sm:max-w-[520px]">
    <Sheet.Header>
      <Sheet.Title>Choose a voice</Sheet.Title>
      <Sheet.Description>Tap play to hear each one reading your text.</Sheet.Description>
    </Sheet.Header>

    <div class="space-y-3 px-4">
      <label class="block space-y-1.5 text-sm font-medium">
        Preview text
        <Textarea bind:value={text} maxlength={400} rows={3} class="font-normal" />
      </label>
      <div class="flex flex-wrap gap-3">
        <ToggleGroup.Root type="single" variant="outline" size="sm" bind:value={gender}>
          <ToggleGroup.Item value="all">All</ToggleGroup.Item>
          <ToggleGroup.Item value="female">Female</ToggleGroup.Item>
          <ToggleGroup.Item value="male">Male</ToggleGroup.Item>
        </ToggleGroup.Root>
        <ToggleGroup.Root type="single" variant="outline" size="sm" bind:value={accent}>
          <ToggleGroup.Item value="all">All</ToggleGroup.Item>
          <ToggleGroup.Item value="us">American</ToggleGroup.Item>
          <ToggleGroup.Item value="uk">British</ToggleGroup.Item>
        </ToggleGroup.Root>
      </div>
    </div>

    <RadioGroup.Root
      value={project.voice}
      disabled={book.locked}
      onValueChange={(voice) => voice && book.patch({ voice })}
      class="flex-1 gap-2 overflow-y-auto px-4"
    >
      {#each visible as voice (voice.id)}
        <div
          class={cn(
            "flex items-center gap-3 rounded-xl border border-border/60 p-3",
            project.voice === voice.id && "border-primary bg-primary/5",
          )}
        >
          <RadioGroup.Item value={voice.id} aria-label={`Use ${voice.name}`} />
          <VoiceAvatar voice={voice.id} name={voice.name} class="size-9" />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="font-medium">{voice.name}</span>
              <Badge variant="secondary">{voice.grade}</Badge>
              {#if voice.recommended}
                <span class="flex items-center gap-1 text-xs text-amber-600 dark:text-amber-400">
                  <Star class="size-3 fill-current" /> Recommended
                </span>
              {/if}
            </div>
            <div class="text-xs text-muted-foreground">
              {ACCENT_LABEL[voice.accent]} · {GENDER_LABEL[voice.gender]}
            </div>
          </div>
          <Button
            variant="outline"
            size="icon-sm"
            class="rounded-full"
            aria-label={`Hear ${voice.name}`}
            disabled={pendingId !== null}
            onclick={() => preview(voice)}
          >
            {#if pendingId === voice.id}
              <Loader class="animate-spin" />
            {:else if lastPreviewId === voice.id && player.previewPlaying}
              <span class="flex h-4 items-end gap-0.5" aria-hidden="true">
                {#each [0, 0.25, 0.5] as delay (delay)}
                  <span
                    class="h-full w-0.5 origin-bottom rounded-full bg-current"
                    style="animation: sound-bar 0.9s ease-in-out {delay}s infinite"
                  ></span>
                {/each}
              </span>
            {:else}
              <Play />
            {/if}
          </Button>
        </div>
      {/each}
    </RadioGroup.Root>

    <Sheet.Footer class="border-t border-border/60">
      {#if book.locked}
        <p class="text-xs text-muted-foreground">Pause first to change this.</p>
      {/if}
      <Button
        variant="outline"
        disabled={app.settings.defaultVoice === project.voice}
        onclick={makeDefault}
      >
        Use for new books too
      </Button>
    </Sheet.Footer>
  </Sheet.Content>
</Sheet.Root>
