use itertools::Itertools;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::terry::finish_ui::FinishUI;
use crate::terry::ui_state::{SolutionState, SolutionStatus, UIState};
use crate::terry::{CaseStatus, SolutionOutcome};
use crate::ui::curses::{
    compilation_status_text, draw_compilations, inner_block, render_block, render_server_status,
    CursesDrawer, CursesUI as GenericCursesUI, GREEN, RED, YELLOW,
};
use crate::ui::FinishUIUtils;

/// An animated UI for Terry tasks, dynamically refreshing using curses as a backend.
pub(crate) type CursesUI = GenericCursesUI<UIState, Drawer, FinishUI>;

/// The drawer of the Terry CursesUI.
pub(crate) struct Drawer;

impl CursesDrawer<UIState> for Drawer {
    fn draw(state: &UIState, frame: &mut Frame, loading: char, frame_index: usize) {
        draw_frame(state, frame, loading, frame_index);
    }
}

/// Draw a frame of interface to the provided `Frame`.
fn draw_frame(state: &UIState, f: &mut Frame, loading: char, frame_index: usize) {
    let header: Line = vec![
        Span::styled(
            state.task.description.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ("),
        Span::raw(state.task.name.clone()),
        Span::raw(")"),
    ]
    .into();
    let header_len = 2;
    let num_compilations = state
        .compilations
        .iter()
        .filter(|(k, _)| !state.solutions.contains_key(*k))
        .count();
    let compilations_len = if num_compilations > 0 {
        num_compilations as u16 + 2
    } else {
        0
    };
    let evaluations_len = state.solutions.len() as u16 + 2;
    let mut workers_len = state
        .executor_status
        .as_ref()
        .map(|s| s.connected_workers.len())
        .unwrap_or(0) as u16
        + 2;
    let total_height = f.area().height;
    // fixed size section heights
    let top_height = header_len + compilations_len;
    // if the sections don't just fit, reduce the size of the workers until they fit but
    // without shortening it more than 3 lines (aka box + 1 worker).
    if top_height + evaluations_len + workers_len > total_height {
        workers_len = std::cmp::max(
            3,
            total_height as i16 - top_height as i16 - evaluations_len as i16,
        ) as u16;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints::<&[Constraint]>(
            [
                Constraint::Length(header_len),
                Constraint::Length(compilations_len),
                Constraint::Min(0),
                Constraint::Length(workers_len),
            ]
            .as_ref(),
        )
        .split(f.area());
    let paragraph = Paragraph::new(header).block(Block::default().borders(Borders::NONE));
    f.render_widget(paragraph, chunks[0]);
    if compilations_len > 0 {
        render_block(f, chunks[1], " Compilations ");
        draw_compilations(
            f,
            inner_block(chunks[1]),
            state
                .compilations
                .iter()
                .filter(|(p, _)| !state.solutions.contains_key(*p)),
            loading,
        );
    }
    render_block(f, chunks[2], " Evaluations ");
    draw_evaluations(f, inner_block(chunks[2]), state, loading);
    render_server_status(
        f,
        chunks[3],
        state.executor_status.as_ref(),
        loading,
        frame_index,
    );
}

/// Draw the evaluations of the solutions.
fn draw_evaluations(frame: &mut Frame, rect: Rect, state: &UIState, loading: char) {
    let max_len = FinishUIUtils::get_max_len(&state.solutions);
    let text: Vec<Line> = state
        .solutions
        .keys()
        .sorted()
        .map(|path| {
            let solution_state = &state.solutions[path];
            let mut spans = vec![Span::raw(format!(
                "{:<max_len$}  ",
                path.file_name()
                    .expect("Invalid file name")
                    .to_string_lossy(),
                max_len = max_len
            ))];
            if let Some(comp_status) = state.compilations.get(path) {
                spans.push(compilation_status_text(comp_status, loading));
            } else {
                spans.push(Span::raw("    "));
            }
            spans.push(Span::raw(" "));
            spans.push(evaluation_score(
                state.task.max_score,
                solution_state,
                loading,
            ));
            spans.push(Span::raw("  "));
            spans.append(&mut evaluation_line(solution_state));
            spans.into()
        })
        .collect();
    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, rect);
}

/// Return the line with the status of the evaluation of a solution.
fn evaluation_line<'a>(state: &SolutionState) -> Vec<Span<'a>> {
    match &state.status {
        SolutionStatus::Pending => vec![],
        SolutionStatus::Generating => vec![Span::raw("Generating")],
        SolutionStatus::Generated => vec![Span::raw("Generated")],
        SolutionStatus::Validating => vec![Span::raw("Validating")],
        SolutionStatus::Validated => vec![Span::raw("Validated")],
        SolutionStatus::Solving => vec![Span::raw("Solving")],
        SolutionStatus::Solved => vec![Span::raw("Solved")],
        SolutionStatus::Checking => vec![Span::raw("Checking")],
        SolutionStatus::Done => evaluation_outcome(state.outcome.as_ref()),
        SolutionStatus::Failed(e) => vec![Span::raw(format!("Failed: {e}"))],
        SolutionStatus::Skipped => vec![Span::raw("Skipped")],
    }
}

/// Return the line with the outcome of the evaluation of a solution.
fn evaluation_outcome<'a>(outcome: Option<&Result<SolutionOutcome, String>>) -> Vec<Span<'a>> {
    match outcome {
        Some(Ok(outcome)) => {
            let mut res = Vec::new();
            for (val, feed) in outcome
                .validation
                .cases
                .iter()
                .zip(outcome.feedback.cases.iter())
            {
                match val.status {
                    CaseStatus::Missing => res.push(Span::styled("m ", *YELLOW)),
                    CaseStatus::Parsed => {
                        if feed.correct {
                            res.push(Span::styled("c ", *GREEN))
                        } else {
                            res.push(Span::styled("w ", *RED))
                        }
                    }
                    CaseStatus::Invalid => res.push(Span::styled("i ", *RED)),
                }
            }
            res
        }
        Some(Err(e)) => vec![Span::raw(format!("Checker failed: {e}"))],
        None => vec![Span::raw("unknown")],
    }
}

/// Return the score column of a solution.
fn evaluation_score<'a>(max_score: f64, state: &SolutionState, loading: char) -> Span<'a> {
    match state.status {
        SolutionStatus::Pending => Span::raw("..."),
        SolutionStatus::Generating
        | SolutionStatus::Generated
        | SolutionStatus::Validating
        | SolutionStatus::Validated
        | SolutionStatus::Solving
        | SolutionStatus::Solved
        | SolutionStatus::Checking => Span::raw(format!(" {loading} ")),
        SolutionStatus::Done => {
            if let Some(Ok(outcome)) = &state.outcome {
                let score = format!("{:>3.0}", outcome.score * max_score);
                if abs_diff_eq!(outcome.score, 0.0) {
                    Span::styled(score, *RED)
                } else if abs_diff_eq!(outcome.score, 1.0) {
                    Span::styled(score, *GREEN)
                } else {
                    Span::styled(score, *YELLOW)
                }
            } else {
                Span::raw(format!(" {loading} "))
            }
        }
        SolutionStatus::Failed(_) | SolutionStatus::Skipped => Span::raw(" X "),
    }
}
