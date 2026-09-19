Instaladores generados automáticamente por GitHub Actions. Elige el que corresponda a tu equipo:

| Archivo | Sistema | GPU |
| --- | --- | --- |
| `*_linux-x64.deb` / `.rpm` / `.AppImage` | Linux | ninguna (CPU) |
| `*_linux-x64-vulkan.*` | Linux | AMD, Intel o NVIDIA vía Vulkan (ligero) |
| `*_linux-x64-cuda.deb` | Linux | NVIDIA con CUDA 12 (incluye el runtime, pesa más) |
| `*_windows-x64.exe` / `.msi` | Windows | ninguna (CPU) |
| `*_windows-x64-vulkan.exe` | Windows | AMD, Intel o NVIDIA vía Vulkan (ligero) |
| `*_windows-x64-cuda.exe` | Windows | NVIDIA con CUDA 12 (incluye el runtime, pesa más) |
| `*_macos-arm64.dmg` | macOS | Apple Silicon con Metal |
| `*_macos-x64.dmg` | macOS | Mac Intel (CPU) |

Las variantes CUDA requieren driver NVIDIA ≥ 525. Las variantes Vulkan requieren drivers gráficos actualizados.
Los modelos de Whisper se descargan desde la propia app la primera vez.
