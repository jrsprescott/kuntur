# Especificación del usuario: terminal IA ligera en Rust

> Recibida y aprobada el 2026-11-23. Se adopta **como ampliación de Kuntur**, no como proyecto nuevo:
> la arquitectura de la sección 2 ya existe (F1–F3) y se mantiene **GPUI** en lugar de Tauri/egui/Iced
> (justificación en la tabla de abajo). Cómo se reparte cada sección en fases: [`plans/README.md`](plans/README.md).

## Decisiones tomadas al adoptarla

| Tema | Decisión |
|---|---|
| Framework de interfaz (§2) | **GPUI** (vía `gpui-kit`), ya en uso. Tauri dibuja toda la UI en WebView2 (+100 MB y la terminal pasaría a xterm.js, sin `alacritty_terminal`); egui redibuja la ventana completa cada cuadro (más CPU en reposo); Iced obligaría a rehacer F1–F3. GPUI: RAM medida 64–80 MB, texto por GPU, expone `HasWindowHandle` (para `wry`), arrastrar/soltar (`ExternalPaths`), `img`/`svg` y Markdown (`gpui-component`). |
| Módulos (§2) | Se mantienen los crates `kuntur-*` (`term`, `agents`, `usage`, `files`, `config`, `complete`); se agregan `kuntur-ssh`, `kuntur-preview`, `kuntur-mcp` y `kuntur-audit` cuando llegue su fase. |
| Filtros de estadísticas (§10) | **Hoy / 7 días / 30 días / 1 año / Todo** (reemplaza 7/30/90/Todo). ✅ hecho en `kuntur-usage`. |
| "Cuentas que más facturan" (§10) | Los historiales locales **no guardan la cuenta de facturación**. Se muestra **costo estimado por modelo y agente** (tokens × tabla de precios editable) y el costo real cuando la fuente lo trae (OpenCode). |
| Agentes (§5) | DeepSeek no tiene CLI oficial de agente; Antigravity es un IDE; Kimi sí tiene CLI. Se detectan en el PATH y se pueden agregar otros por configuración; los lectores de estadísticas se escriben solo para formatos verificados. |
| Estado de MCP (§11) | Los servidores MCP los lanza el agente: Kuntur no ve su estado directamente. Se generan las configuraciones por CLI y el estado se obtiene con **"Probar"** (lanzar y hacer `initialize`); un proxy MCP queda como mejora posterior. |
| Auditoría (§13) | Desde los registros de cada agente (Claude Code y Codex guardan los comandos que ejecutan) y los hooks de F5. No es universal: cada agente necesita su lector. |
| "Esperando confirmación" (§3, §12) | Heurística hasta F5; exacto con hooks / OSC 7501. |
| Vista previa web/PDF (§9) | `wry` (WebView2) como ventana hija: **spike obligatorio** antes de comprometerlo. WebView2 suma ~80–150 MB en procesos propios: se crea al abrir la vista previa y se destruye al cerrarla; los menús de Kuntur no pueden dibujarse encima. |
| Office → PDF (§9) | Opcional: solo si LibreOffice está instalado (se detecta; si no, "Abrir con su programa"). |
| Panel izquierdo (§3) | **Pendiente de decidir:** agentes arriba + árbol abajo a la vez (especificación) o el botón "Archivos" que alterna (diseño v5 actual). |

---

## Texto original

### 1. Objetivo
Construir una terminal de escritorio para trabajar con agentes de IA por línea de
comandos (Claude Code, Codex, DeepSeek, OpenCode, Kimi, Antigravity, etc.), ligera y
rápida, como reemplazo de Warp. Interfaz limpia y mínima, sin dependencias en la nube.
Plataforma principal: Windows, con trabajo frecuente en servidores Linux en la nube por SSH.

### 2. Arquitectura (importante)
- Alacritty por sí solo NO maneja paneles, árboles ni visores. Se usa su núcleo, el crate
  `alacritty_terminal`, para emular la terminal, embebido en una interfaz construida con
  Tauri, egui o Iced (elegir uno y justificarlo).
- La interfaz gestiona paneles, árbol de archivos, botones, vista previa y estado.
- PTY local: `portable-pty` (ConPTY en Windows).
- SSH/SFTP: `russh` + `russh-sftp` (alternativa: `ssh2`).
- Visor web embebido: `wry` (WebView2 en Windows).
- Base de datos local: SQLite (`rusqlite` o `sqlx`).
- Código modular: `ui`, `sessions`, `pty`, `ssh_sftp`, `agents`, `mcp`, `preview`,
  `stats`, `audit`, `config`.

### 3. Distribución de la interfaz
- **Barra superior:** pestañas, una por sesión.
- **Esquina superior derecha:** SOLO botones de icono. Cada uno abre su panel (modal o
  lateral) al hacer clic y se cierra al volver a pulsarlo.
- **Panel izquierdo (arriba):** agentes CLI detectados y su estado (inactivo / en
  ejecución / esperando confirmación). Clic en un agente = abrirlo en la sesión activa.
- **Panel izquierdo (abajo):** árbol de archivos de la sesión activa, mostrando siempre
  la ruta de trabajo actual y si es local o remota.
- **Zona central:** terminal. Se divide en dos cuando se abre la vista previa.

### 4. Botones de la esquina superior derecha
1. **Tema:** blanco, negro y azul. Guardar la preferencia.
2. **Estadísticas:** panel de uso de agentes (sección 10).
3. **Conexiones:** gestor de conexiones SSH guardadas (sección 8).
4. **MCP:** gestor de servidores MCP por sesión (sección 11).
5. **Auditoría:** registro de comandos ejecutados por los agentes (sección 13).

