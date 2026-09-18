<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { app, addFiles, startQueue, stopQueue, clearFinished, queuedCount, saveSettings, downloadedModels, isActive } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import JobCard from "./JobCard.svelte";
  import JobDetail from "./JobDetail.svelte";

  const languages: [string, string][] = [
    ["es", "Español"], ["en", "Inglés"], ["pt", "Portugués"], ["fr", "Francés"], ["de", "Alemán"],
    ["it", "Italiano"], ["ca", "Catalán"], ["auto", "Detectar automáticamente"],
  ];

  let models = $derived(downloadedModels());
  let queued = $derived(queuedCount());
  let finished = $derived(app.jobs.filter((j) => j.status === "done" || j.status === "error" || j.status === "cancelled").length);
  let selected = $derived(app.jobs.find((j) => j.id === app.selectedJobId) ?? null);

  async function pickFiles() {
    const res = await open({
      multiple: true,
      title: "Selecciona audio o video",
      filters: [{ name: "Audio y video", extensions: app.sys?.supportedExtensions ?? ["mp3", "wav", "mp4"] }],
    });
    if (!res) return;
    await addFiles(Array.isArray(res) ? res : [res]);
  }
</script>

<header class="top">
  <div>
    <h1>Transcribir</h1>
    <p class="hint">Arrastra archivos de audio o video, elige el modelo y pulsa Transcribir.</p>
  </div>
  <div class="controls">
    <div class="field">
      <label for="model">Modelo</label>
      {#if models.length}
        <select id="model" class="input" value={app.settings?.modelId} onchange={(e) => saveSettings({ modelId: (e.target as HTMLSelectElement).value })} disabled={app.running}>
          {#each models as m}
            <option value={m.id}>{m.name}</option>
          {/each}
          {#if app.settings && !models.some((m) => m.id === app.settings?.modelId)}
            <option value={app.settings.modelId}>{app.settings.modelId} (no descargado)</option>
          {/if}
        </select>
      {:else}
        <button class="btn sm" onclick={() => (app.view = "models")}><Icon name="download" size={15} /> Descargar un modelo</button>
      {/if}
    </div>
    <div class="field">
      <label for="lang">Idioma</label>
      <select id="lang" class="input" value={app.settings?.language} onchange={(e) => saveSettings({ language: (e.target as HTMLSelectElement).value })} disabled={app.running}>
        {#each languages as [code, name]}
          <option value={code}>{name}</option>
        {/each}
      </select>
    </div>
  </div>
</header>

<div class="toolbar">
  <button class="btn" onclick={pickFiles}><Icon name="plus" size={16} /> Agregar archivos</button>
  {#if app.running}
    <button class="btn danger" onclick={stopQueue}><Icon name="stop" size={16} /> Detener</button>
  {:else}
    <button class="btn primary" onclick={startQueue} disabled={queued === 0}>
      <Icon name="play" size={16} /> Transcribir{queued > 1 ? ` (${queued})` : ""}
    </button>
  {/if}
  <span class="spacer"></span>
  {#if finished > 0}
    <button class="btn ghost sm" onclick={clearFinished}><Icon name="trash" size={15} /> Limpiar terminados</button>
  {/if}
</div>

{#if app.jobs.length === 0}
  <button class="dropzone" onclick={pickFiles}>
    <div class="dz-icon"><Icon name="mic" size={30} stroke={1.8} /></div>
    <h2>Arrastra aquí tus archivos</h2>
    <p class="hint">o haz clic para seleccionarlos · MP3, WAV, M4A, MP4, MKV, OGG, FLAC y más</p>
  </button>
{:else}
  <div class="split">
    <div class="jobs scroll">
      {#each app.jobs as job (job.id)}
        <JobCard {job} selected={job.id === app.selectedJobId} onselect={() => (app.selectedJobId = job.id)} />
      {/each}
      <button class="dropzone small" onclick={pickFiles}>
        <Icon name="plus" size={16} /> Agregar más archivos
      </button>
    </div>
    <div class="detail">
      {#if selected}
        <JobDetail job={selected} />
      {:else}
        <div class="empty hint">Selecciona un archivo para ver su transcripción.</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 22px 26px 12px; }
  .controls { display: flex; gap: 12px; }
  .controls .field { min-width: 190px; }
  .toolbar { display: flex; align-items: center; gap: 10px; padding: 6px 26px 14px; }
  .spacer { flex: 1; }
  .dropzone { flex: 1; margin: 0 26px 26px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; border: 2px dashed var(--border-strong); border-radius: 16px; color: var(--text-2); background: var(--surface); transition: border-color 0.15s, background 0.15s; }
  .dropzone:hover { border-color: var(--accent); background: var(--accent-soft); }
  .dz-icon { width: 64px; height: 64px; border-radius: 50%; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); margin-bottom: 8px; }
  .dropzone.small { flex: none; margin: 4px 0 0; flex-direction: row; padding: 12px; font-size: 13px; font-weight: 550; border-radius: 12px; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(300px, 380px) 1fr; gap: 18px; padding: 0 26px 26px; }
  .jobs { display: flex; flex-direction: column; gap: 10px; padding-right: 4px; }
  .detail { min-width: 0; display: flex; flex-direction: column; }
  .empty { display: grid; place-items: center; height: 100%; }
</style>
