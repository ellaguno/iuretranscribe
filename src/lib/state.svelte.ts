import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  api,
  type DocKind,
  type DownloadProgress,
  type ModelInfo,
  type ProgressEvent,
  type Segment,
  type SegmentEvent,
  type Settings,
  type SystemInfo,
  type TranscriptResult,
} from "./api";

export type View = "transcribe" | "models" | "settings";
export type JobStatus = "queued" | "decoding" | "loading" | "transcribing" | "done" | "error" | "cancelled";

export interface DocState {
  status: "idle" | "loading" | "done" | "error";
  content?: string;
  path?: string;
  error?: string;
}

export interface Job {
  id: string;
  path: string;
  name: string;
  sizeBytes: number;
  durationSecs: number | null;
  status: JobStatus;
  percent: number;
  error?: string;
  result?: TranscriptResult;
  liveSegments: Segment[];
  summary: DocState;
  minutes: DocState;
  startedAt?: number;
  finishedAt?: number;
}

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export interface DownloadState {
  downloaded: number;
  total: number | null;
}

export const app = $state({
  ready: false,
  view: "transcribe" as View,
  settings: null as Settings | null,
  sys: null as SystemInfo | null,
  models: [] as ModelInfo[],
  jobs: [] as Job[],
  selectedJobId: null as string | null,
  running: false,
  dragging: false,
  downloads: {} as Record<string, DownloadState>,
  toasts: [] as Toast[],
  now: Date.now(),
});

let toastSeq = 0;
let stopRequested = false;

export function toast(text: string, kind: Toast["kind"] = "info", ms = 4500) {
  const id = ++toastSeq;
  app.toasts.push({ id, kind, text });
  setTimeout(() => {
    const i = app.toasts.findIndex((t) => t.id === id);
    if (i >= 0) app.toasts.splice(i, 1);
  }, ms);
}