### 5. Detección de agentes
- Buscar en el PATH ejecutables conocidos: `claude`, `codex`, `deepseek`, `opencode`,
  `kimi`, `antigravity`, y permitir añadir otros por configuración (nombre, ejecutable,
  argumentos, ruta de sus registros).
- Refrescar la lista al iniciar y bajo demanda.
- En sesiones remotas, detectar los agentes instalados en el servidor.

### 6. Sesiones locales y remotas
- **Sesión local:** shell de Windows; el árbol lee el sistema de archivos local.
- **Sesión remota:** la propia app abre la conexión SSH (desde el gestor de conexiones);
  el árbol lee el servidor por SFTP sobre la misma conexión.
- Soportar la secuencia **OSC 7** para actualizar la carpeta actual cuando cambie dentro
  de la sesión (documentar cómo configurarla en bash/zsh del servidor y en PowerShell).
- Cada pestaña guarda: tipo (local/remota), conexión, carpeta actual, agente y MCP activos.
- El árbol NUNCA debe mezclar archivos locales con remotos: siempre refleja la sesión activa.

### 7. Transferencia de archivos (sesiones remotas)
- Arrastrar archivos desde el Explorador de Windows y soltarlos sobre una carpeta del
  árbol = subir por SFTP a esa ruta remota (evento de arrastrar y soltar de la ventana).
- Clic derecho en un archivo remoto = descargar a una carpeta local elegida.
- Barra de progreso, cancelación y aviso si el archivo ya existe (sobrescribir / renombrar).
- Opción rápida: carpeta temporal por sesión para "subir archivos para que el agente los lea".

### 8. Gestor de conexiones
- Guardar host, puerto, usuario y ruta de llave SSH.
- Nunca guardar contraseñas en texto plano: usar el Administrador de credenciales de Windows.
- Clic en una conexión = nueva pestaña remota ya conectada.
- Verificar y recordar la huella del servidor (known_hosts).

### 9. Vista previa de archivos y páginas
- Detectar en la salida de la terminal rutas de archivos y URLs (ampliando el sistema de
  "hints" de Alacritty); aparecen subrayadas y clicables.
- Clic (o clic derecho > "Vista previa") divide la ventana: terminal a la izquierda,
  vista previa a la derecha.
- El panel tiene pestañas para varios archivos y botones: abrir con su programa,
  descargar, cerrar.
- Formatos:
  - Markdown: renderizado con formato.
  - Imágenes (PNG, JPG, SVG, GIF): visor integrado.
  - PDF y HTML / páginas web: visor web embebido (wry).
  - PPTX, DOCX, XLSX: convertir a PDF en segundo plano con LibreOffice (modo headless)
    y mostrar el PDF.
- Archivos remotos: descargar por SFTP a una carpeta temporal y previsualizar; limpiar
  temporales al cerrar la sesión.
- Recarga automática cuando el agente modifica el archivo (vigilar cambios en local;
  en remoto, consultar la fecha de modificación por SFTP).

### 10. Estadísticas de uso
- Métricas: agente más usado, número de agentes usados, consumo de tokens (entrada y
  salida), cuentas que más facturan, número de sesiones.
- Filtros: día, 7 días, 30 días, año, todo.
- Fuente: registros locales de cada CLI (por ejemplo, los archivos JSONL de sesión de
  Claude Code y Codex) más el registro propio de la app.
- Almacenamiento: SQLite local, con importación incremental (no releer todo cada vez).
- Gráficos simples y exportación a CSV.

### 11. Gestor de MCP por sesión
- Catálogo de servidores MCP configurados (ej. godot-mcp, navegador, base de datos).
- Activar o desactivar MCP por pestaña/sesión; cada sesión guarda su propia lista.
- Al lanzar un agente, generar su archivo de configuración MCP según el formato de cada
  CLI (Claude Code, Codex, OpenCode, etc.).
- Indicador de estado de cada MCP (conectado, error, detenido).
- Las capturas o archivos que generen los MCP (ej. screenshots de una escena de Godot)
  se abren en el panel de vista previa y se actualizan solas.

### 12. Alertas
- Notificar (insignia en la pestaña + notificación de Windows) cuando un agente termina
  o queda esperando confirmación en una pestaña que no está visible.

### 13. Registro de auditoría
- Guardar fecha, sesión, agente, host y comando ejecutado.
- Panel con búsqueda y filtros; exportar a CSV.

### 14. Extras
- **Visor de cambios (diff):** mostrar a colores qué líneas modificó el agente.
- **Paleta de comandos:** atajo de teclado (ej. Ctrl+Shift+P) para abrir sesiones,
  conexiones, agentes y archivos escribiendo pocas letras.

### 15. Requisitos no funcionales
- Bajo consumo de RAM y CPU en reposo; arranque rápido.
- Sin telemetría ni dependencias en la nube.
- Configuración en un archivo legible (TOML) en la carpeta del usuario.
- Manejo de errores claro (conexión caída, reconexión SSH).

### 16. Orden de desarrollo sugerido
1. Interfaz base + `alacritty_terminal` + pestañas + tema.
2. Árbol de archivos local.
3. Sesiones SSH + árbol remoto por SFTP + OSC 7.
4. Arrastrar y soltar / descarga.
5. Detección de agentes y panel izquierdo.
6. Gestor de conexiones.
7. Vista previa de archivos.
8. Gestor de MCP por sesión.
9. Estadísticas, alertas y auditoría.
10. Visor de cambios y paleta de comandos.

### 17. Instrucción para el agente
Antes de programar, presenta un plan de arquitectura (framework de interfaz elegido,
estructura de carpetas y crates) y espera aprobación. Luego avanza por fases según el
orden sugerido, dejando cada fase compilando y probada antes de pasar a la siguiente.
