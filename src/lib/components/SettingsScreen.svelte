<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type ModelVariant, type Performance, type Theme } from "$lib/api";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Label } from "$lib/components/ui/label";
  import * as RadioGroup from "$lib/components/ui/radio-group";
  import { Switch } from "$lib/components/ui/switch";
  import * as ToggleGroup from "$lib/components/ui/toggle-group";
  import { formatBytes } from "$lib/format";
  import { MODEL_INFO, MODEL_VARIANTS } from "$lib/models";
  import { app } from "$lib/stores/app.svelte";
  import { attempt } from "$lib/toast";
  import { update } from "$lib/stores/update.svelte";
  import { cn } from "$lib/utils";
  import DownloadStatus from "./DownloadStatus.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  let storage = $state<number | null>(null);

  const settings = $derived(app.settings);
  const gpuDisabled = $derived(!app.gpuSupported || settings.model === "fast");

  $effect(() => {
    void attempt(api.storageSize()).then((size) => (storage = size ?? null));
  });

  function selectModel(variant: ModelVariant) {
    if (variant === settings.model || app.downloading) return;
    if (app.models[variant]) void app.updateSettings({ model: variant });
    else void app.startDownload(variant);
  }

  async function changeFolder() {
    const dir = await attempt(open({ directory: true, defaultPath: settings.outputDir }));
    if (dir) await app.updateSettings({ outputDir: dir });
  }
</script>

