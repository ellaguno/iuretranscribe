import { ask, message } from "@tauri-apps/plugin-dialog";
import { api, type UpdateNotice } from "./api";
import { app, toast } from "./state.svelte";

/**
 * Actualizaciones, en dos capas:
 * 1. Actualizador de Tauri (releases firmadas, `latest.json`): descarga e instala
 *    dentro de la app. En Linux sólo aplica al AppImage; Windows y macOS siempre.
 * 2. Aviso: si el actualizador no puede (instalación .deb, sin manifiesto, sin
 *    red), consulta la última release en GitHub y muestra un aviso con enlace.
 *
 * `silent` = chequeo de arranque (no molesta si no hay nada); desde Ajustes es explícito.
 */
export async function checkForUpdates(silent: boolean): Promise<void> {
  const target = app.sys?.updateTarget;
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check(target ? { target } : undefined);
    if (update) {
      app.updateNotice = { version: update.version, url: "https://github.com/ellaguno/iuretranscribe/releases/latest" };
      const install = await ask(`Hay una nueva versión de IureTranscribe (${update.version}).\n¿Descargar e instalar ahora?`, {
        title: "IureTranscribe — Actualización disponible",
        kind: "info",
        okLabel: "Actualizar",
        cancelLabel: "Ahora no",
      });
      if (!install) return;
      toast("Descargando la actualización…", "info", 6000);
      await update.downloadAndInstall();
      const restart = await ask("Actualización instalada. ¿Reiniciar IureTranscribe ahora?", { title: "IureTranscribe", kind: "info", okLabel: "Reiniciar", cancelLabel: "Después" });
      if (restart) {
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await relaunch();
      }
      return;
    }
    // Sin actualización según el manifiesto: confirma con el aviso por si el
    // manifiesto no cubre esta variante o instalación.
  } catch (err) {
    console.warn("Actualizador de Tauri no disponible:", err);
  }
  let notice: UpdateNotice | null = null;
  try {
    notice = await api.checkUpdateNotice();
  } catch (err) {
    if (!silent) await message(`No se pudo buscar actualizaciones.\n${err}`, { title: "IureTranscribe", kind: "warning" });
    return;
  }
  app.updateNotice = notice;
  if (!notice && !silent) await message("Ya tienes la última versión.", { title: "IureTranscribe", kind: "info" });
}