export function applyTheme() {
  const pref = app.settings?.theme ?? "system";
  const dark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

/** Forma en que se guardan los trabajos en disco (sin segmentos en vivo). */
type StoredJob = Omit<Job, "liveSegments">;

function serializeJobs(): string {
  const stored: StoredJob[] = app.jobs.map((j) => ({
    id: j.id,
    path: j.path,
    name: j.name,
    sizeBytes: j.sizeBytes,
    durationSecs: j.durationSecs,
    status: isActive(j) ? "queued" : j.status,
    percent: isActive(j) ? 0 : j.percent,
    error: j.error,
    result: j.result,
    summary: j.summary.status === "loading" ? { status: "idle" } : j.summary,
    minutes: j.minutes.status === "loading" ? { status: "idle" } : j.minutes,
    startedAt: j.startedAt,
    finishedAt: j.finishedAt,
  }));
  return JSON.stringify(stored);
}

let persistTimer: ReturnType<typeof setTimeout> | undefined;
function startPersistence() {
  $effect.root(() => {
    $effect(() => {
      const json = serializeJobs(); // lee sólo los campos persistidos → se re-ejecuta cuando cambian
      clearTimeout(persistTimer);
      persistTimer = setTimeout(() => api.saveJobs(json).catch((e) => console.error("saveJobs", e)), 400);
    });
  });
}

async function restoreJobs() {
  let stored: StoredJob[] = [];
  try {
    stored = JSON.parse(await api.loadJobs());
  } catch (e) {
    console.error("loadJobs", e);
    return;
  }
  if (!Array.isArray(stored) || !stored.length) return;
  const jobs: Job[] = stored.map((j) => ({
    ...j,
    status: isActive(j as Job) ? "queued" : j.status,
    liveSegments: [],
    summary: j.summary ?? { status: "idle" },
    minutes: j.minutes ?? { status: "idle" },
  }));
  // Recupera resumen/minuta que ya existan en disco (p. ej. tras un reinicio).
  await Promise.all(
    jobs
      .filter((j) => j.result && (j.summary.status !== "done" || j.minutes.status !== "done"))
      .map(async (j) => {
        try {
          const docs = await api.loadDocuments(j.result!.outputDir, j.result!.baseName);
          if (docs.summary && j.summary.status !== "done") j.summary = { status: "done", content: docs.summary.content, path: docs.summary.path };
          if (docs.minutes && j.minutes.status !== "done") j.minutes = { status: "done", content: docs.minutes.content, path: docs.minutes.path };
        } catch { /* sin documentos */ }
      }),
  );
  app.jobs = jobs;
  app.selectedJobId = jobs.find((j) => j.status === "done")?.id ?? jobs[0]?.id ?? null;
  const interrupted = stored.filter((j) => isActive(j as Job)).length;
  if (interrupted) toast(`${interrupted} transcripción(es) se interrumpieron al cerrar la app y volvieron a la cola`, "info", 7000);
}

export async function init() {
  const [settings, sys, models] = await Promise.all([api.getSettings(), api.systemInfo(), api.listModels()]);
  app.settings = settings;
  app.sys = sys;
  app.models = models;
  applyTheme();
  await restoreJobs();
  startPersistence();
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

  await listen<ProgressEvent>("job-progress", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (!job || job.status === "done" || job.status === "error" || job.status === "cancelled") return;
    job.status = e.payload.stage;
    job.percent = e.payload.percent;
  });
  await listen<SegmentEvent>("job-segment", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (job && job.status !== "done") job.liveSegments.push(e.payload.segment);
  });
  await listen<DownloadProgress>("model-download-progress", (e) => {
    const p = e.payload;
    if (p.status === "downloading") {
      app.downloads[p.id] = { downloaded: p.downloaded, total: p.total };
    } else {
      delete app.downloads[p.id];
      refreshModels();
      const name = app.models.find((m) => m.id === p.id)?.name ?? p.id;
      if (p.status === "done") toast(`Modelo ${name} descargado`, "success");
      else if (p.status === "error") toast(`Error descargando ${name}: ${p.message ?? ""}`, "error", 8000);
    }
  });
  await getCurrentWebview().onDragDropEvent((event) => {
    const t = event.payload.type;
    if (t === "enter" || t === "over") app.dragging = true;
    else if (t === "leave") app.dragging = false;
    else if (t === "drop") {
      app.dragging = false;
      addFiles(event.payload.paths);
    }
  });
  setInterval(() => (app.now = Date.now()), 1000);
  app.ready = true;
}

export async function refreshModels() {
  app.models = await api.listModels();
}

export async function saveSettings(patch: Partial<Settings>) {
  if (!app.settings) return;
  Object.assign(app.settings, patch);
  applyTheme();
  try {
    await api.saveSettings($state.snapshot(app.settings));
  } catch (e) {
    toast(`No se pudieron guardar los ajustes: ${e}`, "error");
  }
}

export function selectedModel(): ModelInfo | undefined {
  return app.models.find((m) => m.id === app.settings?.modelId);
}

export function downloadedModels(): ModelInfo[] {
  return app.models.filter((m) => m.downloaded);
}

export async function addFiles(paths: string[]) {
  const fresh = paths.filter((p) => !app.jobs.some((j) => j.path === p));
  if (!fresh.length) return;
  const probes = await api.probeFiles(fresh);
  let skipped = 0;
  for (const p of probes) {
    if (!p.supported) {
      skipped++;
      continue;
    }
    app.jobs.push({
      id: crypto.randomUUID(),
      path: p.path,
      name: p.name,
      sizeBytes: p.sizeBytes,
      durationSecs: p.durationSecs,
      status: "queued",
      percent: 0,
      liveSegments: [],
      summary: { status: "idle" },
      minutes: { status: "idle" },
    });
  }
  if (!app.selectedJobId && app.jobs.length) app.selectedJobId = app.jobs[0].id;
  if (skipped) toast(`${skipped} archivo(s) omitido(s): formato no compatible`, "error");
  app.view = "transcribe";
}

