<script lang="ts">
  import { untrack } from "svelte";
  import { api, type DocKind } from "../lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, appStatus, generateDoc, isActive, metaFilled, metaFilledByUser, openWithEditor, retranscribe, toast, type Job } from "../lib/state.svelte";
  import { fmtDuration, fmtSpeed, fmtTimestamp } from "../lib/format";
  import { renderMarkdown } from "../lib/markdown";
  import Icon from "./Icon.svelte";
  import MetaForm from "./MetaForm.svelte";
  import IurePicker from "./IurePicker.svelte";
  import { composeWithIurefficient, downloadComposed, iureConfigured, iureLoggedIn, iureTranscriptDoc, iureWaitAiOptions, notify, pollCompose, segmentLine, summaryWithIurefficient, term, uploadTranscriptOnly } from "../lib/state.svelte";
  import type { IureBlueprint, IureCase, IureCommitment } from "../lib/api";
  import type { ComposedDoc } from "../lib/state.svelte";

  // ---- Compromisos → tareas
  let commitFor = $state<string | null>(null); // documentId de la minuta cuyos compromisos se muestran
  let commitments = $state<(IureCommitment & { include: boolean })[]>([]);
  let commitCaseId = $state<string | null>(null);
  let commitLoading = $state(false);
  let commitApplying = $state(false);
  let commitError = $state("");
  let commitDone = $state<number | null>(null);
  let caseQuery = $state("");
  let caseResults = $state<IureCase[]>([]);
  let caseTimer: ReturnType<typeof setTimeout> | undefined;
  async function loadCommitments(documentId: string) {
    commitFor = documentId;
    commitLoading = true;
    commitError = "";
    commitDone = null;
    try {
      const r = await api.iureCommitments(documentId);
      commitments = r.commitments.map((c) => ({ ...c, include: true }));
      commitCaseId = r.caseId ?? job.iure?.caseId ?? null;
      if (!commitments.length) commitError = "La instancia no detectó compromisos en esta minuta.";
    } catch (e) {
      commitError = String(e);
    } finally {
      commitLoading = false;
    }
  }
  async function applyCommitments() {
    if (!commitFor) return;
    const items = commitments.filter((c) => c.include && c.title.trim()).map(({ include, ...c }) => c);
    if (!items.length) return;
    commitApplying = true;
    commitError = "";
    try {
      const n = await api.iureApplyCommitments(commitFor, commitCaseId, items);
      commitDone = n;
      toast(`${n} tarea(s) creadas en Iurefficient`, "success", 7000);
      notify("IureTranscribe", `${n} tarea(s) creadas en Iurefficient`);
    } catch (e) {
      commitError = String(e);
    } finally {
      commitApplying = false;
    }
  }
  function onCaseQuery() {
    clearTimeout(caseTimer);
    caseTimer = setTimeout(async () => {
      try {
        caseResults = await api.iureSearchCases(caseQuery);
      } catch {
        caseResults = [];
      }
    }, 350);
  }

  let blueprints = $state<IureBlueprint[] | null>(null);
  let loadingBlueprints = $state(false);
  let blueprintError = $state("");
  async function loadBlueprints() {
    const doc = iureTranscriptDoc(job);
    if (!doc) return;
    loadingBlueprints = true;
    blueprintError = "";
    try {
      const o = await iureWaitAiOptions(doc.id, 4);
      if (!o.canGenerate) blueprintError = o.reason === "no_text" ? "La instancia todavía no ha extraído el texto del documento; inténtalo en unos segundos." : "No se puede generar a partir de ese documento.";
      blueprints = o.blueprints;
    } catch (e) {
      blueprintError = String(e);
    } finally {
      loadingBlueprints = false;
    }
  }
  const label = (b: { genre: string; name?: string; blueprintName?: string }) => `${b.genre} ${b.name ?? b.blueprintName ?? ""}`;
  const isMinuta = (b: { genre: string; name?: string; blueprintName?: string }) => /minuta|minute|acta/i.test(label(b));
  const isResumen = (b: { genre: string; name?: string; blueprintName?: string }) => /resumen|summary|síntesis|sintesis/i.test(label(b));
  function blueprintsFor(kind: "summary" | "minutes"): IureBlueprint[] {
    if (!blueprints) return [];
    if (kind === "summary") return blueprints.filter(isResumen);
    const f = blueprints.filter(isMinuta);
    return f.length ? f : blueprints.filter((b) => !isResumen(b));
  }
  let summaryTarget = $state<"summary" | "minutes" | null>(null);
  async function generateWithoutProjectFor(kind: "summary" | "minutes") {
    uploadingTranscript = true;
    try {
      if (await uploadTranscriptOnly(job, null, null)) {
        if (kind === "summary") await summaryWithIurefficient(job);
        else await loadBlueprints();
      }
    } finally {
      uploadingTranscript = false;
    }
  }
  function composedFor(kind: "summary" | "minutes") {
    const all = job.iure?.composed ?? [];
    const f = kind === "minutes" ? all.filter(isMinuta) : all.filter(isResumen);
    return f.length ? f : kind === "minutes" ? all.filter((c) => !isResumen(c)) : all.filter((c) => !isMinuta(c));
  }
  $effect(() => {
    // Reanuda el seguimiento de generaciones en curso tras reiniciar.
    if ((job.iure?.composed ?? []).some((c) => !c.error && c.state !== "SUCCESS" && c.state !== "FAILURE")) pollCompose(job);
  });

  let { job }: { job: Job } = $props();
  let showPicker = $state(false);
  let pickerThenCompose = $state(false);
  let uploadingTranscript = $state(false);

  let tab = $state<"transcript" | "summary" | "minutes">("transcript");
  let filled = $derived(metaFilled(job.meta));
  // Abierto por defecto mientras no se hayan capturado datos; plegado cuando ya hay.
  // svelte-ignore state_referenced_locally
  let showMeta = $state(!metaFilledByUser(job.meta));
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
  // Sólo al cambiar de archivo: `untrack` evita que el efecto dependa de los campos
  // del formulario (si no, se plegaba con cada tecla al detectar datos capturados).
  $effect(() => {
    const id = job.id;
    untrack(() => {
      if (id) {
        tab = "transcript";
        showMeta = !metaFilledByUser(job.meta);
      }
    });
  });

  async function copyText() {
    const text = (job.result ? job.result.segments : job.liveSegments).map(segmentLine).filter(Boolean).join("\n");
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
  let editor = $derived(appStatus("editor"));
  /** IureEditor abre Markdown, texto y .docx; no tiene sentido ofrecerla para PDF. */
  function editorCanOpen(path: string) {
    return /\.(md|markdown|txt|docx)$/i.test(path);
  }
  function reveal(path: string) {
    api.revealPath(path).catch((e) => toast(String(e), "error"));
  }
</script>

{#if showPicker}
  <IurePicker {job} onclose={() => (showPicker = false)} onsaved={() => { if (summaryTarget === "summary") { summaryTarget = null; pickerThenCompose = false; summaryWithIurefficient(job); } else if (pickerThenCompose) { pickerThenCompose = false; summaryTarget = null; loadBlueprints(); } }} />
{/if}

<div class="card panel">
  <div class="phead">
    <div class="ptitle">
      <h2 title={job.path}>{job.name}</h2>
      {#if job.result}
        <div class="stats">
          <span><Icon name="clock" size={13} /> Audio {fmtDuration(job.result.audioSecs)}</span>
          <span><Icon name="zap" size={13} /> Transcrito en {fmtDuration(job.result.elapsedSecs)} ({fmtSpeed(job.result.audioSecs, job.result.elapsedSecs)})</span>
          {#if job.result.detectedLanguage}<span>Idioma: {job.result.detectedLanguage}</span>{/if}
          {#if job.iure}<span class="iure" title={job.iure.files.join(", ")}><Icon name="cloud" size={13} /> Iurefficient: {job.iure.folder || "raíz"}</span>{/if}
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
        {#if iureConfigured() || iureLoggedIn()}
          <button class="btn sm {job.iure ? '' : 'primary'}" title={job.iure ? `Guardado en ${job.iure.folder} · volver a subir` : "Subir transcripción, resumen y minuta a un proyecto de Iurefficient"} disabled={!!job.iureUpload} onclick={() => (showPicker = true)}>
            <Icon name="upload" size={14} /> {job.iure ? "Guardado en Iurefficient" : "Guardar en Iurefficient"}
          </button>
          {#if job.iure}
            <button class="btn sm ghost" title="Abrir Iurefficient en el navegador" onclick={() => openUrl(job.iure!.webUrl)}><Icon name="external" size={14} /></button>
          {/if}
        {:else}
          <button class="btn sm ghost" title="Conecta tu cuenta de Iurefficient en Ajustes para guardar directo en un proyecto" onclick={() => (app.view = "settings")}><Icon name="cloud" size={14} /> Iurefficient</button>
        {/if}
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
          <div class="seg" class:mine={s.speaker && i > 0 && segments[i - 1].speaker === s.speaker}>
            <span class="ts">{fmtTimestamp(s.startMs)}</span>
            <span class="txt">{#if s.speaker}<span class="spk" class:other={s.speaker === "Interlocutor"}>{s.speaker}</span> {/if}{s.text}</span>
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    {@const kind = tab}
    {@const doc = kind === "summary" ? job.summary : job.minutes}
    <div class="body scroll">
      {#if job.result && iureLoggedIn()}
        <div class="engine card-inner">
          <div class="engine-head">
            <span class="label"><Icon name="cloud" size={14} /> {kind === "minutes" ? "Minuta" : "Resumen"} con el motor de Iurefficient</span>
            <span class="hint">Con el formato del despacho, sin llave de OpenRouter.</span>
          </div>
          {#each composedFor(kind) as c (c.taskId)}
            <div class="composed">
              {#if c.state === "SUCCESS"}
                <span class="pill success"><Icon name="check" size={11} stroke={3} /> {c.blueprintName}</span>
                {#if c.localPath}
                  <button class="btn sm" onclick={() => openFile(c.localPath!)}><Icon name="file" size={13} /> Abrir {c.localPath.split(".").pop()?.toUpperCase()}</button>
                  {#if editorCanOpen(c.localPath)}
                    <button class="btn sm ghost" title={editor?.installed ? "Abrir con IureEditor" : "IureEditor no está instalada: descargar"} onclick={() => openWithEditor(c.localPath!)}><Icon name="edit" size={13} /> IureEditor</button>
                  {/if}
                  <button class="btn sm ghost" title="Mostrar en la carpeta" onclick={() => reveal(c.localPath!)}><Icon name="folder" size={13} /></button>
                {:else if c.documentId}
                  <button class="btn sm ghost" onclick={() => downloadComposed(job, c)}><Icon name="download" size={13} /> Descargar junto a la transcripción</button>
                {/if}
                {#if c.link}<button class="btn sm ghost" title={job.iure?.caseId ? "Documentos del proyecto" : "Documentos → General (sin proyecto)"} onclick={() => openUrl(c.link!)}><Icon name="external" size={13} /> Ver en Iurefficient</button>{/if}
                {#if c.documentId && kind === "minutes"}
                  <button class="btn sm {commitFor === c.documentId ? 'ghost' : 'primary'}" onclick={() => loadCommitments(c.documentId!)} disabled={commitLoading}><Icon name="check" size={13} /> Compromisos → tareas</button>
                {/if}
              {:else if c.error}
                <span class="pill danger">{c.blueprintName}: {c.error}</span>
              {:else}
                <span class="pill accent"><span class="spin"><Icon name="loader" size={11} /></span> {c.blueprintName}: {c.section ? `sección ${c.current} de ${c.total} · ${c.section}` : "en cola…"}</span>
              {/if}
            </div>
          {/each}
          {#if commitFor && kind === "minutes"}
            <div class="commits">
              <div class="engine-head">
                <span class="label"><Icon name="list" size={14} /> Compromisos detectados</span>
                <button class="btn icon ghost" onclick={() => (commitFor = null)} aria-label="Cerrar"><Icon name="x" size={14} /></button>
              </div>
              {#if commitLoading}
                <p class="hint"><span class="spin"><Icon name="loader" size={13} /></span> Leyendo la minuta…</p>
              {:else}
                {#if commitError}<p class="hint errtxt">{commitError}</p>{/if}
                {#each commitments as c, i (i)}
                  <div class="commit" class:off={!c.include}>
                    <input type="checkbox" bind:checked={c.include} />
                    <input class="input" bind:value={c.title} placeholder="Título de la tarea" />
                    <input class="input who" bind:value={c.assigneeName} placeholder={c.assignedToId ? "Responsable" : "Responsable (no identificado)"} title={c.assignedToId ? "Usuario de la instancia identificado" : "Sin usuario identificado: la tarea se creará sin responsable"} />
                    <input class="input date" type="date" bind:value={c.dueDate} title={c.dueHint ?? ""} />
                  </div>
                {/each}
                {#if commitments.length}
                  {#if !commitCaseId}
                    <div class="field">
                      <span class="label">Las tareas se crean en un {term("case").toLowerCase()}: elige uno</span>
                      <input class="input" placeholder="Buscar {term('case').toLowerCase()}…" bind:value={caseQuery} oninput={onCaseQuery} />
                      {#if caseResults.length}
                        <div class="cases">
                          {#each caseResults.slice(0, 8) as cs (cs.id)}
                            <button class="btn sm ghost" onclick={() => { commitCaseId = cs.id; caseResults = []; caseQuery = `${cs.caseNumber} · ${cs.title}`; }}>{cs.caseNumber} · {cs.title}</button>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/if}
                  <div class="row-actions">
                    <button class="btn sm primary" onclick={applyCommitments} disabled={commitApplying || !commitCaseId || !commitments.some((c) => c.include && c.title.trim())}>
                      {#if commitApplying}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="check" size={13} />{/if} Crear {commitments.filter((c) => c.include && c.title.trim()).length} tarea(s) en Iurefficient
                    </button>
                    {#if commitDone !== null}<span class="pill success">{commitDone} creadas</span>{/if}
                  </div>
                {/if}
              {/if}
            </div>
          {/if}
          {#if !iureTranscriptDoc(job)}
            <p class="hint">La instancia genera a partir de un documento suyo: la transcripción se sube primero (a un {app.iureSession?.terminology?.case ?? "proyecto"}, o sin proyecto) y después se elige el formato.</p>
            <div class="row-actions">
              <button class="btn sm primary" onclick={() => { pickerThenCompose = kind === "minutes"; summaryTarget = kind; showPicker = true; }} disabled={uploadingTranscript || !!job.iureUpload}><Icon name="layers" size={13} /> Elegir {app.iureSession?.terminology?.case ?? "proyecto"} y generar</button>
              <button class="btn sm" onclick={() => generateWithoutProjectFor(kind)} disabled={uploadingTranscript || !!job.iureUpload}>
                {#if uploadingTranscript}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="upload" size={13} />{/if} Generar sin proyecto
              </button>
            </div>
          {:else if kind === "summary"}
            <p class="hint">Usa la IA de la instancia con tus instrucciones de resumen (Ajustes → Resumen y minuta) y descuenta de la cuota de IA del plan.</p>
            <div class="row-actions">
              <button class="btn sm primary" onclick={() => summaryWithIurefficient(job)} disabled={job.summary.status === "loading"}>
                {#if job.summary.status === "loading"}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="sparkles" size={13} />{/if} {job.summary.status === "done" ? "Volver a generar el resumen" : "Generar resumen con Iurefficient"}
              </button>
              {#if blueprints !== null && blueprintsFor("summary").length}
                {#each blueprintsFor("summary") as b (b.id)}
                  <button class="btn sm" onclick={() => composeWithIurefficient(job, b)}><Icon name="doc" size={13} /> Formato «{b.name}»</button>
                {/each}
              {/if}
            </div>
          {:else if blueprints === null}
            <div>
              <button class="btn sm primary" onclick={loadBlueprints} disabled={loadingBlueprints}>
                {#if loadingBlueprints}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="sparkles" size={13} />{/if} {composedFor(kind).length ? "Generar otro formato" : "Elegir formato y generar"}
              </button>
            </div>
          {:else}
            {#if blueprintError}<p class="hint errtxt">{blueprintError}</p>{/if}
            {#if blueprintsFor(kind).length}
              <div class="bps">
                {#each blueprintsFor(kind) as b (b.id)}
                  <button class="bp" class:suggested={b.suggested} onclick={() => composeWithIurefficient(job, b)} disabled={!!blueprintError}>
                    <strong>{b.name}</strong>{b.suggested ? " · sugerido" : ""}
                    <span class="hint">{b.description || `${b.genre} · ${b.sectionCount} secciones`}</span>
                  </button>
                {/each}
              </div>
            {:else if !blueprintError}
              <p class="hint">La instancia no tiene formatos (blueprints) publicados para generar documentos.</p>
            {/if}
            <div><button class="btn sm ghost" onclick={loadBlueprints} disabled={loadingBlueprints}><Icon name="refresh" size={13} /> Actualizar formatos</button></div>
          {/if}
        </div>
      {/if}
      {#if doc.status === "done" && doc.content}
        <div class="docbar">
          <span class="hint" title={doc.path}>Guardado en {doc.path}{filled ? " · con detalles de la reunión" : ""}</span>
          <button class="btn sm ghost" onclick={() => copyDoc(kind)}><Icon name="copy" size={14} /> Copiar</button>
          <button class="btn sm ghost" onclick={() => openFile(doc.path!)}><Icon name="external" size={14} /> Abrir</button>
          <button class="btn sm ghost" title={editor?.installed ? "Abrir con IureEditor para editarlo y subirlo a Iurefficient" : "IureEditor no está instalada: descargar"} onclick={() => openWithEditor(doc.path!)}><Icon name="edit" size={14} /> IureEditor</button>
          <button class="btn sm ghost" onclick={() => generateDoc(job, kind)} title="Volver a generar"><Icon name="refresh" size={14} /></button>
        </div>
        <div class="md">{@html renderMarkdown(doc.content)}</div>
      {:else if doc.status === "loading"}
        <div class="empty"><span class="spin"><Icon name="loader" size={22} /></span><p class="hint">Generando con {app.settings?.openrouterModel}…</p></div>
      {:else}
        <div class="empty">
          {#if doc.status === "error"}<p class="err">{doc.error}</p>{/if}
          {#if iureLoggedIn() && !hasKey}
            <p class="hint">También puedes generar localmente con OpenRouter configurando una llave en Ajustes.</p>
          {:else if hasKey}
            <button class="btn {iureLoggedIn() ? '' : 'primary'}" onclick={() => generateDoc(job, kind)}>
              <Icon name="sparkles" size={16} /> Generar {kind === "summary" ? "resumen" : "minuta"} con OpenRouter
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
  .stats .iure { color: var(--accent); }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .outputs { display: flex; gap: 6px; flex-shrink: 0; align-items: flex-start; }
  .meta { border-top: 1px solid var(--border); }
  .meta-toggle { width: 100%; display: flex; align-items: center; gap: 8px; padding: 9px 18px; color: var(--text-2); font-weight: 550; text-align: left; }
  .meta-toggle:hover { background: var(--surface-2); }
  .meta-toggle .hint { font-weight: 400; }
  .chev { margin-left: auto; display: inline-flex; transition: transform 0.15s; }
  .chev.up { transform: rotate(180deg); }
  .meta-body { padding: 4px 18px 14px; }
  .engine { display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; margin-bottom: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); }
  .engine-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .composed { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .row-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .spk { display: inline-block; padding: 0 6px; margin-right: 2px; border-radius: 6px; font-size: 12px; font-weight: 650; background: var(--accent-soft); color: var(--accent); }
  .spk.other { background: var(--warn-soft); color: var(--warn); }
  .seg.mine .spk { visibility: hidden; position: absolute; }
  .commits { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 9px; background: var(--surface); }
  .commit { display: grid; grid-template-columns: auto 1fr 180px 150px; gap: 6px; align-items: center; }
  .commit.off { opacity: 0.5; }
  .commit input[type="checkbox"] { accent-color: var(--accent); }
  .commit .input { padding: 5px 8px; font-size: 13px; }
  .cases { display: flex; flex-direction: column; gap: 2px; align-items: flex-start; }
  .engine .label { display: inline-flex; align-items: center; gap: 6px; }
  .errtxt { color: var(--danger); }
  .bps { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 8px; }
  .bp { display: flex; flex-direction: column; gap: 3px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 9px; background: var(--surface); text-align: left; }
  .bp:hover:not(:disabled) { border-color: var(--accent); }
  .bp.suggested { border-color: var(--accent); }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
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
