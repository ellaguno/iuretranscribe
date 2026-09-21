<script lang="ts">
  import { api, type IureCase, type IureCrmItem, type IureCrmKind, type IureListing } from "../lib/api";
  import { app, iureFilesFor, iureLoggedIn, saveSettings, saveToCase, saveToCrm, saveToIurefficient, suggestedHours, term, type Job } from "../lib/state.svelte";
  import { fmtBytes } from "../lib/format";
  import Icon from "./Icon.svelte";

  let { job, onclose }: { job: Job; onclose: () => void } = $props();

  type Mode = "case" | "crm" | "webdav";
  let mode = $state<Mode>(iureLoggedIn() ? "case" : "webdav");
  let includeMedia = $state(app.settings?.iureUploadMedia ?? true);
  let uploading = $derived(!!job.iureUpload);
  let files = $derived(iureFilesFor(job, includeMedia));
  let loading = $state(false);
  let error = $state("");

  // --- carpeta WebDAV
  let listing = $state<IureListing | null>(null);
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

  // --- proyecto (API)
  let caseQuery = $state("");
  let cases = $state<IureCase[]>([]);
  let selectedCase = $state<IureCase | null>(null);
  let logHours = $state(true);
  let hours = $state(0.25);
  let caseTimer: ReturnType<typeof setTimeout> | undefined;
  async function searchCases() {
    loading = true;
    error = "";
    try {
      cases = await api.iureSearchCases(caseQuery);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  function onCaseInput() {
    clearTimeout(caseTimer);
    caseTimer = setTimeout(searchCases, 350);
  }

  // --- CRM
  let crmKind = $state<IureCrmKind>("opportunity");
  let crmQuery = $state("");
  let crmItems = $state<IureCrmItem[]>([]);
  let selectedCrm = $state<IureCrmItem | null>(null);
  let withActivity = $state(true);
  let crmTimer: ReturnType<typeof setTimeout> | undefined;
  async function searchCrm() {
    loading = true;
    error = "";
    try {
      crmItems = await api.iureCrmSearch(crmKind, crmQuery);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  function onCrmInput() {
    clearTimeout(crmTimer);
    crmTimer = setTimeout(searchCrm, 350);
  }

  $effect(() => {
    hours = suggestedHours(job);
    if (mode === "webdav" && !listing) {
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
    } else if (mode === "case" && !cases.length) {
      searchCases();
    } else if (mode === "crm" && !crmItems.length) {
      searchCrm();
    }
  });

  let canSave = $derived(
    files.length > 0 && !uploading && !loading &&
    ((mode === "webdav" && !!listing?.canUpload) || (mode === "case" && !!selectedCase) || (mode === "crm" && !!selectedCrm)),
  );

  async function save() {
    await saveSettings({ iureUploadMedia: includeMedia });
    let ok = false;
    if (mode === "webdav" && listing) ok = await saveToIurefficient(job, listing.path, includeMedia);
    else if (mode === "case" && selectedCase) ok = await saveToCase(job, selectedCase.id, `${selectedCase.caseNumber} · ${selectedCase.title}`, includeMedia, logHours ? hours : null);
    else if (mode === "crm" && selectedCrm) ok = await saveToCrm(job, selectedCrm.kind, selectedCrm.id, selectedCrm.name, includeMedia, withActivity);
    if (ok) onclose();
  }
  function baseName(p: string) {
    return p.split(/[\\/]/).pop() ?? p;
  }
  let destinoLabel = $derived(
    mode === "webdav" ? (listing?.path || "la raíz") : mode === "case" ? (selectedCase ? `${selectedCase.caseNumber} · ${selectedCase.title}` : `un ${term("case").toLowerCase()}`) : selectedCrm ? selectedCrm.name : crmKind === "lead" ? "un lead" : "una oportunidad",
  );
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && !uploading && onclose()}>
  <div class="modal card" role="dialog" aria-modal="true" aria-label="Guardar en Iurefficient">
    <div class="head">
      <h2><Icon name="upload" size={17} /> Guardar en Iurefficient</h2>
      <button class="btn icon ghost" onclick={onclose} disabled={uploading} aria-label="Cerrar"><Icon name="x" size={16} /></button>
    </div>

    <div class="modes">
      <button class:active={mode === "case"} onclick={() => (mode = "case")} disabled={!iureLoggedIn()} title={iureLoggedIn() ? "" : "Inicia sesión en Ajustes"}><Icon name="layers" size={14} /> {term("case")}</button>
      <button class:active={mode === "crm"} onclick={() => (mode = "crm")} disabled={!iureLoggedIn() || !app.iureSession?.crm} title={app.iureSession?.crm ? "" : "El CRM no está disponible en tu instancia o sesión"}><Icon name="sparkles" size={14} /> CRM</button>
      <button class:active={mode === "webdav"} onclick={() => (mode = "webdav")}><Icon name="folder" size={14} /> Carpeta</button>
    </div>

    {#if mode === "webdav"}
      <div class="crumbs">
        <button class="crumb" onclick={() => go("")} disabled={loading || uploading}><Icon name="folder" size={13} /> Raíz</button>
        {#each crumbs as c, i}
          <span class="sep">/</span>
          <button class="crumb" onclick={() => go(crumbs.slice(0, i + 1).join("/"))} disabled={loading || uploading}>{c}</button>
        {/each}
      </div>
    {:else if mode === "case"}
      <div class="search"><Icon name="layers" size={15} /><input class="input" placeholder="Buscar {term('case').toLowerCase()} por título, código o {term('client').toLowerCase()}" bind:value={caseQuery} oninput={onCaseInput} /></div>
    {:else}
      <div class="search">
        <div class="seg">
          <button class:active={crmKind === "opportunity"} onclick={() => { crmKind = "opportunity"; selectedCrm = null; searchCrm(); }}>Oportunidades</button>
          <button class:active={crmKind === "lead"} onclick={() => { crmKind = "lead"; selectedCrm = null; searchCrm(); }}>Leads</button>
        </div>
        <input class="input" placeholder="Buscar por nombre u organización" bind:value={crmQuery} oninput={onCrmInput} />
      </div>
    {/if}

    <div class="list scroll">
      {#if loading}
        <div class="empty"><span class="spin"><Icon name="loader" size={18} /></span> Cargando…</div>
      {:else if error}
        <div class="empty err">{error}</div>
      {:else if mode === "webdav" && listing}
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
      {:else if mode === "case"}
        {#if !cases.length}<div class="empty hint">Sin resultados.</div>{/if}
        {#each cases as c (c.id)}
          <button class="row" class:selected={selectedCase?.id === c.id} onclick={() => (selectedCase = c)}>
            <Icon name="layers" size={16} />
            <span class="name"><strong>{c.caseNumber}</strong> · {c.title}</span>
            {#if c.clientName}<span class="hint">{c.clientName}</span>{/if}
            {#if selectedCase?.id === c.id}<Icon name="check" size={14} />{/if}
          </button>
        {/each}
      {:else}
        {#if !crmItems.length}<div class="empty hint">Sin resultados.</div>{/if}
        {#each crmItems as it (it.id)}
          <button class="row" class:selected={selectedCrm?.id === it.id} onclick={() => (selectedCrm = it)}>
            <Icon name="sparkles" size={16} />
            <span class="name"><strong>{it.name}</strong>{it.organization ? ` · ${it.organization}` : ""}</span>
            {#if it.stage}<span class="pill">{it.stage}</span>{:else if it.status}<span class="pill">{it.status}</span>{/if}
            {#if selectedCrm?.id === it.id}<Icon name="check" size={14} />{/if}
          </button>
        {/each}
      {/if}
    </div>

    <div class="files">
      <div class="label">Se subirán a <strong>{destinoLabel}</strong>:</div>
      <ul>
        {#each files as f}<li>{baseName(f)}</li>{/each}
      </ul>
      <label class="check"><input type="checkbox" bind:checked={includeMedia} disabled={uploading} /> Incluir el audio/video original</label>
      {#if mode === "case"}
        <label class="check"><input type="checkbox" bind:checked={logHours} disabled={uploading} /> Registrar <input class="input hours" type="number" step="0.25" min="0.25" bind:value={hours} disabled={!logHours || uploading} /> horas facturables en el {term("case").toLowerCase()}</label>
      {:else if mode === "crm"}
        <label class="check"><input type="checkbox" bind:checked={withActivity} disabled={uploading} /> Registrar una actividad de reunión ({Math.max(1, Math.round((job.result?.audioSecs ?? 0) / 60))} min) con el resumen</label>
      {/if}
      <p class="hint">{mode === "webdav" ? "Un archivo con el mismo nombre en la carpeta se guarda como versión nueva. Los subtítulos .srt/.vtt se suben como .txt." : "Después podrás generar la minuta con el motor de Iurefficient desde la pestaña Minuta."}</p>
    </div>

    {#if job.iureUpload}
      <div class="progress-box">
        <div class="hint">Subiendo {job.iureUpload.fileName || "…"} ({job.iureUpload.index + 1} de {job.iureUpload.totalFiles}){job.iureUpload.total ? ` · ${fmtBytes(job.iureUpload.sent)} / ${fmtBytes(job.iureUpload.total)}` : ""}</div>
        <div class="progress" class:indeterminate={!job.iureUpload.total}><div style="width:{job.iureUpload.total ? (job.iureUpload.sent / job.iureUpload.total) * 100 : 0}%"></div></div>
      </div>
    {/if}

    <div class="foot">
      <button class="btn" onclick={onclose} disabled={uploading}>Cancelar</button>
      <button class="btn primary" onclick={save} disabled={!canSave}>
        {#if uploading}<span class="spin"><Icon name="loader" size={15} /></span> Subiendo…{:else}<Icon name="upload" size={15} /> Guardar{/if}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.45); display: grid; place-items: center; z-index: 30; }
  .modal { width: min(680px, 92vw); max-height: 90vh; display: flex; flex-direction: column; overflow: hidden; }
  .head { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px 8px; }
  .head h2 { display: flex; align-items: center; gap: 8px; }
  .modes { display: flex; gap: 4px; padding: 0 14px 8px; }
  .modes button { display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; border-radius: 8px; color: var(--text-2); font-weight: 550; }
  .modes button.active { background: var(--accent-soft); color: var(--accent); }
  .modes button:disabled { opacity: 0.45; }
  .crumbs { display: flex; align-items: center; flex-wrap: wrap; gap: 2px; padding: 0 14px 8px; font-size: 13px; }
  .crumb { display: inline-flex; align-items: center; gap: 4px; padding: 3px 7px; border-radius: 6px; color: var(--accent); font-weight: 550; }
  .crumb:hover:not(:disabled) { background: var(--accent-soft); }
  .sep { color: var(--muted); }
  .search { display: flex; align-items: center; gap: 8px; padding: 0 18px 8px; }
  .search .seg { display: inline-flex; background: var(--surface-2); padding: 3px; border-radius: 9px; gap: 2px; flex-shrink: 0; }
  .search .seg button { padding: 4px 10px; border-radius: 7px; font-size: 13px; color: var(--text-2); font-weight: 550; }
  .search .seg button.active { background: var(--surface); color: var(--text); box-shadow: var(--shadow); }
  .list { flex: 1; min-height: 160px; max-height: 300px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 9px 18px; text-align: left; color: var(--text); }
  .row:hover { background: var(--surface-2); }
  .row.selected { background: var(--accent-soft); }
  .row .name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row :global(svg:last-child) { color: var(--muted); }
  .empty { display: flex; align-items: center; gap: 8px; justify-content: center; padding: 30px; color: var(--muted); text-align: center; }
  .empty.err { color: var(--danger); }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .files { padding: 10px 18px; font-size: 13px; display: flex; flex-direction: column; gap: 6px; }
  .files ul { margin: 0; padding-left: 18px; color: var(--text-2); }
  .check { display: flex; align-items: center; gap: 7px; cursor: pointer; flex-wrap: wrap; }
  .check input[type="checkbox"] { accent-color: var(--accent); }
  .input.hours { width: 72px; padding: 3px 6px; }
  .progress-box { padding: 0 18px 8px; display: flex; flex-direction: column; gap: 5px; }
  .foot { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 18px 16px; }
</style>
