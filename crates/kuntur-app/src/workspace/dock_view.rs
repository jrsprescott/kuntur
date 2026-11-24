//! Terminal acoplada (F4): segundo panel debajo de la terminal principal de cada sesión, con su
//! propio PTY y el mismo shell. Ctrl+` la muestra / enfoca / oculta (como VS Code); ocultarla no
//! la destruye (el historial sigue), solo se suelta si su shell termina o se cierra la sesión.
//! La lógica (foco, límites del separador, carpeta inicial) vive en `crate::dock`, con pruebas.

use crate::dock::{Slot, Toggle, ratio_for_pointer, start_dir};

use super::*;

/// Separador arrastrable entre la principal y la acoplada.
const SASH: f32 = 5.0;
const DOCK_HEADER: f32 = 24.0;

/// Valor que viaja mientras se arrastra el separador (no se dibuja nada).
pub(super) struct DockDrag;

impl Render for DockDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Workspace {
    /// Opciones del PTY para un shell (`None` = el predeterminado) en una carpeta.
    pub(super) fn pane_options(shell: Option<String>, cwd: Option<PathBuf>) -> PaneOptions {
        let size = GridSize::new(100, 30);
        let mut opts = match shell {
            Some(program) => PaneOptions {
                program,
                args: Vec::new(),
                cwd: None,
                size,
                env: Vec::new(),
            },
            None => PaneOptions::default_shell(size),
        };
        opts.cwd = cwd;
        opts
    }

