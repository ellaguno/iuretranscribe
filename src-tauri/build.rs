fn main() {
    // ggml-metal usa `@available(...)`, que requiere el símbolo ___isPlatformVersionAtLeast
    // de compiler-rt; rustc no lo enlaza por sí solo, así que añadimos libclang_rt.osx.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        if let Ok(out) = std::process::Command::new("clang").arg("--print-resource-dir").output() {
            if out.status.success() {
                let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
                println!("cargo:rustc-link-search=native={dir}/lib/darwin");
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        }
    }
    tauri_build::build()
}
