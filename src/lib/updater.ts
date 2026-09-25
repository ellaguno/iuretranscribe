import { ask, message } from "@tauri-apps/plugin-dialog";
import { api, type UpdateNotice } from "./api";
import { app, toast } from "./state.svelte";
import { t } from "./i18n.svelte";

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
      const install = await ask(t("update.available", { version: update.version }), {
        title: t("update.availableTitle"),
        kind: "info",
        okLabel: t("update.update"),
        cancelLabel: t("update.notNow"),
      });
      if (!install) return;
      toast(t("update.downloading"), "info", 6000);
      await update.downloadAndInstall();
      const restart = await ask(t("update.installed"), { title: "IureTranscribe", kind: "info", okLabel: t("update.restart"), cancelLabel: t("update.later") });
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
    if (!silent) await message(t("update.checkFailed", { error: String(err) }), { title: "IureTranscribe", kind: "warning" });
    return;
  }
  app.updateNotice = notice;
  if (!notice && !silent) await message(t("update.upToDate"), { title: "IureTranscribe", kind: "info" });
}
