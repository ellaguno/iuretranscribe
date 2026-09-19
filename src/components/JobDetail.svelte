<script lang="ts">
  import { api, type DocKind } from "../lib/api";
  import { app, generateDoc, isActive, metaFilled, retranscribe, toast, type Job } from "../lib/state.svelte";
  import { fmtDuration, fmtSpeed, fmtTimestamp } from "../lib/format";
  import { renderMarkdown } from "../lib/markdown";
  import Icon from "./Icon.svelte";
  import MetaForm from "./MetaForm.svelte";

  let { job }: { job: Job } = $props();
  let tab = $state<"transcript" | "summary" | "minutes">("transcript");
  let filled = $derived(metaFilled(job.meta));
  // Abierto por defecto mientras no se hayan capturado datos; plegado cuando ya hay.
  // svelte-ignore state_referenced_locally
  let showMeta = $state(!metaFilled(job.meta));
  let metaSummary = $derived(
    [job.meta.date, job.meta.place, job.meta.participants.split(/\n|,|;/).map((x) => x.trim()).filter(Boolean).join(", ")]
      .filter(Boolean)
      .join(" · "),
  );
  let docsDone = $derived(job.summary.status === "done" || job.minutes.status === "done");
  let listEl = $state<HTMLDivElement | null>(null);

  let segments = $derived(job.result ? job.result.segments : job.liveSegments);
  let hasKey = $derived(!!app.settings?.openrouterApiKey?.trim());

  $effect(() => {
    // Autoscroll mientras llegan segmentos en vivo.
    if (isActive(job) && listEl && segments.length) listEl.scrollTop = listEl.scrollHeight;
  });
  $effect(() => {
    if (job.id) {
      tab = "transcript";
      showMeta = !metaFilled(job.meta);
    }
  });

  async function copyText() {
    const text = job.result ? job.result.text : job.liveSegments.map((s) => s.text).join(" ");
    await navigator.clipboard.writeText(text);
    toast("Texto copiado al portapapeles", "success", 2000);
  }
  async function copyDoc(kind: DocKind) {
    const c = (kind === "summary" ? job.summary : job.minutes).content ?? "";
    await navigator.clipboard.writeText(c);
    toast("Copiado al portapapeles", "success", 2000);
  }
  function openFile(path: string) {
    api.openPath(path).catch((e) => toast(String(e), "error"));
  }
  function reveal(path: string) {
    api.revealPath(path).catch((e) => toast(String(e), "error"));
  }
</script>

