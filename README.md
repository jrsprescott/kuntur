# Kuntur

> **Estado: en desarrollo (repositorio privado).** Aún no hay una versión utilizable.

**Kuntur** (cóndor, en quechua) es una terminal para Windows escrita en Rust, liviana y pensada para
trabajar con agentes de IA en la línea de comandos (Claude Code, Codex, OpenCode, Qwen…): ves de un
vistazo, como el cóndor desde lo alto, todas tus sesiones y lo que hace cada agente.

- Meta de consumo: **< 120 MB de RAM** (como referencia, Warp usa ~900 MB en el mismo equipo).
- Sesiones en un riel lateral (completo o solo íconos), terminal acoplada sin IA en la misma ruta (local o SSH).
- Panel **Agentes**: árbol compacto de agentes y subagentes con su acción actual, estado y worktree.
- Panel **Archivos**: Windows ↔ Linux (SFTP), arrastrar, copiar/pegar, subir y descargar.
- Uso y costos en una línea (tokens/s, contexto, límites de 5 h y semanal).
- MCP y comandos personalizados por sesión; 3 paletas (Blue, Dark, White); diseño minimalista.

## Estructura
```
kuntur/
├── Cargo.toml                 workspace
├── crates/
│   ├── kuntur-app/            binario `kuntur`: ventana y UI (GPUI + gpui-kit)
│   ├── kuntur-term/           PTY (ConPTY / SSH) + parser VT (alacritty_terminal)
│   ├── kuntur-agents/         agentes, hooks, worktrees
│   ├── kuntur-usage/          uso, costos, límites 5 h / semanal
│   ├── kuntur-files/          panel Archivos: local, SFTP, WSL, transferencias
│   ├── kuntur-complete/       autocompletado y memoria de carpetas
│   └── kuntur-config/         configuración, temas y atajos
├── themes/                    blue.toml · dark.toml · white.toml
├── spikes/hola-gpui/          prueba técnica de GPUI (70 MB medidos)
└── docs/
    ├── PLAN.md                plan completo por fases
    ├── INVESTIGACION.md       referencias y fuentes
    └── simulacion/            simulación interactiva de la interfaz (index.html)
```

## Ver la simulación
Abrir `docs/simulacion/index.html` en el navegador (los datos son de ejemplo).

## Compilar
Requisitos: Rust estable (1.90+) y Visual Studio Build Tools con C++ y Windows SDK.
```powershell
cargo check --workspace
cd spikes\hola-gpui; cargo run --release   # ventana de prueba GPUI
```
