<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { app, addFiles, startQueue, stopQueue, clearFinished, queuedCount, saveSettings, downloadedModels, isActive } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import JobCard from "./JobCard.svelte";
  import JobDetail from "./JobDetail.svelte";
  import { t, type Key } from "../lib/i18n.svelte";

  // Idioma del audio para Whisper (no el de la interfaz).
  const languages = ["es", "en", "pt", "fr", "de", "it", "ca", "auto"] as const;

  let models = $derived(downloadedModels());
  let queued = $derived(queuedCount());
  let finished = $derived(app.jobs.filter((j) => j.status === "done" || j.status === "error" || j.status === "cancelled").length);
  // La cola se procesa en orden de llegada, pero se muestra con la más reciente arriba.
  let newestFirst = $derived([...app.jobs].reverse());
  let selected = $derived(app.jobs.find((j) => j.id === app.selectedJobId) ?? null);

  async function pickFiles() {
    const res = await open({
      multiple: true,
      title: t("transcribe.pickTitle"),
      filters: [{ name: t("transcribe.pickFilter"), extensions: app.sys?.supportedExtensions ?? ["mp3", "wav", "mp4"] }],
    });
    if (!res) return;
    await addFiles(Array.isArray(res) ? res : [res]);
  }
</script>

<header class="top">
  <div>
    <h1>{t("transcribe.title")}</h1>
    <p class="hint">{t("transcribe.hint")}</p>
  </div>
  <div class="controls">
    <div class="field">
      <label for="model">{t("transcribe.model")}</label>
      {#if models.length}
        <select id="model" class="input" value={app.settings?.modelId} onchange={(e) => saveSettings({ modelId: (e.target as HTMLSelectElement).value })} disabled={app.running}>
          {#each models as m}
            <option value={m.id}>{m.name}</option>
          {/each}
          {#if app.settings && !models.some((m) => m.id === app.settings?.modelId)}
            <option value={app.settings.modelId}>{t("transcribe.modelNotDownloaded", { id: app.settings.modelId })}</option>
          {/if}
        </select>
      {:else}
        <button class="btn sm" onclick={() => (app.view = "models")}><Icon name="download" size={15} /> {t("transcribe.downloadModel")}</button>
      {/if}
    </div>
    <div class="field">
      <label for="lang">{t("transcribe.language")}</label>
      <select id="lang" class="input" value={app.settings?.language} onchange={(e) => saveSettings({ language: (e.target as HTMLSelectElement).value })} disabled={app.running}>
        {#each languages as code}
          <option value={code}>{t(`lang.${code}` as Key)}</option>
        {/each}
      </select>
    </div>
  </div>
</header>

{#if app.iureSession && !app.iureSession.loggedIn}
  <button class="connect-banner" onclick={() => (app.view = "settings")}>
    <Icon name="cloud" size={18} />
    <span><strong>{t("transcribe.connectTitle")}</strong> {t("transcribe.connectText")}</span>
    <span class="go">{t("transcribe.connectGo")} <Icon name="chevronRight" size={14} /></span>
  </button>
{/if}

<div class="toolbar">
  <button class="btn" onclick={pickFiles}><Icon name="plus" size={16} /> {t("transcribe.addFiles")}</button>
  {#if app.running}
    <button class="btn danger" onclick={stopQueue}><Icon name="stop" size={16} /> {t("transcribe.stop")}</button>
  {:else}
    <button class="btn primary" onclick={startQueue} disabled={queued === 0}>
      <Icon name="play" size={16} /> {t("transcribe.start")}{queued > 1 ? ` (${queued})` : ""}
    </button>
  {/if}
  <span class="spacer"></span>
  {#if finished > 0}
    <button class="btn ghost sm" onclick={clearFinished}><Icon name="trash" size={15} /> {t("transcribe.clearFinished")}</button>
  {/if}
</div>

{#if app.jobs.length === 0}
  <button class="dropzone" onclick={pickFiles}>
    <div class="dz-icon"><Icon name="mic" size={30} stroke={1.8} /></div>
    <h2>{t("transcribe.dropHere")}</h2>
    <p class="hint">{t("transcribe.dropHint")}</p>
  </button>
{:else}
  <div class="split">
    <div class="jobs scroll">
      {#each newestFirst as job (job.id)}
        <JobCard {job} selected={job.id === app.selectedJobId} onselect={() => (app.selectedJobId = job.id)} />
      {/each}
      <button class="dropzone small" onclick={pickFiles}>
        <Icon name="plus" size={16} /> {t("transcribe.addMore")}
      </button>
    </div>
    <div class="detail">
      {#if selected}
        <JobDetail job={selected} />
      {:else}
        <div class="empty hint">{t("transcribe.selectFile")}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 22px 26px 12px; }
  .controls { display: flex; gap: 12px; }
  .controls .field { min-width: 190px; }
  .connect-banner { margin: 0 26px 10px; display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-radius: 12px; border: 1px solid var(--accent); background: var(--accent-soft); color: var(--text); text-align: left; font-size: 13px; }
  .connect-banner .go { margin-left: auto; display: inline-flex; align-items: center; gap: 4px; color: var(--accent); font-weight: 650; white-space: nowrap; }
  .toolbar { display: flex; align-items: center; gap: 10px; padding: 6px 26px 14px; }
  .spacer { flex: 1; }
  .dropzone { flex: 1; margin: 0 26px 26px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; border: 2px dashed var(--border-strong); border-radius: 16px; color: var(--text-2); background: var(--surface); transition: border-color 0.15s, background 0.15s; }
  .dropzone:hover { border-color: var(--accent); background: var(--accent-soft); }
  .dz-icon { width: 64px; height: 64px; border-radius: 50%; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); margin-bottom: 8px; }
  .dropzone.small { flex: none; margin: 4px 0 0; flex-direction: row; padding: 12px; font-size: 13px; font-weight: 550; border-radius: 12px; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(300px, 380px) 1fr; grid-template-rows: minmax(0, 1fr); gap: 18px; padding: 0 26px 26px; }
  @media (max-width: 1000px) { .split { grid-template-columns: minmax(230px, 300px) 1fr; gap: 14px; padding: 0 18px 18px; } }
  .jobs { display: flex; flex-direction: column; gap: 10px; padding-right: 4px; }
  .detail { min-width: 0; display: flex; flex-direction: column; }
  .empty { display: grid; place-items: center; height: 100%; }
</style>
