import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  modelId: string;
  language: string;
  translate: boolean;
  formats: string[];
  outputMode: "same" | "custom";
  outputDir: string | null;
  useGpu: boolean;
  threads: number;
  beamSize: number;
  autoSummary: boolean;
  autoMinutes: boolean;
  openrouterApiKey: string;
  openrouterModel: string;
  summaryPrompt: string;
  minutesPrompt: string;
  theme: "system" | "light" | "dark";
}

export interface ModelInfo {
  id: string;
  name: string;
  fileName: string;
  sizeMb: number;
  description: string;
  quality: "alta" | "media" | "baja";
  recommended: boolean;
  downloaded: boolean;
  downloading: boolean;
  path: string;
}

export interface SystemInfo {
  backend: string;
  cpuThreads: number;
  platform: string;
  modelsDir: string;
  settingsPath: string;
  ffmpegAvailable: boolean;
  version: string;
  supportedExtensions: string[];
}

export interface FileProbe {
  path: string;
  name: string;
  sizeBytes: number;
  durationSecs: number | null;
  supported: boolean;
}

export interface Segment {
  startMs: number;
  endMs: number;
  text: string;
}

export interface OutputFile {
  format: string;
  path: string;
}

export interface TranscriptResult {
  jobId: string;
  segments: Segment[];
  text: string;
  audioSecs: number;
  elapsedSecs: number;
  outputs: OutputFile[];
  outputDir: string;
  baseName: string;
  detectedLanguage: string | null;
}

export type DocKind = "summary" | "minutes";

export interface DocumentResult {
  kind: DocKind;
  content: string;
  path: string;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number | null;
  status: "downloading" | "done" | "cancelled" | "error";
  message: string | null;
}

export interface ProgressEvent {
  jobId: string;
  stage: "decoding" | "loading" | "transcribing";
  percent: number;
}

export interface SegmentEvent {
  jobId: string;
  segment: Segment;
}

export const api = {
  systemInfo: () => invoke<SystemInfo>("system_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<boolean>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  unloadModel: () => invoke<void>("unload_model"),
  probeFiles: (paths: string[]) => invoke<FileProbe[]>("probe_files", { paths }),
  transcribeFile: (jobId: string, path: string) =>
    invoke<TranscriptResult>("transcribe_file", { request: { jobId, path } }),
  cancelJob: (jobId: string) => invoke<boolean>("cancel_job", { jobId }),
  generateDocument: (kind: DocKind, text: string, outputDir: string, baseName: string) =>
    invoke<DocumentResult>("generate_document", { request: { kind, text, outputDir, baseName } }),
  loadJobs: () => invoke<string>("load_jobs"),
  saveJobs: (json: string) => invoke<void>("save_jobs", { json }),
  loadDocuments: (outputDir: string, baseName: string) =>
    invoke<{ summary: DocumentResult | null; minutes: DocumentResult | null }>("load_documents", { outputDir, baseName }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
};