export function removeJob(id: string) {
  const i = app.jobs.findIndex((j) => j.id === id);
  if (i < 0) return;
  const job = app.jobs[i];
  if (isActive(job)) return;
  app.jobs.splice(i, 1);
  if (app.selectedJobId === id) app.selectedJobId = app.jobs[0]?.id ?? null;
}

export function clearFinished() {
  app.jobs = app.jobs.filter((j) => isActive(j) || j.status === "queued");
  if (!app.jobs.some((j) => j.id === app.selectedJobId)) app.selectedJobId = app.jobs[0]?.id ?? null;
}

export function retryJob(id: string) {
  const job = app.jobs.find((j) => j.id === id);
  if (!job || isActive(job)) return;
  job.status = "queued";
  job.percent = 0;
  job.error = undefined;
  job.result = undefined;
  job.liveSegments = [];
  job.summary = { status: "idle" };
  job.minutes = { status: "idle" };
}

export function isActive(job: Job): boolean {
  return job.status === "decoding" || job.status === "loading" || job.status === "transcribing";
}

export function queuedCount(): number {
  return app.jobs.filter((j) => j.status === "queued").length;
}

export async function startQueue() {
  if (app.running) return;
  const model = selectedModel();
  if (!model?.downloaded) {
    toast("Primero descarga un modelo en la sección Modelos", "error");
    app.view = "models";
    return;
  }
  app.running = true;
  stopRequested = false;
  try {
    while (!stopRequested) {
      const job = app.jobs.find((j) => j.status === "queued");
      if (!job) break;
      await runJob(job);
    }
  } finally {
    app.running = false;
  }
}

export function stopQueue() {
  stopRequested = true;
  for (const job of app.jobs) if (isActive(job)) api.cancelJob(job.id);
}

export async function cancelJob(id: string) {
  const job = app.jobs.find((j) => j.id === id);
  if (job && isActive(job)) await api.cancelJob(id);
}

async function runJob(job: Job) {
  job.status = "decoding";
  job.percent = 0;
  job.liveSegments = [];
  job.error = undefined;
  job.startedAt = Date.now();
  if (!app.selectedJobId || !app.jobs.some((j) => j.id === app.selectedJobId && isActive(j))) app.selectedJobId = job.id;
  try {
    const result = await api.transcribeFile(job.id, job.path);
    job.result = result;
    job.durationSecs = result.audioSecs;
    job.status = "done";
    job.percent = 100;
    job.finishedAt = Date.now();
    const s = app.settings;
    if (s?.autoSummary) await generateDoc(job, "summary");
    if (s?.autoMinutes) await generateDoc(job, "minutes");
  } catch (e) {
    const msg = String(e);
    job.finishedAt = Date.now();
    job.status = /cancelad/i.test(msg) ? "cancelled" : "error";
    job.error = msg;
    if (job.status === "error") toast(`Error en ${job.name}: ${msg}`, "error", 8000);
  }
}

export async function generateDoc(job: Job, kind: DocKind) {
  if (!job.result) return;
  const slot = kind === "summary" ? job.summary : job.minutes;
  if (slot.status === "loading") return;
  slot.status = "loading";
  slot.error = undefined;
  try {
    const res = await api.generateDocument(kind, job.result.text, job.result.outputDir, job.result.baseName);
    slot.status = "done";
    slot.content = res.content;
    slot.path = res.path;
  } catch (e) {
    slot.status = "error";
    slot.error = String(e);
  }
}

export async function downloadModel(id: string) {
  app.downloads[id] = { downloaded: 0, total: null };
  await refreshModels();
  try {
    await api.downloadModel(id);
  } catch (e) {
    delete app.downloads[id];
    await refreshModels();
    if (!/cancelad/i.test(String(e))) toast(String(e), "error", 8000);
  }
}

export async function cancelDownload(id: string) {
  await api.cancelDownload(id);
}

export async function deleteModel(id: string) {
  try {
    await api.deleteModel(id);
    await refreshModels();
    toast("Modelo eliminado", "info");
  } catch (e) {
    toast(String(e), "error");
  }
}
