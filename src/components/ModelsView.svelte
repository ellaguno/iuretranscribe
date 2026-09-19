<script lang="ts">
  import { api } from "../lib/api";
  import { app, downloadModel, cancelDownload, deleteModel, saveSettings, refreshModels, toast } from "../lib/state.svelte";
  import { fmtBytes, fmtMb } from "../lib/format";
  import Icon from "./Icon.svelte";

  const qualityLabel = { alta: "Calidad alta", media: "Calidad media", baja: "Calidad básica" } as const;

  function use(id: string) {
    saveSettings({ modelId: id });
    toast("Modelo seleccionado", "success", 2000);
  }
  function openDir() {
    if (app.sys) api.openPath(app.sys.modelsDir).catch((e) => toast(String(e), "error"));
  }
</script>

<header class="top">
  <div>
    <h1>Modelos</h1>
    <p class="hint">Los modelos se descargan una sola vez desde Hugging Face (whisper.cpp) y se guardan en tu equipo.</p>
  </div>
  <div class="hactions">
    <button class="btn ghost sm" onclick={refreshModels}><Icon name="refresh" size={15} /> Actualizar</button>
    <button class="btn sm" onclick={openDir} title={app.sys?.modelsDir}><Icon name="folder" size={15} /> Abrir carpeta</button>
  </div>
</header>

<div class="list scroll">
  {#each app.models as m (m.id)}
    {@const dl = app.downloads[m.id]}
    {@const current = app.settings?.modelId === m.id}
    <div class="card model" class:current>
      <div class="mhead">
        <div class="mtitle">
          <h3>{m.name}</h3>
          {#if m.recommended}<span class="pill accent">Recomendado</span>{/if}
          {#if m.bundled}<span class="pill">Incluido</span>{/if}
          {#if current}<span class="pill success"><Icon name="check" size={11} stroke={3} /> En uso</span>{/if}
        </div>
        <span class="size">{fmtMb(m.sizeMb)}</span>
      </div>
      <p class="desc">{m.description}</p>
      <div class="mfoot">
        <span class="pill">{qualityLabel[m.quality]}</span>
        <span class="spacer"></span>
        {#if dl}
          <div class="dl">
            <div class="progress" class:indeterminate={!dl.total}><div style="width:{dl.total ? (dl.downloaded / dl.total) * 100 : 0}%"></div></div>
            <span class="hint">{fmtBytes(dl.downloaded)}{dl.total ? ` / ${fmtBytes(dl.total)}` : ""}</span>
          </div>
          <button class="btn sm danger" onclick={() => cancelDownload(m.id)}><Icon name="x" size={14} /> Cancelar</button>
        {:else if m.downloaded}
          {#if !current}
            <button class="btn sm primary" onclick={() => use(m.id)}>Usar este modelo</button>
          {/if}
          {#if !m.bundled}
            <button class="btn sm ghost danger" title="Eliminar del disco" onclick={() => deleteModel(m.id)} disabled={app.running}><Icon name="trash" size={14} /></button>
          {/if}
        {:else}
          <button class="btn sm primary" onclick={() => downloadModel(m.id)}><Icon name="download" size={14} /> Descargar</button>
        {/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 22px 26px 14px; }
  .hactions { display: flex; gap: 8px; flex-shrink: 0; }
  .list { flex: 1; min-height: 0; padding: 0 26px 26px; display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 12px; align-content: start; }
  .model { padding: 14px 16px; display: flex; flex-direction: column; gap: 8px; }
  .model.current { border-color: var(--accent); }
  .mhead { display: flex; justify-content: space-between; align-items: center; gap: 10px; }
  .mtitle { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .size { font-size: 12.5px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .desc { font-size: 13px; color: var(--text-2); }
  .mfoot { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
  .spacer { flex: 1; }
  .dl { display: flex; flex-direction: column; gap: 3px; min-width: 150px; }
</style>
