# Ajustes v0.2 — resultados

Plan: [`2026-11-24-ajustes-v0.2.md`](2026-11-24-ajustes-v0.2.md). Rama `ajustes/v0.2` (versión **0.2.0**).

## Qué se hizo

| # | Ajuste | Estado |
|---|---|---|
| 1 | Imágenes como Warp: Ctrl+V con imagen → Claude Code con su tecla (`[Image #N]`), otros agentes y shell con la ruta del PNG, SSH subiendo por SFTP; arrastrar imágenes; barra del agente con miniaturas | ✅ lógica con pruebas; captura ✅ (sin agente: se pega la ruta) |
| 2 | Título del agente en la pestaña (OSC 0/2, sin spinners), nombre según el shell, nombre manual que gana, marquesina en la pestaña activa (se detiene con el mouse, sin temporizador en reposo) | ✅ con pruebas |
| 3 | Notificaciones por tipo, tiempo mínimo por sesión (5 min), solo en segundo plano | ✅ con pruebas |
| 4 | Configuración (⚙, Ctrl+, , paleta): temas en círculos con su nombre, letra, panel, marquesina, idioma, notificaciones, actualizaciones | ✅ captura |
| 5–6 | Árbol tipo VS Code y menú contextual es/en (sin "Diff This") | ✅ captura; "Buscar en la carpeta" y "Línea de tiempo" en gris (próximamente) |
| 7 | Aviso de actualización: GitHub Releases, SHA-256 y firma verificados, `msiexec /passive`, reabre Kuntur | ✅ con pruebas; de punta a punta ⏳ con la primera 0.2.x publicada |
| 8 | Firma: `msi.ps1 -Sign`, SignPath opcional en la CI, `docs/FIRMA.md` | ✅ preparado; certificado ⏳ (usuario) |
| 9 | Acceso directo en el escritorio (MSI) | ✅ tabla `Shortcut` revisada con `msi.ps1 -Check` |
| 10 | Linux/Mac: carpetas por sistema, `$SHELL`, trabajos de CI | ✅ base; la app completa ⏳ en la CI |
| 11 | Publicación: historial y binarios revisados, repositorio público, nombre «Kuntur Terminal IA Agente», capturas nuevas | ✅ |

Además: los binarios de release ya no llevan rutas de la PC (`--remap-path-prefix`; `msi.ps1` se niega a armar un
MSI que las tenga) y los tooltips de la barra de título están en español e inglés.

## Verificación
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` en verde,
  salvo `terminal_view::ghost::tests::tab_acepta_la_ruta_sugerida_en_powershell`, que ya fallaba antes en esta PC
  (PSReadLine muestra sus propias predicciones en la línea; no es de esta entrega).
- Capturas (`scripts/smoke.ps1 … -Display 0 -AllowPrimary`, con permiso del usuario porque esta PC tiene un solo
  monitor), siempre en carpetas neutras (`C:\Windows`, `C:\Users\Public\Documents\Kuntur`) y con datos sintéticos:
  `docs/images/kuntur-dark.png`, `kuntur-settings.png`, `kuntur-files.png`, `kuntur-usage.png`, `kuntur-preview.png`.
  RAM medida en las pruebas: 66–83 MB.
- Para las capturas se agregaron `-Settings`, `-FileMenu` y `-PasteImage` a `smoke.ps1`. Ojo: `-AllowPrimary` solo
  funciona junto con `-Display <n>`.

## Pendiente
- Pruebas manuales con agentes reales: Win+Shift+S → Ctrl+V dentro de `claude` (Windows lee `CF_DIB`), Codex,
  OpenCode y una pestaña SSH; título de Claude Code en la pestaña.
- Actualizar de 0.2.0 a una 0.2.x publicada (prueba de punta a punta del actualizador).
- Certificado de firma (SignPath Foundation o propio) — ver `docs/FIRMA.md`.
- Árbol: portapapeles del sistema (copiar en Kuntur y pegar en el Explorador), selección múltiple, arrastrar dentro
  del árbol; "Buscar en la carpeta" y "Abrir línea de tiempo".
- Traducir al inglés el resto de la interfaz (paleta, Uso, Auditoría, Conexiones, MCP).
- Ver los trabajos `linux` y `macos` de la CI en GitHub.