<div class="mx-auto max-w-2xl space-y-5 p-8">
  <div class="flex items-center gap-3">
    <Button
      variant="ghost"
      size="icon"
      aria-label="Back"
      onclick={() => app.go({ name: "library" })}
    >
      <ArrowLeft />
    </Button>
    <h1 class="text-2xl font-semibold tracking-tight">Settings</h1>
  </div>

  <SettingsCard title="Voice pack quality">
    <RadioGroup.Root
      value={settings.model}
      onValueChange={(v) => selectModel(v as ModelVariant)}
      disabled={app.downloading}
    >
      {#each MODEL_VARIANTS as variant (variant)}
        {@const info = MODEL_INFO[variant]}
        <div
          class={cn(
            "space-y-3 rounded-xl border border-border/60 p-4",
            settings.model === variant && "border-primary bg-primary/5",
          )}
        >
          <div class="flex items-center gap-3">
            <RadioGroup.Item value={variant} aria-label={info.name} />
            <div class="min-w-0 flex-1 space-y-0.5">
              <div class="flex items-center gap-2 font-medium">
                {info.name}
                {#if info.recommended}<Badge>Recommended</Badge>{/if}
              </div>
              <p class="text-sm text-muted-foreground">
                {info.blurb} · {info.size}
              </p>
            </div>
            {#if app.models[variant]}
              <span class="text-xs text-muted-foreground">Downloaded</span>
              <Button
                variant="outline"
                size="sm"
                disabled={settings.model === variant || app.downloading}
                title={settings.model === variant ? "In use right now" : undefined}
                onclick={() => app.removeModel(variant)}
              >
                Remove
              </Button>
            {:else}
              <Button
                variant="outline"
                size="sm"
                disabled={app.downloading}
                onclick={() => app.startDownload(variant)}
              >
                Download
              </Button>
            {/if}
          </div>
          {#if app.download.variant === variant}
            <DownloadStatus onretry={() => app.startDownload(variant)} />
          {/if}
        </div>
      {/each}
    </RadioGroup.Root>
  </SettingsCard>

  <SettingsCard title="Performance">
    <RadioGroup.Root
      value={settings.performance}
      onValueChange={(v) => app.updateSettings({ performance: v as Performance })}
    >
      {#each [{ value: "full", label: "Full speed" }, { value: "balanced", label: "Leave some power for me" }] as option (option.value)}
        <Label class="cursor-pointer gap-3 font-normal">
          <RadioGroup.Item value={option.value} />
          {option.label}
        </Label>
      {/each}
    </RadioGroup.Root>
    <div class="flex items-start justify-between gap-4 border-t border-border/60 pt-4">
      <div class="space-y-1">
        <Label for="gpu">Use graphics card (experimental)</Label>
        <p class="text-xs text-muted-foreground">
          {app.gpuSupported
            ? "Works with Natural quality. Aloud falls back to the processor if it doesn't work."
            : navigator.userAgent.includes("Linux")
              ? "Not available on Linux yet"
              : "Not available on this computer yet"}
        </p>
      </div>
      <Switch
        id="gpu"
        disabled={gpuDisabled}
        checked={settings.gpu && !gpuDisabled}
        onCheckedChange={(gpu) => app.updateSettings({ gpu })}
      />
    </div>
  </SettingsCard>

  <SettingsCard title="Audiobooks folder">
    <div class="flex items-center justify-between gap-4">
      <code
        class="min-w-0 truncate rounded-md bg-muted px-2 py-1 text-xs"
        title={settings.outputDir}
      >
        {settings.outputDir}
      </code>
      <Button variant="outline" size="sm" onclick={changeFolder}>Change…</Button>
    </div>
  </SettingsCard>

  <SettingsCard title="While making audiobooks">
    <div class="flex items-center justify-between gap-4">
      <Label for="keep-awake" class="font-normal">Keep computer awake</Label>
      <Switch
        id="keep-awake"
        checked={settings.keepAwake}
        onCheckedChange={(keepAwake) => app.updateSettings({ keepAwake })}
      />
    </div>
    <div class="flex items-center justify-between gap-4">
      <Label for="notify" class="font-normal">Notify me when done</Label>
      <Switch
        id="notify"
        checked={settings.notifyOnFinish}
        onCheckedChange={(notifyOnFinish) => app.updateSettings({ notifyOnFinish })}
      />
    </div>
  </SettingsCard>

  <SettingsCard title="Storage">
    <div class="flex items-center justify-between gap-4 text-sm">
      <span>
        Saved audio for your books:
        <span class="font-medium">{storage === null ? "…" : formatBytes(storage)}</span>
      </span>
      <Button variant="link" size="sm" onclick={() => attempt(api.openDataDir())}>
        Open data folder
      </Button>
    </div>
  </SettingsCard>

  <SettingsCard title="Appearance">
    <ToggleGroup.Root
      type="single"
      variant="outline"
      value={settings.theme}
      onValueChange={(v) => v && app.updateSettings({ theme: v as Theme })}
    >
      <ToggleGroup.Item value="system">System</ToggleGroup.Item>
      <ToggleGroup.Item value="light">Light</ToggleGroup.Item>
      <ToggleGroup.Item value="dark">Dark</ToggleGroup.Item>
    </ToggleGroup.Root>
  </SettingsCard>

  <SettingsCard title="About">
    <div class="space-y-2 text-sm text-muted-foreground">
      <p>
        Aloud {app.version} is free software under the GNU General Public License, version 3 or later.
      </p>
      <p>
        It uses Kokoro-82M (Apache-2.0), espeak-ng (GPL-3.0-or-later), misaki-rs (MIT), ONNX Runtime
        (MIT) and the Inter font (OFL-1.1).
      </p>
      <div class="flex flex-wrap gap-x-4">
        <Button
          variant="link"
          size="sm"
          class="h-auto px-0"
          onclick={() => attempt(api.openAbout("source"))}
        >
          Source code
        </Button>
        <Button
          variant="link"
          size="sm"
          class="h-auto px-0"
          onclick={() => attempt(api.openAbout("license"))}
        >
          Licence
        </Button>
        <Button
          variant="link"
          size="sm"
          class="h-auto px-0"
          onclick={() => attempt(api.openAbout("notices"))}
        >
          Third-party notices
        </Button>
      </div>
    </div>
    <div class="flex items-center justify-between gap-4">
      <Label for="check-updates" class="font-normal">Check for updates automatically</Label>
      <Switch
        id="check-updates"
        checked={settings.checkUpdates}
        onCheckedChange={(checkUpdates) => app.updateSettings({ checkUpdates })}
      />
    </div>
    <div>
      <Button
        variant="outline"
        size="sm"
        disabled={update.checking}
        onclick={() => update.checkNow()}
      >
        {update.checking ? "Checking…" : "Check now"}
      </Button>
    </div>
  </SettingsCard>
</div>
