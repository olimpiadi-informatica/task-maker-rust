use itertools::Itertools;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::widgets::{Paragraph, Text, Widget};

use task_maker_format::ui::{
    inner_block, render_block, render_server_status, CursesDrawer, FrameType,
};

use crate::tools::find_bad_case::state::{SharedUIState, TestcaseStatus, UIState};

pub struct CursesUI;

impl CursesDrawer<UIState> for CursesUI {
    fn draw(state: &UIState, f: FrameType, loading: char, frame_index: usize) {
        CursesUI::draw_frame(state, f, loading, frame_index);
    }
}

impl CursesUI {
    fn draw_frame(state: &UIState, mut f: FrameType, loading: char, frame_index: usize) {
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
            .constraints(
                [
                    Constraint::Length(header_len),
                    Constraint::Min(0),
                    Constraint::Length(workers_len),
                ]
                .as_ref(),
            )
            .split(f.size());

        let shared = state.shared.read().unwrap();
        Self::render_header(state, &shared, &mut f, chunks[0]);
        Self::render_generation_status(state, &mut f, chunks[1]);
        render_server_status(
            &mut f,
            chunks[2],
            state.executor_status.as_ref(),
            loading,
            frame_index,
        );
    }

    fn render_header(state: &UIState, shared: &SharedUIState, f: &mut FrameType, rect: Rect) {
        let errors = state
            .batches
            .iter()
            .flat_map(|b| b.testcase_status.iter())
            .filter(|tc| matches!(tc, TestcaseStatus::Error))
            .count();
        Paragraph::new(
            [
                Text::raw(format!("Solution:        {}\n", state.solution.display())),
                Text::raw(format!(
                    "Generator args:  {}\n",
                    state.generator_args.iter().join(" ")
                )),
                Text::raw(format!("Batch size:      {}\n", state.batch_size)),
                Text::raw(format!("Batch index:     {}\n", shared.batch_index)),
                Text::raw("Progress:\n"),
                Text::raw(format!(
                    "    Generated:   {}\n",
                    state.progress.inputs_generated
                )),
                Text::raw(format!(
                    "    Solved:      {}\n",
                    state.progress.inputs_solved
                )),
                Text::raw(format!(
                    "    Average gen: {:.3}s\n",
                    state.progress.generator_time_sum
                        / (state.progress.inputs_generated.max(1) as f64)
                )),
                Text::raw(format!(
                    "    Average sol: {:.3}s\n",
                    state.progress.solution_time_sum / (state.progress.inputs_solved.max(1) as f64)
                )),
                Text::raw(format!("    Errors:      {}", errors)),
            ]
            .iter(),
        )
        .render(f, rect);
    }

    fn render_generation_status(state: &UIState, f: &mut FrameType, rect: Rect) {
        let mut text = vec![];
        for i in 0..state.batches.len().min(10) {
            let batch_index = state.batches.len() - 1 - i;
            let batch = &state.batches[state.batches.len() - 1 - i];
            text.push(Text::raw(format!("Batch {:>3}: ", batch_index)));
            text.extend(
                batch
                    .testcase_status
                    .iter()
                    .map(Self::testcase_status_to_text),
            );
            text.push(Text::raw("\n"));
        }
        render_block(f, rect, "Progress");
        Paragraph::new(text.iter())
            .wrap(true)
            .render(f, inner_block(rect));
    }

    fn testcase_status_to_text(status: &TestcaseStatus) -> Text {
        match status {
            TestcaseStatus::Pending => Text::raw("."),
            TestcaseStatus::Generating => Text::raw("g"),
            TestcaseStatus::Generated => Text::raw("G"),
            TestcaseStatus::Validating => Text::raw("v"),
            TestcaseStatus::Validated => Text::raw("V"),
            TestcaseStatus::Solving => Text::raw("s"),
            TestcaseStatus::Solved => Text::raw("S"),
            TestcaseStatus::Checking => Text::raw("c"),
            TestcaseStatus::Success => Text::raw("✓"),
            TestcaseStatus::Failed(_) => Text::raw("✕"),
            TestcaseStatus::Error => Text::raw("!"),
        }
    }
}
