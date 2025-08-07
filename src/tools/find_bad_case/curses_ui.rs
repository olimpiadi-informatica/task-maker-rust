use itertools::Itertools;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use task_maker_format::ui::curses::{BLUE, BOLD, GREEN, RED};
use task_maker_format::ui::{inner_block, render_block, render_server_status, CursesDrawer};

use crate::tools::find_bad_case::state::{SharedUIState, TestcaseStatus, UIState};

pub struct CursesUI;

impl CursesDrawer<UIState> for CursesUI {
    fn draw(state: &UIState, frame: &mut Frame, loading: char, frame_index: usize) {
        CursesUI::draw_frame(state, frame, loading, frame_index);
    }
}

impl CursesUI {
    fn draw_frame(state: &UIState, f: &mut Frame, loading: char, frame_index: usize) {
        let header_len = 10; // Number of lines of the header.
        let workers_len = state
            .executor_status
            .as_ref()
            .map(|s| s.connected_workers.len())
            .unwrap_or(0) as u16
            + 2;
        // FIXME: shrink workers_len if needed
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints::<&[Constraint]>(
                [
                    Constraint::Length(header_len),
                    Constraint::Min(0),
                    Constraint::Length(workers_len),
                ]
                .as_ref(),
            )
            .split(f.area());

        let shared = state.shared.read().unwrap();
        Self::render_header(state, &shared, f, chunks[0]);
        Self::render_generation_status(state, f, chunks[1]);
        render_server_status(
            f,
            chunks[2],
            state.executor_status.as_ref(),
            loading,
            frame_index,
        );
    }

    fn render_header(state: &UIState, shared: &SharedUIState, f: &mut Frame, rect: Rect) {
        let errors = state
            .batches
            .iter()
            .flat_map(|b| b.testcase_status.iter())
            .filter(|tc| matches!(tc, TestcaseStatus::Error))
            .count();

        let text = vec![
            Line::from(vec![
                Span::styled("Solution:        ", *BOLD),
                Span::raw(state.solution.to_string_lossy().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Generator args:  ", *BOLD),
                Span::raw(state.generator_args.iter().join(" ")),
            ]),
            Line::from(vec![
                Span::styled("Batch size:      ", *BOLD),
                Span::raw(state.batch_size.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Batch index:     ", *BOLD),
                Span::raw(shared.batch_index.to_string()),
            ]),
            Line::from(vec![Span::styled("Progress:", *BLUE)]),
            Line::from(vec![
                Span::styled("    Generated:   ", *BOLD),
                Span::raw(state.progress.inputs_generated.to_string()),
            ]),
            Line::from(vec![
                Span::styled("    Solved:      ", *BOLD),
                Span::raw(state.progress.inputs_solved.to_string()),
            ]),
            Line::from(vec![
                Span::styled("    Average gen: ", *BOLD),
                Span::raw(format!(
                    "{:.3}s\n",
                    state.progress.generator_time_sum
                        / (state.progress.inputs_generated.max(1) as f64)
                )),
            ]),
            Line::from(vec![
                Span::styled("    Average sol: ", *BOLD),
                Span::raw(format!(
                    "{:.3}s",
                    state.progress.solution_time_sum / (state.progress.inputs_solved.max(1) as f64)
                )),
            ]),
            Line::from(vec![
                Span::styled("    Errors:      ", *BOLD),
                Span::raw(errors.to_string()),
            ]),
        ];

        let paragraph = Paragraph::new(text);
        f.render_widget(paragraph, rect);
    }

    fn render_generation_status(state: &UIState, f: &mut Frame, rect: Rect) {
        let mut text = vec![];
        for i in 0..state.batches.len().min(10) {
            let mut line = Vec::new();
            let batch_index = state.batches.len() - 1 - i;
            let batch = &state.batches[state.batches.len() - 1 - i];
            line.push(Span::raw(format!("Batch {batch_index:>3}: ")));
            line.extend(
                batch
                    .testcase_status
                    .iter()
                    .map(Self::testcase_status_to_text),
            );
            text.push(line.into());
        }
        render_block(f, rect, "Progress");
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });
        f.render_widget(paragraph, inner_block(rect));
    }

    fn testcase_status_to_text(status: &TestcaseStatus) -> Span {
        match status {
            TestcaseStatus::Pending => Span::raw("."),
            TestcaseStatus::Generating => Span::raw("g"),
            TestcaseStatus::Generated => Span::raw("G"),
            TestcaseStatus::Validating => Span::raw("v"),
            TestcaseStatus::Validated => Span::raw("V"),
            TestcaseStatus::Solving => Span::raw("s"),
            TestcaseStatus::Solved => Span::raw("S"),
            TestcaseStatus::Checking => Span::raw("c"),
            TestcaseStatus::Success => Span::styled("✓", *GREEN),
            TestcaseStatus::Failed(_) => Span::styled("✕", *RED),
            TestcaseStatus::Error => Span::styled("!", *RED),
        }
    }
}
