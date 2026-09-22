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
  recordingsDir: string | null;
  recordMic: boolean;
  recordSystem: boolean;
  micDevice: string | null;
  autoTranscribeRecording: boolean;
  liveTranscription: boolean;
  liveChunkSecs: number;
  liveIsFinal: boolean;
  iureDomain: string;
  iureEmail: string;
  iureAppPassword: string;
  iureUploadMedia: boolean;
  iureLastFolder: string | null;
  iureAutoCompose: boolean;
  speakerSplit: boolean;
  myName: string;
  checkUpdates: boolean;
}

export interface IureEntry {
  name: string;
  path: string;
  isFolder: boolean;
  canUpload: boolean;
  size: number | null;
}

export interface IureListing {
  path: string;
  canUpload: boolean;
  entries: IureEntry[];
}

export interface IureConnectionInfo {
  webUrl: string;
  rootFolders: string[];
}

export interface IureUploaded {
  fileName: string;
  remotePath: string;
  created: boolean;
  renamedFrom: string | null;
}

export interface IureUploadResult {
  folder: string;
  webUrl: string;
  uploaded: IureUploaded[];
}

export interface IureLoginResult {
  loggedIn: boolean;
  requiresTotp: boolean;
  totpToken: string | null;
  name: string | null;
}

export interface IureTerminology {
  case: string;
  cases: string;
  client: string;
  clients: string;
  specialty: string | null;
}

export interface IureSessionStatus {
  loggedIn: boolean;
  name: string | null;
  email: string | null;
  crm: boolean;
  terminology: IureTerminology | null;
  error: string | null;
}

export interface IureCase {
  id: string;
  caseNumber: string;
  title: string;
  status: string;
  clientName: string | null;
  clientId: string | null;
}

export interface IureDocRef {
  id: string;
  fileName: string;
  isTranscript: boolean;
}

export interface IureCaseUploadResult {
  documents: IureDocRef[];
  timeEntryId: string | null;
  webUrl: string;
}

export interface IureBlueprint {
  id: string;
  name: string;
  description: string;
  genre: string;
  sectionCount: number;
  hasDesign: boolean;
  suggested: boolean;
}

export interface IureAiOptions {
  canGenerate: boolean;
  reason: string | null;
  isTranscript: boolean;
  composing: boolean;
  blueprints: IureBlueprint[];
}

export interface IureComposeStatus {
  state: string;
  current: number;
  total: number;
  section: string | null;
  documentId: string | null;
  link: string | null;
  error: string | null;
}

export type IureCrmKind = "lead" | "opportunity";

export interface IureCrmItem {
  kind: IureCrmKind;
  id: string;
  name: string;
  organization: string | null;
  status: string | null;
  stage: string | null;
  amount: number | null;
}

export interface IureCrmUploadResult {
  documents: IureDocRef[];
  activityId: string | null;
  webUrl: string;
}

export interface IureUploadProgress {
  jobId: string;
  fileName: string;
  index: number;
  totalFiles: number;
  sent: number;
  total: number;
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
  bundled: boolean;
  path: string;
}

export type AppId = "transcribe" | "editor" | "dav";
export interface AppStatus {
  id: AppId;
  name: string;
  description: string;
  installed: boolean;
  path: string | null;
  downloadUrl: string;
  latestVersion: string | null;
}
/** Unidad de IureDav configurada en este equipo. */
export interface DavMount {
  id: string;
  name: string;
  url: string;
  user: string;
  mountPoint: string;
  mounted: boolean;
  writable: boolean;
}

export interface SystemInfo {
  backend: string;
  cpuThreads: number;
  platform: string;
  modelsDir: string;
  settingsPath: string;
  ffmpegAvailable: boolean;
  recordingsDir: string;
  version: string;
  updateTarget: string;
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
  /** Quién habla, cuando la grabación distinguió micrófono y sistema. */
  speaker?: string | null;
}

