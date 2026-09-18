export function fmtDuration(secs: number | null | undefined): string {
  if (secs == null || !isFinite(secs)) return "—";
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(r).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function fmtTimestamp(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const base = `${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
  return h > 0 ? `${h}:${base}` : base;
}

export function fmtBytes(bytes: number | null | undefined): string {
  if (bytes == null || !isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function fmtMb(mb: number): string {
  return mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${mb} MB`;
}

export function fmtSpeed(audioSecs: number, elapsedSecs: number): string {
  if (!audioSecs || !elapsedSecs) return "—";
  return `${(audioSecs / elapsedSecs).toFixed(1)}×`;
}

export function fmtPercent(done: number, total: number | null): string {
  if (!total) return fmtBytes(done);
  return `${Math.round((done / total) * 100)}%`;
}