    /// Vista de terminal con el tema y la fuente actuales.
    pub(super) fn new_terminal(
        &mut self,
        mut opts: PaneOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalView> {
        // F5b: `KUNTUR_PIPE` y `KUNTUR_SESSION` para los ganchos de los agentes que corran ahí.
        let token = self.terminal_env(&mut opts);
        let palette = palette_from(&self.theme);
        let (family, size_pt, background) = (self.mono_font.clone(), self.settings.font_size, self.colors.terminal);
        let view = cx.new(|cx| {
            let mut v = TerminalView::new(window, cx, opts, palette.clone(), family, size_pt);
            v.set_palette(palette, background, cx);
            v
        });
        self.register_terminal(&view, token);
        view
    }

    /// Panel que tiene el teclado ahora mismo en la sesión.
    fn focused_slot(&self, id: u64, window: &Window, cx: &App) -> Slot {
        let dock_focused = self
            .sessions
            .get(id)
            .and_then(|e| e.value.dock.as_ref())
            .is_some_and(|d| d.read(cx).is_focused(window));
        if dock_focused { Slot::Dock } else { Slot::Main }
    }

    /// Ctrl+`, el botón de la barra o la paleta, sobre la sesión activa.
    pub(super) fn toggle_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.sessions.active_id() else {
            return;
        };
        self.usage.open = false;
        self.audit.open = false;
        let focused = self.focused_slot(id, window, cx);
        let outcome = match self.sessions.get_mut(id) {
            Some(e) => e.value.dock_state.toggle(focused),
            None => return,
        };
        if outcome == Toggle::Show {
            self.ensure_dock(id, window, cx);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Oculta la acoplada de la sesión (sigue viva) y el teclado vuelve a la principal.
    pub(super) fn hide_dock(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.sessions.get_mut(id) {
            e.value.dock_state.hide();
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Crea la acoplada si aún no existe: mismo shell que la sesión, en su carpeta actual (o en la
    /// personal si todavía no se conoce).
    pub(super) fn ensure_dock(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.sessions.get(id) else {
            return;
        };
        if entry.value.dock.is_some() {
            return;
        }
        let view = match &entry.value.remote {
            // Remota (F6): otro canal en la misma conexión, en la carpeta de la principal; sin volver
            // a autenticar.
            Some(r) => {
                let (session, cwd) = (r.session.clone(), r.main.cwd());
                self.new_remote_terminal(&session, cwd, window, cx).0
            }
            None => {
                // En el worktree del agente si la sesión se abrió en uno (F5b); si no, en la carpeta
                // actual.
                let here = self.session_worktree(id).or_else(|| entry.value.view.read(cx).cwd());
                let cwd = start_dir(here, self.home.as_deref());
                let opts = Self::pane_options(entry.value.shell.clone(), cwd);
                self.new_terminal(opts, window, cx)
            }
        };
        let events = cx.subscribe_in(&view, window, |this, view, ev: &TerminalEvent, window, cx| match ev {
            TerminalEvent::TitleChanged => cx.notify(),
            TerminalEvent::Exited => this.dock_exited(view, window, cx),
        });
        if let Some(e) = self.sessions.get_mut(id) {
            e.value.dock = Some(view);
            e.value._dock_events = Some(events);
        }
    }

    /// El shell de la acoplada terminó (`exit`): se suelta y la principal recupera el teclado.
    fn dock_exited(&mut self, view: &Entity<TerminalView>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .sessions
            .iter()
            .find(|e| e.value.dock.as_ref() == Some(view))
            .map(|e| e.id)
        else {
            return;
        };
        if let Some(e) = self.sessions.get_mut(id) {
            e.value.dock = None;
            e.value._dock_events = None;
            e.value.dock_state.dock_exited();
        }
        if self.sessions.active_id() == Some(id) {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    /// Clic en un panel: queda como el panel con foco de la sesión (para Ctrl+` y al volver a la
    /// pestaña). La vista misma ya tomó el teclado.
    fn remember_focus(&mut self, id: u64, slot: Slot) {
        if let Some(e) = self.sessions.get_mut(id) {
            e.value.dock_state.set_focus(slot);
        }
    }

    /// Terminal de la sesión que debe recibir el teclado (la principal o la acoplada).
    pub(super) fn focus_target(&self, id: u64) -> Option<Entity<TerminalView>> {
        let e = self.sessions.get(id)?;
        match (e.value.dock_state.focus(), &e.value.dock) {
            (Slot::Dock, Some(dock)) => Some(dock.clone()),
            _ => Some(e.value.view.clone()),
        }
    }

    /// ¿El teclado de la sesión va a su acoplada (visible y viva)?
    pub(super) fn dock_has_focus(&self, id: u64) -> bool {
        self.sessions
            .get(id)
            .is_some_and(|e| e.value.dock_state.focus() == Slot::Dock && e.value.dock.is_some())
    }

    /// Carpeta del panel con foco de la sesión activa (para la línea inferior).
    pub(super) fn focused_cwd(&self, cx: &App) -> Option<PathBuf> {
        let id = self.sessions.active_id()?;
        self.focus_target(id).and_then(|v| v.read(cx).cwd())
    }

    /// ¿La sesión activa tiene la acoplada visible?
    pub(super) fn dock_open(&self) -> bool {
        self.sessions.active().is_some_and(|e| e.value.dock_state.open)
    }

    /// Prueba de humo: abre la acoplada de la primera sesión tras `delay` (después de los comandos
    /// de la principal, para que herede su carpeta), le escribe `input` y, con `reopen`, la oculta
    /// y la vuelve a mostrar para comprobar que el historial sigue.
    pub(super) fn smoke_dock(
        &mut self,
        delay: Duration,
        input: Option<String>,
        reopen: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let typed = input.as_ref().map_or(0, |i| i.split("\\n").count()) as u64;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |w, window, cx| {
                w.toggle_dock(window, cx);
                if let (Some(input), Some(dock)) = (input, w.sessions.active().and_then(|e| e.value.dock.clone())) {
                    dock.read(cx).send_smoke_input(input);
                }
            });
            if !reopen {
                return;
            }
            // send_smoke_input espera 2,5 s y 0,6 s por comando.
            let wait = 2_500 + 600 * typed + 1_000;
            cx.background_executor().timer(Duration::from_millis(wait)).await;
            let _ = this.update_in(cx, |w, window, cx| w.toggle_dock(window, cx));
            cx.background_executor().timer(Duration::from_millis(800)).await;
            let _ = this.update_in(cx, |w, window, cx| w.toggle_dock(window, cx));
        })
        .detach();
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    /// Área de terminales de la sesión activa: la principal y, si está visible, el separador, el
    /// encabezado y la acoplada.
    pub(super) fn render_terminals(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.sessions.active() else {
            return div().into_any_element();
        };
        let id = entry.id;
        let main = div()
            .id(("main-pane", id))
            .flex_1()
            .min_h_0()
            .on_any_mouse_down(cx.listener(move |this, _, _, _| this.remember_focus(id, Slot::Main)))
            .child(entry.value.view.clone());
        let dock = entry
            .value
            .dock
            .clone()
            .filter(|_| entry.value.dock_state.open)
            .map(|dock| self.render_dock(id, dock, entry.value.dock_state.ratio(), cx));

        v_flex()
            .id("terminals")
            .size_full()
            // Arrastre del separador: los límites del área dan la proporción.
            .on_drag_move(cx.listener(move |this, ev: &DragMoveEvent<DockDrag>, _, cx| {
                let b = ev.bounds;
                let ratio = ratio_for_pointer(
                    f32::from(b.origin.y),
                    f32::from(b.size.height),
                    f32::from(ev.event.position.y),
                );
                if let Some(e) = this.sessions.get_mut(id) {
                    e.value.dock_state.set_ratio(ratio);
                }
                cx.notify();
            }))
            .children(self.render_remote_banner(id, cx))
            .child(main)
            .children(dock)
            .into_any_element()
    }

    fn render_dock(&self, id: u64, dock: Entity<TerminalView>, ratio: f32, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let (border, accent) = (c.border, c.accent);
        let remote = self.remote_label(id);
        let shell = remote.clone().unwrap_or_else(|| {
            self.sessions
                .get(id)
                .and_then(|e| e.value.shell.clone())
                .map(|s| short_title(&s))
                .unwrap_or_else(|| "PowerShell".into())
        });
        let cwd = dock.read(cx).cwd().map(|p| match remote {
            Some(_) => p.display().to_string(),
            None => kuntur_files::display_path(&p, self.home.as_deref()),
        });
        let hover = c.hover;

        // Separador: línea de 1 px en una franja de 5 px que se toma con el mouse.
        let sash = div()
            .id(("dock-sash", id))
            .h(px(SASH))
            .flex_shrink_0()
            .w_full()
            .flex()
            .items_center()
            .cursor_ns_resize()
            .bg(c.terminal)
            .hover(move |s| s.bg(accent.opacity(0.3)))
            .on_drag(DockDrag, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| DockDrag)
            })
            .child(div().w_full().h(px(1.)).bg(border));

        let header = h_flex()
            .h(px(DOCK_HEADER))
            .flex_shrink_0()
            .pl(px(12.))
            .pr(px(4.))
            .gap(px(6.))
            .items_center()
            .bg(c.terminal)
            .text_size(px(11.5))
            .text_color(c.muted)
            .child(Icon::new(IconName::SquareTerminal).size(px(12.)).flex_shrink_0())
            .child(div().flex_shrink_0().text_color(c.text).child("Terminal"))
            .child(div().flex_shrink_0().child(format!("· {shell}")))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when_some(cwd, |this, cwd| this.child(format!("· {cwd}"))),
            )
            .child(
                div()
                    .id(("dock-hide", id))
                    .size(px(20.))
                    .flex_shrink_0()
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .tooltip(|window, cx| Tooltip::new("Ocultar (Ctrl+`)").build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.hide_dock(id, window, cx);
                    }))
                    .child(Icon::new(IconName::Close).size(px(12.))),
            );

        v_flex()
            .id(("dock", id))
            .flex_shrink_0()
            .h(relative(ratio))
            .min_h(px(crate::dock::MIN_DOCK_PX))
            .child(sash)
            .child(header)
            .child(
                div()
                    .id(("dock-pane", id))
                    .flex_1()
                    .min_h_0()
                    .on_any_mouse_down(cx.listener(move |this, _, _, _| this.remember_focus(id, Slot::Dock)))
                    .child(dock),
            )
            .into_any_element()
    }
}
