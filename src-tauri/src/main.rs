// Evita la ventana de consola en Windows en modo release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match std::env::args().nth(1).as_deref() {
        // Servidor MCP por stdio para Claude Desktop, Claude Code, VS Code… Sin ventana.
        Some("--mcp") => std::process::exit(iuretranscribe_lib::run_mcp()),
        // Lo que hay que pegar en la configuración del cliente MCP.
        Some("--mcp-config") => println!("{:#}", iuretranscribe_lib::mcp_client_config()),
        _ => iuretranscribe_lib::run(),
    }
}
