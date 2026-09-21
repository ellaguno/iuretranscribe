import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  api,
  type IureUploadProgress,
  type DeviceList,
  type DocKind,
  type RecordingStatus,
  type DownloadProgress,
  type ModelInfo,
  type ProgressEvent,
  type Segment,
  type SegmentEvent,
  type Settings,
  type SystemInfo,
  type TranscriptResult,
} from "./api";

export type View = "transcribe" | "record" | "models" | "settings";
export type JobStatus = "queued" | "decoding" | "loading" | "transcribing" | "done" | "error" | "cancelled";

export interface DocState {
  status: "idle" | "loading" | "done" | "error";
  content?: string;
  path?: string;
  error?: string;
}

/** Datos de la reunión que el usuario puede capturar en cualquier momento. */
export interface JobMeta {
  participants: string;
  date: string;
  place: string;
  notes: string;
}

/** Fecha de hoy en español, p. ej. «19 de septiembre de 2026». */
export function todayLabel(): string {
  try {
    return new Date().toLocaleDateString("es-MX", { day: "numeric", month: "long", year: "numeric" });
  } catch {
    return new Date().toISOString().slice(0, 10);
  }
}

export function emptyMeta(): JobMeta {
  return { participants: "", date: todayLabel(), place: "", notes: "" };
}

/** ¿El usuario capturó algo más allá de la fecha sugerida? */
export function metaFilledByUser(m: JobMeta): boolean {
  return !!(m.place.trim() || m.participants.trim() || m.notes.trim() || (m.date.trim() && m.date.trim() !== todayLabel()));
}

export function metaToContext(m: JobMeta): string {
  const lines: string[] = [];
  if (m.date.trim()) lines.push(`Fecha: ${m.date.trim()}`);
  if (m.place.trim()) lines.push(`Lugar: ${m.place.trim()}`);
  if (m.participants.trim()) {
    const people = m.participants.split(/\n|,|;/).map((x) => x.trim()).filter(Boolean);
    lines.push(`Participantes: ${people.join(", ")}`);
  }
  if (m.notes.trim()) lines.push(`Notas adicionales: ${m.notes.trim()}`);
  return lines.join("\n");
}

export function metaFilled(m: JobMeta): boolean {
  return !!(m.date.trim() || m.place.trim() || m.participants.trim() || m.notes.trim());
}

