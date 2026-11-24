//! Cola de transferencias al pie del panel "Archivos" (F7): una fila por trabajo con barra de
//! progreso, × para cancelar y, si el destino existe, "Sobrescribir / Renombrar / Omitir". La
//! lógica (estados, conflictos, nombres) está en `kuntur_files::transfer`, con pruebas.

use kuntur_files::{Conflict, Direction, Job, JobState, format_size};

use super::files_panel::small_button;
use super::*;

/// Filas visibles como mucho (las más recientes).
const MAX_ROWS: usize = 5;

impl Workspace {
    pub(super) fn render_transfers(&self, cx: &mut Context<Self>) -> AnyElement {
        let jobs = self.files.transfers.snapshot();
        if jobs.is_empty() {
            return div().into_any_element();
        }
        let c = &self.colors;
        let s = self.files.transfers.summary();
        let title = if s.active > 0 {
            format!(
                "{} en curso · {} / {}",
                s.active,
                format_size(s.done_bytes),
                format_size(s.total_bytes)
            )
        } else if s.asking > 0 {
            "Esperan respuesta".to_string()
        } else {
            "Transferencias terminadas".to_string()
        };
        // Varias preguntas a la vez: la respuesta vale para todas.
        let for_all = s.asking > 1;
        let hover = c.hover;
        let header = h_flex()
            .h(px(24.))
            .px(px(6.))
            .gap(px(6.))
            .items_center()
            .text_size(px(11.))
            .text_color(c.muted)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(title),
            )
            .when(s.finished > 0, |this| {
                this.child(
                    div()
                        .id("transfers-clear")
                        .flex_shrink_0()
                        .px(px(6.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(move |st| st.bg(hover))
                        .child("Limpiar")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.files.transfers.clear_finished();
                            cx.notify();
                        })),
                )
            });

        let start = jobs.len().saturating_sub(MAX_ROWS);
        let rows: Vec<AnyElement> = jobs[start..]
            .iter()
            .map(|j| self.render_transfer_row(j, for_all, cx))
            .collect();

        v_flex()
            .flex_shrink_0()
            .mx(px(8.))
            .mb(px(6.))
            .p(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(c.border)
            .child(header)
            .children(rows)
            .into_any_element()
    }

    fn render_transfer_row(&self, job: &Job, for_all: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let id = job.id;
        let (status, color): (String, Hsla) = match &job.state {
            JobState::Queued => ("en espera".into(), c.muted),
            JobState::Running => (format!("{} %", (job.fraction() * 100.0).round() as u32), c.text),
            JobState::Asking => ("ya existe".into(), c.warn),
            JobState::Done => ("listo".into(), c.ok),
            JobState::Skipped => ("omitido".into(), c.muted),
            JobState::Cancelled => ("cancelado".into(), c.muted),
            JobState::Failed(_) => ("error".into(), c.error),
        };
        let icon = match job.direction {
            Direction::Upload => IconName::ArrowUp,
            Direction::Download => IconName::ArrowDown,
            Direction::Copy => IconName::Copy,
        };
        let cancellable = matches!(job.state, JobState::Queued | JobState::Running | JobState::Asking);
        let tip = format!("{} {} → {}", job.direction.verb(), job.name, job.dest_dir);
        let hover = c.hover;

        let line = h_flex()
            .id(("transfer", id as usize))
            .h(px(22.))
            .px(px(4.))
            .gap(px(6.))
            .items_center()
            .text_size(px(11.5))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(Icon::new(icon).size(px(12.)).flex_shrink_0().text_color(c.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(c.text)
                    .child(job.name.clone()),
            )
            .child(div().flex_shrink_0().text_size(px(11.)).text_color(color).child(status))
            .when(cancellable, |this| {
                this.child(
                    div()
                        .id(("transfer-cancel", id as usize))
                        .flex_shrink_0()
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .text_color(c.muted)
                        .hover(move |s| s.bg(hover))
                        .tooltip(|window, cx| Tooltip::new("Cancelar").build(window, cx))
                        .child(Icon::new(IconName::Close).size(px(11.)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.files.transfers.cancel(id);
                            cx.notify();
                        })),
                )
            });

        let bar = (job.state == JobState::Running).then(|| {
            div()
                .mx(px(4.))
                .mb(px(3.))
                .h(px(3.))
                .rounded(px(2.))
                .bg(c.border)
                .child(
                    div()
                        .h_full()
                        .rounded(px(2.))
                        .bg(c.accent)
                        .w(relative(job.fraction().max(0.02))),
                )
        });

        let error = match &job.state {
            JobState::Failed(e) => Some(
                div()
                    .px(px(22.))
                    .pb(px(3.))
                    .text_size(px(10.5))
                    .text_color(c.error)
                    .child(e.clone()),
            ),
            _ => None,
        };

        let ask = (job.state == JobState::Asking).then(|| {
            let suffix = if for_all { " (todos)" } else { "" };
            let choice = |key: &'static str, label: &'static str, conflict: Conflict, color: Hsla| {
                small_button((key, id as usize), label, color, c).on_click(cx.listener(move |this, _, _, cx| {
                    this.files.transfers.resolve(id, conflict, for_all);
                    this.start_transfer_poll(cx);
                    cx.notify();
                }))
            };
            v_flex()
                .px(px(22.))
                .pb(px(4.))
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(c.muted)
                        .child(format!("Ya existe en el destino{suffix}:")),
                )
                .child(
                    h_flex()
                        .gap(px(4.))
                        .flex_wrap()
                        .child(choice("ask-over", "Sobrescribir", Conflict::Overwrite, c.text))
                        .child(choice("ask-rename", "Renombrar", Conflict::Rename, c.accent))
                        .child(choice("ask-skip", "Omitir", Conflict::Skip, c.muted)),
                )
        });

        v_flex()
            .child(line)
            .children(bar)
            .children(error)
            .children(ask)
            .into_any_element()
    }
}