<div class="card panel">
  <div class="phead">
    <div class="ptitle">
      <h2 title={job.path}>{job.name}</h2>
      {#if job.result}
        <div class="stats">
          <span><Icon name="clock" size={13} /> Audio {fmtDuration(job.result.audioSecs)}</span>
          <span><Icon name="zap" size={13} /> Transcrito en {fmtDuration(job.result.elapsedSecs)} ({fmtSpeed(job.result.audioSecs, job.result.elapsedSecs)})</span>
          {#if job.result.detectedLanguage}<span>Idioma: {job.result.detectedLanguage}</span>{/if}
        </div>
      {:else if isActive(job)}
        <div class="stats"><span class="spin"><Icon name="loader" size={13} /></span><span>Procesando…</span></div>
      {/if}
    </div>
    {#if job.result}
      <div class="outputs">
        {#each job.result.outputs as o}
          <button class="btn sm" title={o.path} onclick={() => openFile(o.path)}><Icon name="file" size={14} /> .{o.format}</button>
        {/each}
        <button class="btn sm ghost" title="Mostrar en la carpeta" onclick={() => reveal(job.result!.outputs[0]?.path ?? job.result!.outputDir)}><Icon name="folder" size={14} /></button>
        <button class="btn sm ghost" title="Volver a transcribir el archivo completo con calidad alta (beam search)" disabled={app.running} onclick={() => retranscribe(job.id)}><Icon name="refresh" size={14} /> Calidad alta</button>
      </div>
    {/if}
  </div>

  <div class="meta" class:open={showMeta}>
    <button class="meta-toggle" onclick={() => (showMeta = !showMeta)} aria-expanded={showMeta}>
      <Icon name="doc" size={15} />
      <span>Detalles de la reunión</span>
      {#if filled}<span class="summary hint" title={metaSummary}>{metaSummary}</span>{:else}<span class="hint">participantes, fecha y lugar para el resumen y la minuta</span>{/if}
      <span class="chev" class:up={showMeta}><Icon name="chevron" size={14} /></span>
    </button>
    {#if showMeta}
      <div class="meta-body">
        <MetaForm bind:meta={job.meta} hint={"Se envían junto con la transcripción al generar el resumen y la minuta." + (docsDone ? " Ya se generaron documentos: usa «volver a generar» en su pestaña para aplicar estos cambios." : "")} />
      </div>
    {/if}
  </div>

  <div class="tabs">
    <button class:active={tab === "transcript"} onclick={() => (tab = "transcript")}><Icon name="list" size={15} /> Transcripción</button>
    <button class:active={tab === "summary"} onclick={() => (tab = "summary")} disabled={!job.result}>
      <Icon name="sparkles" size={15} /> Resumen {#if job.summary.status === "done"}<span class="dot"></span>{/if}
    </button>
    <button class:active={tab === "minutes"} onclick={() => (tab = "minutes")} disabled={!job.result}>
      <Icon name="doc" size={15} /> Minuta {#if job.minutes.status === "done"}<span class="dot"></span>{/if}
    </button>
    <span class="spacer"></span>
    {#if tab === "transcript" && segments.length}
      <button class="btn sm ghost" onclick={copyText}><Icon name="copy" size={14} /> Copiar texto</button>
    {/if}
  </div>

  {#if tab === "transcript"}
    <div class="body scroll" bind:this={listEl}>
      {#if segments.length === 0}
        <div class="empty hint">
          {#if job.status === "queued"}La transcripción aparecerá aquí.{:else if job.status === "error"}{job.error}{:else if isActive(job)}Esperando los primeros segmentos…{:else}Sin texto.{/if}
        </div>
      {:else}
        {#each segments as s, i (i)}
          <div class="seg">
            <span class="ts">{fmtTimestamp(s.startMs)}</span>
            <span class="txt">{s.text}</span>
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    {@const kind = tab}
    {@const doc = kind === "summary" ? job.summary : job.minutes}
    <div class="body scroll">
      {#if doc.status === "done" && doc.content}
        <div class="docbar">
          <span class="hint" title={doc.path}>Guardado en {doc.path}{filled ? " · con detalles de la reunión" : ""}</span>
          <button class="btn sm ghost" onclick={() => copyDoc(kind)}><Icon name="copy" size={14} /> Copiar</button>
          <button class="btn sm ghost" onclick={() => openFile(doc.path!)}><Icon name="external" size={14} /> Abrir</button>
          <button class="btn sm ghost" onclick={() => generateDoc(job, kind)} title="Volver a generar"><Icon name="refresh" size={14} /></button>
        </div>
        <div class="md">{@html renderMarkdown(doc.content)}</div>
      {:else if doc.status === "loading"}
        <div class="empty"><span class="spin"><Icon name="loader" size={22} /></span><p class="hint">Generando con {app.settings?.openrouterModel}…</p></div>
      {:else}
        <div class="empty">
          {#if doc.status === "error"}<p class="err">{doc.error}</p>{/if}
          {#if hasKey}
            <button class="btn primary" onclick={() => generateDoc(job, kind)}>
              <Icon name="sparkles" size={16} /> Generar {kind === "summary" ? "resumen" : "minuta"}
            </button>
            <p class="hint">Se enviará la transcripción a OpenRouter ({app.settings?.openrouterModel}).</p>
            {#if filled}
              <p class="hint ok"><Icon name="check" size={13} /> Se incluirán los detalles de la reunión: {metaSummary}</p>
            {:else}
              <p class="hint"><Icon name="info" size={13} /> Sin detalles de la reunión. <button class="link" onclick={() => (showMeta = true)}>Captura participantes, fecha y lugar</button> para una minuta más completa.</p>
            {/if}
          {:else}
            <p class="hint">Configura tu llave de OpenRouter en Ajustes para generar resúmenes y minutas.</p>
            <button class="btn" onclick={() => (app.view = "settings")}><Icon name="settings" size={15} /> Ir a Ajustes</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .phead { display: flex; justify-content: space-between; gap: 14px; padding: 16px 18px 10px; }
  .ptitle { min-width: 0; }
  .ptitle h2 { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .stats { display: flex; flex-wrap: wrap; gap: 14px; margin-top: 4px; font-size: 12.5px; color: var(--muted); }
  .stats span { display: inline-flex; align-items: center; gap: 5px; }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .outputs { display: flex; gap: 6px; flex-shrink: 0; align-items: flex-start; }
  .meta { border-top: 1px solid var(--border); }
  .meta-toggle { width: 100%; display: flex; align-items: center; gap: 8px; padding: 9px 18px; color: var(--text-2); font-weight: 550; text-align: left; }
  .meta-toggle:hover { background: var(--surface-2); }
  .meta-toggle .hint { font-weight: 400; }
  .chev { margin-left: auto; display: inline-flex; transition: transform 0.15s; }
  .chev.up { transform: rotate(180deg); }
  .meta-body { padding: 4px 18px 14px; }
  .meta-toggle .summary { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-weight: 400; }
  .hint.ok { color: var(--success); display: inline-flex; align-items: center; gap: 5px; }
  .link { color: var(--accent); text-decoration: underline; font-weight: 550; }
  .tabs { display: flex; align-items: center; gap: 4px; padding: 0 12px; border-bottom: 1px solid var(--border); }
  .tabs > button:not(.btn) { display: inline-flex; align-items: center; gap: 6px; padding: 10px 10px; color: var(--muted); font-weight: 550; border-bottom: 2px solid transparent; margin-bottom: -1px; }
  .tabs > button:not(.btn):hover:not(:disabled) { color: var(--text); }
  .tabs > button.active { color: var(--accent); border-bottom-color: var(--accent); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--success); }
  .spacer { flex: 1; }
  .body { flex: 1; min-height: 0; padding: 12px 18px 18px; user-select: text; }
  .seg { display: grid; grid-template-columns: 58px 1fr; gap: 10px; padding: 5px 0; border-bottom: 1px dashed var(--border); }
  .ts { font-family: var(--mono); font-size: 12px; color: var(--muted); padding-top: 2px; }
  .txt { line-height: 1.5; }
  .empty { height: 100%; min-height: 200px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; padding: 20px; }
  .err { color: var(--danger); max-width: 520px; }
  .docbar { display: flex; align-items: center; gap: 6px; margin-bottom: 8px; }
  .docbar .hint { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