/** Resultado de guardar un trabajo en Iurefficient. */
export interface IureSaved {
  folder: string;
  webUrl: string;
  files: string[];
  savedAt: number;
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
  meta: JobMeta;
  iure?: IureSaved;
  iureUpload?: { fileName: string; index: number; totalFiles: number; sent: number; total: number } | null;
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
  devices: null as DeviceList | null,
  recording: { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false } as RecordingStatus,
  /** Segmentos transcritos en vivo durante la grabación actual. */
  liveSegments: [] as Segment[],
  recordingBusy: false,
  /** Detalles capturados durante la grabación; se adjuntan al trabajo al detenerla. */
  pendingMeta: { participants: "", date: "", place: "", notes: "" } as JobMeta,
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
    meta: { participants: j.meta.participants, date: j.meta.date, place: j.meta.place, notes: j.meta.notes },
    iure: j.iure,
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
    meta: { ...emptyMeta(), ...(j.meta ?? {}) },
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
  pollRecording().then(() => {
    if (app.recording.active && !pollTimer) pollTimer = setInterval(pollRecording, 250);
  });
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
  await listen<IureUploadProgress>("iure-upload-progress", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (job) job.iureUpload = { fileName: e.payload.fileName, index: e.payload.index, totalFiles: e.payload.totalFiles, sent: e.payload.sent, total: e.payload.total };
  });
  await listen<Segment>("live-segment", (e) => {
    app.liveSegments.push(e.payload);
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

export async function addFiles(paths: string[]): Promise<Job[]> {
  const fresh = paths.filter((p) => !app.jobs.some((j) => j.path === p));
  if (!fresh.length) return [];
  const probes = await api.probeFiles(fresh);
  let skipped = 0;
  const added: Job[] = [];
  for (const p of probes) {
    if (!p.supported) {
      skipped++;
      continue;
    }
    added.push({
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
      meta: emptyMeta(),
    });
  }
  app.jobs.push(...added);
  if (!app.selectedJobId && app.jobs.length) app.selectedJobId = app.jobs[0].id;
  if (skipped) toast(`${skipped} archivo(s) omitido(s): formato no compatible`, "error");
  app.view = "transcribe";
  return app.jobs.filter((j) => added.some((a) => a.id === j.id));
}

// ---------------------------------------------------------------------------
// Grabación
// ---------------------------------------------------------------------------
let pollTimer: ReturnType<typeof setInterval> | undefined;

export async function loadDevices() {
  try {
    app.devices = await api.listAudioDevices();
  } catch (e) {
    toast(`No se pudieron listar los dispositivos de audio: ${e}`, "error");
  }
}

async function pollRecording() {
  try {
    app.recording = await api.recordingStatus();
  } catch {
    /* ignorar */
  }
  if (!app.recording.active) stopPolling();
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = undefined;
}

export async function startRecording() {
  if (app.recordingBusy || app.recording.active) return;
  app.recordingBusy = true;
  app.liveSegments = [];
  try {
    const info = await api.startRecording();
    if (info.liveNote) toast(info.liveNote, "info", 7000);
    await pollRecording();
    if (!pollTimer) pollTimer = setInterval(pollRecording, 250);
  } catch (e) {
    toast(String(e), "error", 8000);
  } finally {
    app.recordingBusy = false;
  }
}

export async function stopRecording() {
  if (app.recordingBusy || !app.recording.active) return;
  app.recordingBusy = true;
  stopPolling();
  try {
    const recordingStartedAt = Date.now() - app.recording.elapsedSecs * 1000;
    const wasLive = app.recording.live;
    const res = await api.stopRecording();
    app.recording = { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false };
    const [job] = await addFiles([res.path]);
    const live = app.liveSegments.map((s) => ({ ...s }));
    app.liveSegments = [];
    if (job) {
      job.meta = { ...app.pendingMeta };
      job.liveSegments = live;
      app.selectedJobId = job.id;
    }
    app.pendingMeta = emptyMeta();
    toast(`Grabación guardada (${Math.round(res.durationSecs)} s)`, "success");
    const s = app.settings;
    if (job && wasLive && live.length && s?.liveIsFinal) {
      // La transcripción en vivo es el resultado final: se escriben las salidas sin repetir.
      await finalizeFromLive(job, live, res.durationSecs, (Date.now() - recordingStartedAt) / 1000);
    } else if (s?.autoTranscribeRecording) {
      await startQueue();
    }
  } catch (e) {
    app.recording = { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false };
    toast(String(e), "error", 9000);
  } finally {
    app.recordingBusy = false;
  }
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

async function finalizeFromLive(job: Job, segments: Segment[], audioSecs: number, elapsedSecs: number) {
  job.status = "decoding";
  try {
    const result = await api.saveLiveTranscript(job.id, job.path, segments, audioSecs, elapsedSecs);
    job.result = result;
    job.durationSecs = result.audioSecs;
    job.status = "done";
    job.percent = 100;
    job.startedAt = Date.now() - elapsedSecs * 1000;
    job.finishedAt = Date.now();
    const s = app.settings;
    if (s?.autoSummary) await generateDoc(job, "summary");
    if (s?.autoMinutes) await generateDoc(job, "minutes");
  } catch (e) {
    job.status = "queued";
    toast(`No se pudo guardar la transcripción en vivo: ${e}. Se transcribirá de nuevo.`, "error", 8000);
    await startQueue();
  }
}

export function iureConfigured(): boolean {
  const s = app.settings;
  return !!(s && s.iureDomain.trim() && s.iureEmail.trim() && s.iureAppPassword.trim());
}

/** Archivos locales que se subirían a Iurefficient para un trabajo terminado. */
export function iureFilesFor(job: Job, includeMedia: boolean): string[] {
  const files: string[] = [];
  if (includeMedia) files.push(job.path);
  if (job.result) for (const o of job.result.outputs) files.push(o.path);
  if (job.summary.status === "done" && job.summary.path) files.push(job.summary.path);
  if (job.minutes.status === "done" && job.minutes.path) files.push(job.minutes.path);
  return files;
}

export async function saveToIurefficient(job: Job, folder: string, includeMedia: boolean): Promise<boolean> {
  const files = iureFilesFor(job, includeMedia);
  if (!files.length) {
    toast("No hay archivos que subir todavía", "error");
    return false;
  }
  job.iureUpload = { fileName: "", index: 0, totalFiles: files.length, sent: 0, total: 0 };
  try {
    const res = await api.iureUpload(job.id, folder, files);
    job.iure = { folder: res.folder, webUrl: res.webUrl, files: res.uploaded.map((u) => u.fileName), savedAt: Date.now() };
    if (app.settings) app.settings.iureLastFolder = res.folder;
    const versions = res.uploaded.filter((u) => !u.created).length;
    toast(`Guardado en Iurefficient (${res.uploaded.length} archivo(s)${versions ? `, ${versions} como versión nueva` : ""})`, "success", 6000);
    return true;
  } catch (e) {
    toast(`No se pudo guardar en Iurefficient: ${e}`, "error", 9000);
    return false;
  } finally {
    job.iureUpload = null;
  }
}

/** Vuelve a transcribir un archivo ya terminado con el proceso completo (calidad alta). */
export async function retranscribe(id: string) {
  retryJob(id);
  await startQueue();
}

export async function generateDoc(job: Job, kind: DocKind) {
  if (!job.result) return;
  const slot = kind === "summary" ? job.summary : job.minutes;
  if (slot.status === "loading") return;
  slot.status = "loading";
  slot.error = undefined;
  try {
    const res = await api.generateDocument(kind, job.result.text, job.result.outputDir, job.result.baseName, metaToContext(job.meta));
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
