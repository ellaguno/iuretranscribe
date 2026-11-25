//! Instalación desde Microsoft Store (paquete MSIX).
//!
//! La Store no permite que la app se actualice sola ni que remita a otros instaladores:
//! instalada desde ahí, no se usa el actualizador de Tauri, no se avisa de versiones
//! nuevas en GitHub y la sonda de GPU no recomienda instalar otra variante. Descargar
//! modelos de Whisper sí está permitido y sigue igual (van a AppData, que el paquete
//! virtualiza en su propia carpeta).

use std::path::PathBuf;
use std::sync::OnceLock;

/// Nombre del alias de ejecución que declara el manifiesto del paquete
/// (`windows.appExecutionAlias` en `src-tauri/msix/AppxManifest.xml`).
#[cfg(windows)]
const ALIAS: &str = "iuretranscribe.exe";

/// Si este proceso corre como paquete MSIX (instalado desde la Store o, en las pruebas,
/// con `Add-AppxPackage`). Fuera de Windows siempre es `false`.
pub fn from_store() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(detect)
}

#[cfg(windows)]
fn detect() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
    }
    const APPMODEL_ERROR_NO_PACKAGE: i32 = 15700;
    let mut len = 0u32;
    // Con paquete devuelve ERROR_INSUFFICIENT_BUFFER (el búfer vacío no alcanza).
    let r = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    if r != APPMODEL_ERROR_NO_PACKAGE {
        return true;
    }
    // Lanzado por su ruta dentro de WindowsApps (sin identidad de paquete): también
    // es la copia de la Store y no debe actualizarse sola.
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_ascii_lowercase().contains("\\windowsapps\\"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn detect() -> bool {
    false
}

/// Ruta estable para lanzar la app desde fuera (clientes MCP): el alias de ejecución
/// `%LOCALAPPDATA%\Microsoft\WindowsApps\iuretranscribe.exe`. La ruta real del
/// ejecutable cambia con cada versión y, lanzada directamente, corre sin identidad de
/// paquete (y sin ver los modelos descargados, que están en el AppData del paquete).
pub fn execution_alias() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if from_store() {
            return dirs::data_local_dir().map(|d| d.join("Microsoft").join("WindowsApps").join(ALIAS));
        }
    }
    None
}

/// Prepara el proceso para el paquete MSIX. `iurefficient_connect::agents::command()`
/// registra en Claude Desktop y VS Code el ejecutable de `APPIMAGE` si existe (en Linux,
/// la ruta estable del AppImage); en la Store se usa ese mismo mecanismo con el alias
/// de ejecución. Fuera del paquete no hace nada.
pub fn init() {
    if let Some(alias) = execution_alias() {
        std::env::set_var("APPIMAGE", alias);
    }
}
