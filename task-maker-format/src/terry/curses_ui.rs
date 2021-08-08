use itertools::Itertools;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};

use crate::terry::finish_ui::FinishUI;
use crate::terry::ui_state::{SolutionState, SolutionStatus, UIState};
use crate::terry::{CaseStatus, SolutionOutcome};
use crate::ui::curses::{
    compilation_status_text, draw_compilations, inner_block, render_block, render_server_status,
    CursesDrawer, CursesUI as GenericCursesUI, FrameType,
};
use crate::ui::FinishUIUtils;

/// An animated UI for Terry tasks, dynamically refreshing using curses as a backend.
pub(crate) type CursesUI = GenericCursesUI<UIState, Drawer, FinishUI>;

/// The drawer of the Terry CursesUI.
pub(crate) struct Drawer;

impl CursesDrawer<UIState> for Drawer {
    fn draw(state: &UIState, frame: FrameType, loading: char, frame_index: usize) {
        draw_frame(state, frame, loading, frame_index);
    }
}

/// Draw a frame of interface to the provided `Frame`.
fn draw_frame(state: &UIState, mut f: FrameType, loading: char, frame_index: usize) {
    let header = [
        Text::styled(
            state.task.description.clone(),
            Style::default().modifier(Modifier::BOLD),
        ),
        Text::raw(" ("),
        Text::raw(state.task.name.clone()),
        Text::raw(")\n"),
    ];
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
    let total_height = f.size().height;
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
        .constraints(
            [
                Constraint::Length(header_len),
                Constraint::Length(compilations_len),
                Constraint::Min(0),
                Constraint::Length(workers_len),
            ]
            .as_ref(),
        )
        .split(f.size());
    Paragraph::new(header.iter())
        .block(Block::default().borders(Borders::NONE))
        .wrap(false)
        .render(&mut f, chunks[0]);
    if compilations_len > 0 {
        render_block(&mut f, chunks[1], " Compilations ");
        draw_compilations(
            &mut f,
            inner_block(chunks[1]),
            state
                .compilations
                .iter()
                .filter(|(p, _)| !state.solutions.contains_key(*p)),
            loading,
        );
    }
    render_block(&mut f, chunks[2], " Evaluations ");
    draw_evaluations(&mut f, inner_block(chunks[2]), state, loading);
    render_server_status(
        &mut f,
        chunks[3],
        state.executor_status.as_ref(),
        loading,
        frame_index,
    );
}

/// Draw the evaluations of the solutions.
fn draw_evaluations(frame: &mut FrameType, rect: Rect, state: &UIState, loading: char) {
    let max_len = FinishUIUtils::get_max_len(&state.solutions);
    let text: Vec<_> = state
        .solutions
        .keys()
        .sorted()
        .flat_map(|path| {
            let solution_state = &state.solutions[path];
            let mut texts = vec![Text::raw(format!(
                "{:<max_len$}  ",
                path.file_name()
                    .expect("Invalid file name")
                    .to_string_lossy(),
                max_len = max_len
            ))];
            if let Some(comp_status) = state.compilations.get(path) {
                texts.push(compilation_status_text(comp_status, loading));
            } else {
                texts.push(Text::raw("    "));
            }
            texts.push(Text::raw(" "));
            texts.push(evaluation_score(
                state.task.max_score,
                solution_state,
                loading,
            ));
            texts.push(Text::raw("  "));
            texts.append(&mut evaluation_line(solution_state));
            texts.push(Text::raw("\n"));
            texts
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Return the line with the status of the evaluation of a solution.
fn evaluation_line<'a>(state: &SolutionState) -> Vec<Text<'a>> {
    match &state.status {
        SolutionStatus::Pending => vec![],
        SolutionStatus::Generating => vec![Text::raw("Generating")],
        SolutionStatus::Generated => vec![Text::raw("Generated")],
        SolutionStatus::Validating => vec![Text::raw("Validating")],
        SolutionStatus::Validated => vec![Text::raw("Validated")],
        SolutionStatus::Solving => vec![Text::raw("Solving")],
        SolutionStatus::Solved => vec![Text::raw("Solved")],
        SolutionStatus::Checking => vec![Text::raw("Checking")],
        SolutionStatus::Done => evaluation_outcome(state.outcome.as_ref()),
        SolutionStatus::Failed(e) => vec![Text::raw(format!("Failed: {}", e))],
        SolutionStatus::Skipped => vec![Text::raw("Skipped")],
    }
}

/// Return the line with the outcome of the evaluation of a solution.
fn evaluation_outcome<'a>(outcome: Option<&Result<SolutionOutcome, String>>) -> Vec<Text<'a>> {
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
                    CaseStatus::Missing => res.push(Text::styled(
                        "m ",
                        Style::default().fg(Color::Yellow).modifier(Modifier::BOLD),
                    )),
                    CaseStatus::Parsed => {
                        if feed.correct {
                            res.push(Text::styled(
                                "c ",
                                Style::default().fg(Color::Green).modifier(Modifier::BOLD),
                            ))
                        } else {
                            res.push(Text::styled(
                                "w ",
                                Style::default().fg(Color::Red).modifier(Modifier::BOLD),
                            ))
                        }
                    }
                    CaseStatus::Invalid => res.push(Text::styled(
                        "i ",
                        Style::default().fg(Color::Red).modifier(Modifier::BOLD),
                    )),
                }
            }
            res
        }
        Some(Err(e)) => vec![Text::raw(format!("Checker failed: {}", e))],
        None => vec![Text::raw("unknown")],
    }
}

/// Return the score column of a solution.
fn evaluation_score<'a>(max_score: f64, state: &SolutionState, loading: char) -> Text<'a> {
    match state.status {
        SolutionStatus::Pending => Text::raw("..."),
        SolutionStatus::Generating
        | SolutionStatus::Generated
        | SolutionStatus::Validating
        | SolutionStatus::Validated
        | SolutionStatus::Solving
        | SolutionStatus::Solved
        | SolutionStatus::Checking => Text::raw(format!(" {} ", loading)),
        SolutionStatus::Done => {
            if let Some(Ok(outcome)) = &state.outcome {
                let score = format!("{:>3.0}", outcome.score * max_score);
                if abs_diff_eq!(outcome.score, 0.0) {
                    Text::styled(
                        score,
                        Style::default().fg(Color::Red).modifier(Modifier::BOLD),
                    )
                } else if abs_diff_eq!(outcome.score, 1.0) {
                    Text::styled(
                        score,
                        Style::default().fg(Color::Green).modifier(Modifier::BOLD),
                    )
                } else {
                    Text::styled(
                        score,
                        Style::default().fg(Color::Yellow).modifier(Modifier::BOLD),
                    )
                }
            } else {
                Text::raw(format!(" {} ", loading))
            }
        }
        SolutionStatus::Failed(_) | SolutionStatus::Skipped => Text::raw(" X "),
    }
}
