<script lang="ts">
  import { api, type IureListing } from "../lib/api";
  import { app, iureFilesFor, saveToIurefficient, saveSettings, type Job } from "../lib/state.svelte";
  import { fmtBytes } from "../lib/format";
  import Icon from "./Icon.svelte";

  let { job, onclose }: { job: Job; onclose: () => void } = $props();

  let listing = $state<IureListing | null>(null);
  let loading = $state(false);
  let error = $state("");
  let includeMedia = $state(app.settings?.iureUploadMedia ?? true);
  let uploading = $derived(!!job.iureUpload);
  let files = $derived(iureFilesFor(job, includeMedia));
  let crumbs = $derived((listing?.path ?? "").split("/").filter(Boolean));

  async function go(path: string) {
    loading = true;
    error = "";
    try {
      listing = await api.iureList(path);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Arranca en la última carpeta usada; si falla, en la raíz.
    const last = app.settings?.iureLastFolder;
    (async () => {
      if (last) {
        try {
          listing = await api.iureList(last);
          return;
        } catch {
          /* la carpeta ya no existe */
        }
      }
      await go("");
    })();
  });

  async function save() {
    if (!listing) return;
    await saveSettings({ iureUploadMedia: includeMedia });
    const ok = await saveToIurefficient(job, listing.path, includeMedia);
    if (ok) onclose();
  }
  function baseName(p: string) {
    return p.split(/[\\/]/).pop() ?? p;
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && !uploading && onclose()}>
  <div class="modal card" role="dialog" aria-modal="true" aria-label="Guardar en Iurefficient">
    <div class="head">
      <h2><Icon name="upload" size={17} /> Guardar en Iurefficient</h2>
      <button class="btn icon ghost" onclick={onclose} disabled={uploading} aria-label="Cerrar"><Icon name="x" size={16} /></button>
    </div>

    <div class="crumbs">
      <button class="crumb" onclick={() => go("")} disabled={loading || uploading}><Icon name="folder" size={13} /> Raíz</button>
      {#each crumbs as c, i}
        <span class="sep">/</span>
        <button class="crumb" onclick={() => go(crumbs.slice(0, i + 1).join("/"))} disabled={loading || uploading}>{c}</button>
      {/each}
    </div>

    <div class="list scroll">
      {#if loading}
        <div class="empty"><span class="spin"><Icon name="loader" size={18} /></span> Cargando…</div>
      {:else if error}
        <div class="empty err">{error}</div>
      {:else if listing}
        {#if !listing.entries.some((e) => e.isFolder)}
          <div class="empty hint">Sin subcarpetas. {listing.canUpload ? "Puedes guardar aquí." : "Aquí no se puede guardar."}</div>
        {/if}
        {#each listing.entries.filter((e) => e.isFolder) as e (e.path)}
          <button class="row" onclick={() => go(e.path)}>
            <Icon name="folder" size={16} />
            <span class="name">{e.name}</span>
            {#if !e.canUpload}<span class="pill">solo lectura</span>{/if}
            <Icon name="chevronRight" size={14} />
          </button>
        {/each}
      {/if}
    </div>

    <div class="files">
      <div class="label">Se subirán a <strong>{listing?.path || "la raíz"}</strong>:</div>
      <ul>
        {#each files as f}<li>{baseName(f)}</li>{/each}
      </ul>
      <label class="check"><input type="checkbox" bind:checked={includeMedia} disabled={uploading} /> Incluir el audio/video original</label>
      <p class="hint">Un archivo con el mismo nombre en la carpeta se guarda como versión nueva, no como duplicado.</p>
    </div>

    {#if job.iureUpload}
      <div class="progress-box">
        <div class="hint">Subiendo {job.iureUpload.fileName || "…"} ({job.iureUpload.index + 1} de {job.iureUpload.totalFiles}) · {fmtBytes(job.iureUpload.sent)}{job.iureUpload.total ? ` / ${fmtBytes(job.iureUpload.total)}` : ""}</div>
        <div class="progress"><div style="width:{job.iureUpload.total ? (job.iureUpload.sent / job.iureUpload.total) * 100 : 0}%"></div></div>
      </div>
    {/if}

    <div class="foot">
      <button class="btn" onclick={onclose} disabled={uploading}>Cancelar</button>
      <button class="btn primary" onclick={save} disabled={!listing || !listing.canUpload || uploading || files.length === 0}>
        {#if uploading}<span class="spin"><Icon name="loader" size={15} /></span> Subiendo…{:else}<Icon name="upload" size={15} /> Guardar aquí{/if}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.45); display: grid; place-items: center; z-index: 30; }
  .modal { width: min(640px, 92vw); max-height: 88vh; display: flex; flex-direction: column; overflow: hidden; }
  .head { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px 10px; }
  .head h2 { display: flex; align-items: center; gap: 8px; }
  .crumbs { display: flex; align-items: center; flex-wrap: wrap; gap: 2px; padding: 0 14px 8px; font-size: 13px; }
  .crumb { display: inline-flex; align-items: center; gap: 4px; padding: 3px 7px; border-radius: 6px; color: var(--accent); font-weight: 550; }
  .crumb:hover:not(:disabled) { background: var(--accent-soft); }
  .sep { color: var(--muted); }
  .list { flex: 1; min-height: 160px; max-height: 320px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 9px 18px; text-align: left; color: var(--text); }
  .row:hover { background: var(--surface-2); }
  .row .name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row :global(svg:last-child) { color: var(--muted); }
  .empty { display: flex; align-items: center; gap: 8px; justify-content: center; padding: 30px; color: var(--muted); text-align: center; }
  .empty.err { color: var(--danger); }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .files { padding: 10px 18px; font-size: 13px; display: flex; flex-direction: column; gap: 6px; }
  .files ul { margin: 0; padding-left: 18px; color: var(--text-2); }
  .check { display: flex; align-items: center; gap: 7px; cursor: pointer; }
  .check input { accent-color: var(--accent); }
  .progress-box { padding: 0 18px 8px; display: flex; flex-direction: column; gap: 5px; }
  .foot { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 18px 16px; }
</style>