export interface IureCommitment {
  title: string;
  description: string | null;
  assigneeName: string | null;
  assignedToId: string | null;
  dueDate: string | null;
  dueHint: string | null;
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

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface DeviceList {
  inputs: AudioDevice[];
  systemCapture: "native" | "virtual" | "unavailable";
  note: string;
  backend: string;
}

export interface RecordingStatus {
  active: boolean;
  elapsedSecs: number;
  micLevel: number;
  sysLevel: number;
  path: string | null;
  error: string | null;
  livePendingSecs: number;
  live: boolean;
}

export interface StartRecordingInfo {
  path: string;
  live: boolean;
  liveNote: string | null;
}

export interface RecordingResult {
  path: string;
  durationSecs: number;
}

export interface UpdateNotice {
  version: string;
  url: string;
}

/** Aviso de la sonda de GPU (variantes CUDA/Vulkan): "cpu" = sigue con procesador; "none" = esta variante no sirve aquí. */
export interface GpuNotice {
  level: "cpu" | "none";
  message: string;
  url: string;
}

export const api = {
  checkUpdateNotice: () => invoke<UpdateNotice | null>("check_update_notice"),
  iureTestConnection: () => invoke<IureConnectionInfo>("iure_test_connection"),
  iureList: (folder: string) => invoke<IureListing>("iure_list", { folder }),
  iureUpload: (jobId: string, folder: string, files: string[]) =>
    invoke<IureUploadResult>("iure_upload", { request: { jobId, folder, files } }),
  iureLogin: (password: string, totpCode?: string, totpToken?: string) =>
    invoke<IureLoginResult>("iure_login", { password, totpCode: totpCode ?? null, totpToken: totpToken ?? null }),
  iureSessionStatus: () => invoke<IureSessionStatus>("iure_session_status"),
  iureLogout: () => invoke<void>("iure_logout"),
  /** true si creó una contraseña de aplicación nueva con la sesión; false si ya había. */
  iureEnsureWebdavPassword: () => invoke<boolean>("iure_ensure_webdav_password"),
  iureSearchCases: (query: string) => invoke<IureCase[]>("iure_search_cases", { query }),
  iureUploadToCase: (req: { jobId: string; caseId: string | null; files: string[]; transcriptPath: string | null; hours: number | null; hoursDescription: string | null }) =>
    invoke<IureCaseUploadResult>("iure_upload_to_case", { request: req }),
  iureAiOptions: (documentId: string) => invoke<IureAiOptions>("iure_ai_options", { documentId }),
  iureCompose: (req: { blueprintId: string; caseId: string | null; sourceDocumentIds: string[]; title: string | null; attendees: string[]; extraInstructions: string | null }) =>
    invoke<string>("iure_compose", { request: req }),
  iureComposeStatus: (taskId: string) => invoke<IureComposeStatus>("iure_compose_status", { taskId }),
  /** `baseName` sin extensión: el backend añade la del documento real (.md, .docx…). */
  iureDownloadDocument: (documentId: string, targetDir: string, baseName: string) =>
    invoke<string>("iure_download_document", { request: { documentId, targetDir, baseName } }),
  iureSummaryViaChat: (documentId: string, outputDir: string, baseName: string) =>
    invoke<DocumentResult>("iure_summary_via_chat", { request: { documentId, outputDir, baseName } }),
  iureCommitments: (documentId: string) => invoke<{ commitments: IureCommitment[]; caseId: string | null }>("iure_commitments", { documentId }),
  iureApplyCommitments: (documentId: string, caseId: string | null, commitments: IureCommitment[]) =>
    invoke<number>("iure_apply_commitments", { request: { documentId, caseId, commitments } }),
  iureCrmSearch: (kind: IureCrmKind, query: string) => invoke<IureCrmItem[]>("iure_crm_search", { kind, query }),
  iureUploadToCrm: (req: { jobId: string; kind: IureCrmKind; id: string; files: string[]; activitySubject: string | null; activityDescription: string | null; durationMinutes: number | null }) =>
    invoke<IureCrmUploadResult>("iure_upload_to_crm", { request: req }),
  listAudioDevices: () => invoke<DeviceList>("list_audio_devices"),
  startRecording: (speakers: [string, string] | null) => invoke<StartRecordingInfo>("start_recording", { speakers }),
  stopRecording: () => invoke<RecordingResult>("stop_recording"),
  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
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
  saveLiveTranscript: (jobId: string, path: string, segments: Segment[], audioSecs: number, elapsedSecs: number) =>
    invoke<TranscriptResult>("save_live_transcript", { request: { jobId, path, segments, audioSecs, elapsedSecs } }),
  cancelJob: (jobId: string) => invoke<boolean>("cancel_job", { jobId }),
  generateDocument: (kind: DocKind, text: string, outputDir: string, baseName: string, context: string) =>
    invoke<DocumentResult>("generate_document", { request: { kind, text, outputDir, baseName, context } }),
  loadJobs: () => invoke<string>("load_jobs"),
  saveJobs: (json: string) => invoke<void>("save_jobs", { json }),
  loadDocuments: (outputDir: string, baseName: string) =>
    invoke<{ summary: DocumentResult | null; minutes: DocumentResult | null }>("load_documents", { outputDir, baseName }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  /** Apps de escritorio de Iurefficient: instaladas en este equipo y última versión publicada. */
  appsStatus: (withNetwork: boolean) => invoke<AppStatus[]>("apps_status", { withNetwork }),
  openWithApp: (app: AppId, path: string) => invoke<void>("open_with_app", { app, path }),
  launchApp: (app: AppId) => invoke<void>("launch_app", { app }),
  iuredavMounts: () => invoke<DavMount[]>("iuredav_mounts"),
  launchArgs: () => invoke<string[]>("launch_args"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
};
