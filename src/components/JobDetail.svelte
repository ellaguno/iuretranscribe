<script lang="ts">
  import { untrack } from "svelte";
  import { api, type DocKind } from "../lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, appStatus, generateDoc, isActive, metaFilled, metaFilledByUser, openWithEditor, renameJob, retranscribe, toast, type Job } from "../lib/state.svelte";
  import { fmtDuration, fmtSpeed, fmtTimestamp } from "../lib/format";
  import { renderMarkdown } from "../lib/markdown";
  import Icon from "./Icon.svelte";
  import MetaForm from "./MetaForm.svelte";
  import IurePicker from "./IurePicker.svelte";
  import { composeWithIurefficient, downloadComposed, iureConfigured, iureLoggedIn, iureTranscriptDoc, iureWaitAiOptions, notify, pollCompose, segmentLine, summaryWithIurefficient, term, uploadTranscriptOnly } from "../lib/state.svelte";
  import type { IureBlueprint, IureCase, IureCommitment } from "../lib/api";
  import type { ComposedDoc } from "../lib/state.svelte";
  import { isOtherPartyLabel, t, tn } from "../lib/i18n.svelte";

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
      if (!commitments.length) commitError = t("detail.noCommitments");
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
      toast(tn("detail.tasksCreated", n), "success", 7000);
      notify("IureTranscribe", tn("detail.tasksCreated", n));
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
      if (!o.canGenerate) blueprintError = o.reason === "no_text" ? t("detail.noTextYet") : t("detail.cannotGenerate");
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
  // Cabecera plegable (datos, acciones y detalles): se recuerda entre sesiones.
  let headOpen = $state(readHeadOpen());
  function readHeadOpen() {
    try {
      return localStorage.getItem("jobHeadOpen") !== "0";
    } catch {
      return true;
    }
  }
  function toggleHead() {
    headOpen = !headOpen;
    try {
      localStorage.setItem("jobHeadOpen", headOpen ? "1" : "0");
    } catch {}
  }
  let collapsedSummary = $derived(
    [job.result ? fmtDuration(job.result.audioSecs) : isActive(job) ? t("detail.processing") : "", metaSummary].filter(Boolean).join(" · "),
  );

  // ---- Renombrar
  let renaming = $state(false);
  let draftName = $state("");
  let ext = $derived(job.name.includes(".") ? job.name.slice(job.name.lastIndexOf(".")) : "");
  function startRename() {
    if (isActive(job)) return;
    draftName = ext ? job.name.slice(0, -ext.length) : job.name;
    renaming = true;
  }
  let committing = false;
  async function commitRename() {
    if (!renaming || committing) return;
    const stem = draftName.trim();
    if (!stem || stem + ext === job.name) {
      renaming = false;
      return;
    }
    committing = true;
    try {
      if (await renameJob(job, stem)) renaming = false;
    } finally {
      committing = false;
    }
  }

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
        renaming = false;
        showMeta = !metaFilledByUser(job.meta);
      }
    });
  });

  async function copyText() {
    const text = (job.result ? job.result.segments : job.liveSegments).map(segmentLine).filter(Boolean).join("\n");
    await navigator.clipboard.writeText(text);
    toast(t("detail.copied"), "success", 2000);
  }
  async function copyDoc(kind: DocKind) {
    const c = (kind === "summary" ? job.summary : job.minutes).content ?? "";
    await navigator.clipboard.writeText(c);
    toast(t("detail.copiedShort"), "success", 2000);
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
    <div class="trow">
      <div class="ptitle">
        <button class="btn icon ghost fold" onclick={toggleHead} aria-expanded={headOpen} title={headOpen ? t("detail.foldHead") : t("detail.showHead")}>
          <span class="chev" class:up={headOpen}><Icon name="chevron" size={15} /></span>
        </button>
        {#if renaming}
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input rename" bind:value={draftName} autofocus spellcheck="false" aria-label={t("detail.newName")}
            onkeydown={(e) => { if (e.key === "Enter") commitRename(); else if (e.key === "Escape") renaming = false; }}
            onblur={commitRename} />
          {#if ext}<span class="hint">{ext}</span>{/if}
        {:else}
          <h2 title={job.path} ondblclick={startRename}>{job.name}</h2>
          <button class="btn icon ghost" title={isActive(job) ? t("detail.cannotRename") : t("detail.rename")} disabled={isActive(job)} onclick={startRename}><Icon name="edit" size={14} /></button>
        {/if}
        {#if !headOpen && collapsedSummary}<span class="mini hint" title={collapsedSummary}>{collapsedSummary}</span>{/if}
      </div>
      {#if job.result && headOpen}
        <div class="outputs">
          {#each job.result.outputs as o}
            <button class="btn sm" title={o.path} onclick={() => openFile(o.path)}><Icon name="file" size={14} /> .{o.format}</button>
          {/each}
          <button class="btn sm ghost" title={t("detail.showInFolder")} onclick={() => reveal(job.result!.outputs[0]?.path ?? job.result!.outputDir)}><Icon name="folder" size={14} /></button>
          <button class="btn sm ghost" title={t("detail.highQualityTitle")} disabled={app.running} onclick={() => retranscribe(job.id)}><Icon name="refresh" size={14} /> {t("detail.highQuality")}</button>
          {#if iureConfigured() || iureLoggedIn()}
            <button class="btn sm {job.iure ? '' : 'primary'}" title={job.iure ? t("detail.savedIn", { folder: job.iure.folder }) : t("detail.uploadToProject")} disabled={!!job.iureUpload} onclick={() => (showPicker = true)}>
              <Icon name="upload" size={14} /> {job.iure ? t("detail.savedToIure") : t("detail.saveToIure")}
            </button>
            {#if job.iure}
              <button class="btn sm ghost" title={t("detail.openIureBrowser")} onclick={() => openUrl(job.iure!.webUrl)}><Icon name="external" size={14} /></button>
            {/if}
          {:else}
            <button class="btn sm ghost" title={t("detail.connectInSettings")} onclick={() => (app.view = "settings")}><Icon name="cloud" size={14} /> Iurefficient</button>
          {/if}
        </div>
      {/if}
    </div>
    {#if headOpen}
      {#if job.result}
        <div class="stats">
          <span><Icon name="clock" size={13} /> {t("detail.audio", { duration: fmtDuration(job.result.audioSecs) })}</span>
          <span><Icon name="zap" size={13} /> {t("detail.transcribedIn", { duration: fmtDuration(job.result.elapsedSecs), speed: fmtSpeed(job.result.audioSecs, job.result.elapsedSecs) })}</span>
          {#if job.result.detectedLanguage}<span>{t("detail.language", { lang: job.result.detectedLanguage })}</span>{/if}
          {#if job.iure}<span class="iure" title={job.iure.files.join(", ")}><Icon name="cloud" size={13} /> Iurefficient: {job.iure.folder || t("detail.iureRoot")}</span>{/if}
        </div>
      {:else if isActive(job)}
        <div class="stats"><span class="spin"><Icon name="loader" size={13} /></span><span>{t("detail.processing")}</span></div>
      {/if}
    {/if}
  </div>

  {#if headOpen}
    <div class="meta" class:open={showMeta}>
      <button class="meta-toggle" onclick={() => (showMeta = !showMeta)} aria-expanded={showMeta}>
        <Icon name="doc" size={15} />
        <span class="mlabel">{t("detail.meetingDetails")}</span>
        {#if filled}<span class="summary hint" title={metaSummary}>{metaSummary}</span>{:else}<span class="summary hint">{t("detail.meetingDetailsHint")}</span>{/if}
        <span class="chev" class:up={showMeta}><Icon name="chevron" size={14} /></span>
      </button>
      {#if showMeta}
        <div class="meta-body">
          <MetaForm bind:meta={job.meta} hint={t("detail.metaHint") + (docsDone ? t("detail.metaHintDocs") : "")} />
        </div>
      {/if}
    </div>
  {/if}

  <div class="tabs">
    <button class:active={tab === "transcript"} onclick={() => (tab = "transcript")}><Icon name="list" size={15} /> {t("detail.tabTranscript")}</button>
    <button class:active={tab === "summary"} onclick={() => (tab = "summary")} disabled={!job.result}>
      <Icon name="sparkles" size={15} /> {t("detail.tabSummary")} {#if job.summary.status === "done"}<span class="dot"></span>{/if}
    </button>
    <button class:active={tab === "minutes"} onclick={() => (tab = "minutes")} disabled={!job.result}>
      <Icon name="doc" size={15} /> {t("detail.tabMinutes")} {#if job.minutes.status === "done"}<span class="dot"></span>{/if}
    </button>
    <span class="spacer"></span>
    {#if tab === "transcript" && segments.length}
      <button class="btn sm ghost" onclick={copyText}><Icon name="copy" size={14} /> {t("detail.copyText")}</button>
    {/if}
  </div>

  {#if tab === "transcript"}
    <div class="body scroll" bind:this={listEl}>
      {#if segments.length === 0}
        <div class="empty hint">
          {#if job.status === "queued"}{t("detail.willAppear")}{:else if job.status === "error"}{job.error}{:else if isActive(job)}{t("detail.waitingSegments")}{:else}{t("detail.noText")}{/if}
        </div>
      {:else}
        {#each segments as s, i (i)}
          <div class="seg" class:mine={s.speaker && i > 0 && segments[i - 1].speaker === s.speaker}>
            <span class="ts">{fmtTimestamp(s.startMs)}</span>
            <span class="txt">{#if s.speaker}<span class="spk" class:other={isOtherPartyLabel(s.speaker)}>{s.speaker}</span> {/if}{s.text}</span>
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
            <span class="label"><Icon name="cloud" size={14} /> {kind === "minutes" ? t("detail.engineMinutes") : t("detail.engineSummary")}</span>
            <span class="hint">{t("detail.engineHint")}</span>
          </div>
          {#each composedFor(kind) as c (c.taskId)}
            <div class="composed">
              {#if c.state === "SUCCESS"}
                <span class="pill success"><Icon name="check" size={11} stroke={3} /> {c.blueprintName}</span>
                {#if c.localPath}
                  <button class="btn sm" onclick={() => openFile(c.localPath!)}><Icon name="file" size={13} /> {t("detail.openExt", { ext: c.localPath.split(".").pop()?.toUpperCase() ?? "" })}</button>
                  {#if editorCanOpen(c.localPath)}
                    <button class="btn sm ghost" title={editor?.installed ? t("detail.openWithEditor") : t("detail.editorMissing")} onclick={() => openWithEditor(c.localPath!)}><Icon name="edit" size={13} /> IureEditor</button>
                  {/if}
                  <button class="btn sm ghost" title={t("detail.showInFolder")} onclick={() => reveal(c.localPath!)}><Icon name="folder" size={13} /></button>
                {:else if c.documentId}
                  <button class="btn sm ghost" onclick={() => downloadComposed(job, c)}><Icon name="download" size={13} /> {t("detail.downloadNext")}</button>
                {/if}
                {#if c.link}<button class="btn sm ghost" title={job.iure?.caseId ? t("detail.projectDocs") : t("detail.generalDocs")} onclick={() => openUrl(c.link!)}><Icon name="external" size={13} /> {t("detail.viewInIure")}</button>{/if}
                {#if c.documentId && kind === "minutes"}
                  <button class="btn sm {commitFor === c.documentId ? 'ghost' : 'primary'}" onclick={() => loadCommitments(c.documentId!)} disabled={commitLoading}><Icon name="check" size={13} /> {t("detail.commitmentsToTasks")}</button>
                {/if}
              {:else if c.error}
                <span class="pill danger">{c.blueprintName}: {c.error}</span>
              {:else}
                <span class="pill accent"><span class="spin"><Icon name="loader" size={11} /></span> {c.blueprintName}: {c.section ? t("detail.section", { current: c.current, total: c.total, section: c.section }) : t("detail.inQueue")}</span>
              {/if}
            </div>
          {/each}
          {#if commitFor && kind === "minutes"}
            <div class="commits">
              <div class="engine-head">
                <span class="label"><Icon name="list" size={14} /> {t("detail.detectedCommitments")}</span>
                <button class="btn icon ghost" onclick={() => (commitFor = null)} aria-label={t("detail.close")}><Icon name="x" size={14} /></button>
              </div>
              {#if commitLoading}
                <p class="hint"><span class="spin"><Icon name="loader" size={13} /></span> {t("detail.readingMinutes")}</p>
              {:else}
                {#if commitError}<p class="hint errtxt">{commitError}</p>{/if}
                {#each commitments as c, i (i)}
                  <div class="commit" class:off={!c.include}>
                    <input type="checkbox" bind:checked={c.include} />
                    <input class="input" bind:value={c.title} placeholder={t("detail.taskTitle")} />
                    <input class="input who" bind:value={c.assigneeName} placeholder={c.assignedToId ? t("detail.assignee") : t("detail.assigneeUnknown")} title={c.assignedToId ? t("detail.assigneeKnown") : t("detail.assigneeNone")} />
                    <input class="input date" type="date" bind:value={c.dueDate} title={c.dueHint ?? ""} />
                  </div>
                {/each}
                {#if commitments.length}
                  {#if !commitCaseId}
                    <div class="field">
                      <span class="label">{t("detail.tasksNeedCase", { case: term("case").toLowerCase() })}</span>
                      <input class="input" placeholder={t("detail.searchCase", { case: term("case").toLowerCase() })} bind:value={caseQuery} oninput={onCaseQuery} />
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
                      {#if commitApplying}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="check" size={13} />{/if} {tn("detail.createTasks", commitments.filter((c) => c.include && c.title.trim()).length)}
                    </button>
                    {#if commitDone !== null}<span class="pill success">{t("detail.created", { count: commitDone })}</span>{/if}
                  </div>
                {/if}
              {/if}
            </div>
          {/if}
          {#if !iureTranscriptDoc(job)}
            <p class="hint">{t("detail.uploadFirst", { case: app.iureSession?.terminology?.case ?? t("term.case") })}</p>
            <div class="row-actions">
              <button class="btn sm primary" onclick={() => { pickerThenCompose = kind === "minutes"; summaryTarget = kind; showPicker = true; }} disabled={uploadingTranscript || !!job.iureUpload}><Icon name="layers" size={13} /> {t("detail.chooseAndGenerate", { case: app.iureSession?.terminology?.case ?? t("term.case") })}</button>
              <button class="btn sm" onclick={() => generateWithoutProjectFor(kind)} disabled={uploadingTranscript || !!job.iureUpload}>
                {#if uploadingTranscript}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="upload" size={13} />{/if} {t("detail.generateNoProject")}
              </button>
            </div>
          {:else if kind === "summary"}
            <p class="hint">{t("detail.summaryHint")}</p>
            <div class="row-actions">
              <button class="btn sm primary" onclick={() => summaryWithIurefficient(job)} disabled={job.summary.status === "loading"}>
                {#if job.summary.status === "loading"}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="sparkles" size={13} />{/if} {job.summary.status === "done" ? t("detail.regenerateSummary") : t("detail.generateSummaryIure")}
              </button>
              {#if blueprints !== null && blueprintsFor("summary").length}
                {#each blueprintsFor("summary") as b (b.id)}
                  <button class="btn sm" onclick={() => composeWithIurefficient(job, b)}><Icon name="doc" size={13} /> {t("detail.formatName", { name: b.name })}</button>
                {/each}
              {/if}
            </div>
          {:else if blueprints === null}
            <div>
              <button class="btn sm primary" onclick={loadBlueprints} disabled={loadingBlueprints}>
                {#if loadingBlueprints}<span class="spin"><Icon name="loader" size={13} /></span>{:else}<Icon name="sparkles" size={13} />{/if} {composedFor(kind).length ? t("detail.generateOther") : t("detail.chooseFormat")}
              </button>
            </div>
          {:else}
            {#if blueprintError}<p class="hint errtxt">{blueprintError}</p>{/if}
            {#if blueprintsFor(kind).length}
              <div class="bps">
                {#each blueprintsFor(kind) as b (b.id)}
                  <button class="bp" class:suggested={b.suggested} onclick={() => composeWithIurefficient(job, b)} disabled={!!blueprintError}>
                    <strong>{b.name}</strong>{b.suggested ? t("detail.suggested") : ""}
                    <span class="hint">{b.description || t("detail.sections", { genre: b.genre, count: b.sectionCount })}</span>
                  </button>
                {/each}
              </div>
            {:else if !blueprintError}
              <p class="hint">{t("detail.noBlueprints")}</p>
            {/if}
            <div><button class="btn sm ghost" onclick={loadBlueprints} disabled={loadingBlueprints}><Icon name="refresh" size={13} /> {t("detail.refreshFormats")}</button></div>
          {/if}
        </div>
      {/if}
      {#if doc.status === "done" && doc.content}
        <div class="docbar">
          <span class="hint" title={doc.path}>{t("detail.savedAt", { path: doc.path ?? "" })}{filled ? t("detail.withMeta") : ""}</span>
          <button class="btn sm ghost" onclick={() => copyDoc(kind)}><Icon name="copy" size={14} /> {t("detail.copy")}</button>
          <button class="btn sm ghost" onclick={() => openFile(doc.path!)}><Icon name="external" size={14} /> {t("detail.open")}</button>
          <button class="btn sm ghost" title={editor?.installed ? t("detail.editorTitle") : t("detail.editorMissing")} onclick={() => openWithEditor(doc.path!)}><Icon name="edit" size={14} /> IureEditor</button>
          <button class="btn sm ghost" onclick={() => generateDoc(job, kind)} title={t("detail.regenerate")}><Icon name="refresh" size={14} /></button>
        </div>
        <div class="md">{@html renderMarkdown(doc.content)}</div>
      {:else if doc.status === "loading"}
        <div class="empty"><span class="spin"><Icon name="loader" size={22} /></span><p class="hint">{t("detail.generatingWith", { model: app.settings?.openrouterModel ?? "" })}</p></div>
      {:else}
        <div class="empty">
          {#if doc.status === "error"}<p class="err">{doc.error}</p>{/if}
          {#if iureLoggedIn() && !hasKey}
            <p class="hint">{t("detail.localAlso")}</p>
          {:else if hasKey}
            <button class="btn {iureLoggedIn() ? '' : 'primary'}" onclick={() => generateDoc(job, kind)}>
              <Icon name="sparkles" size={16} /> {kind === "summary" ? t("detail.generateSummaryOR") : t("detail.generateMinutesOR")}
            </button>
            <p class="hint">{t("detail.willSend", { model: app.settings?.openrouterModel ?? "" })}</p>
            {#if filled}
              <p class="hint ok"><Icon name="check" size={13} /> {t("detail.metaIncluded", { summary: metaSummary })}</p>
            {:else}
              <p class="hint"><Icon name="info" size={13} /> {t("detail.noMeta")} <button class="link" onclick={() => (showMeta = true)}>{t("detail.captureMeta")}</button> {t("detail.captureMetaTail")}</p>
            {/if}
          {:else}
            <p class="hint">{t("detail.needKey")}</p>
            <button class="btn" onclick={() => (app.view = "settings")}><Icon name="settings" size={15} /> {t("detail.goSettings")}</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .phead { display: flex; flex-direction: column; gap: 6px; padding: 12px 18px 10px 10px; }
  .trow { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px 14px; }
  .ptitle { flex: 1 1 260px; min-width: 0; display: flex; align-items: center; gap: 4px; }
  .ptitle h2 { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: text; }
  .ptitle .rename { flex: 1; min-width: 120px; max-width: 520px; padding: 4px 8px; font-size: 15px; font-weight: 600; }
  .ptitle .mini { flex: 1 1 0; min-width: 0; margin-left: 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fold { flex-shrink: 0; }
  .stats { display: flex; flex-wrap: wrap; gap: 4px 14px; padding-left: 38px; font-size: 12.5px; color: var(--muted); }
  .stats span { display: inline-flex; align-items: center; gap: 5px; }
  .stats .iure { color: var(--accent); }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .outputs { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; margin-left: auto; }
  .meta { border-top: 1px solid var(--border); }
  .meta-toggle { width: 100%; display: flex; align-items: center; gap: 8px; padding: 9px 18px; color: var(--text-2); font-weight: 550; text-align: left; }
  .meta-toggle:hover { background: var(--surface-2); }
  .meta-toggle .hint { font-weight: 400; }
  .meta-toggle .mlabel { flex-shrink: 0; }
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
