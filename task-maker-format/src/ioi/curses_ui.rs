use crate::ioi::ui_state::*;
use crate::ioi::*;
use crate::ui::{UIMessage, UI};
use failure::Error;
use itertools::Itertools;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{Builder, JoinHandle};
use termion::input::MouseTerminal;
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::AlternateScreen;
use tui::backend::{Backend, TermionBackend};
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::{Frame, Terminal};
use crate::ioi::finish_ui::print_final_state;

/// The framerate of the UI.
const FPS: u64 = 30;

/// The type of the terminal with its backend.
type TerminalType =
    Terminal<TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<io::Stdout>>>>>;

/// An animated UI for IOI tasks, dynamically refreshing using curses as a backend.
pub struct CursesUI {
    /// The thread where the UI lives.
    ui_thread: Option<JoinHandle<()>>,
    /// The state of the task for the UI.
    state: Arc<RwLock<UIState>>,
    /// When it becomes true the UI will stop.
    stop: Arc<AtomicBool>,
}

impl CursesUI {
    /// Try to make a new CursesUI setting up the terminal. May fail on unsupported terminals.
    pub fn new(task: &Task) -> Result<CursesUI, Error> {
        let stdout = io::stdout().into_raw_mode()?;
        let stdout = MouseTerminal::from(stdout);
        let stdout = AlternateScreen::from(stdout);
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;

        let state = Arc::new(RwLock::new(UIState::new(task)));
        let state2 = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = Builder::new()
            .name("CursesUI thread".to_owned())
            .spawn(move || {
                ui_body(terminal, state2, stop2);
            })?;
        Ok(CursesUI {
            ui_thread: Some(handle),
            state,
            stop,
        })
    }
}

impl UI for CursesUI {
    fn on_message(&mut self, message: UIMessage) {
        self.state.write().unwrap().apply(message);
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.ui_thread.take().unwrap().join().unwrap();
        // at this point the terminal should be restored
        let state = self.state.read().unwrap();
        print_final_state(&state);
    }
}

impl Drop for CursesUI {
    fn drop(&mut self) {
        // tell the ui to stop and wait for it, the terminal will be released.
        self.stop.store(true, Ordering::Relaxed);
        if self.ui_thread.is_some() {
            self.ui_thread.take().unwrap().join().unwrap();
        }
    }
}

/// The main function of the UI thread. This will keep looping until the stop command is issued via
/// the `stop` `AtomicBool`.
fn ui_body(mut terminal: TerminalType, state: Arc<RwLock<UIState>>, stop: Arc<AtomicBool>) {
    let header = {
        let state = state.read().unwrap();
        [
            Text::styled(
                state.task.title.clone(),
                Style::default().modifier(Modifier::BOLD),
            ),
            Text::raw(" ("),
            Text::raw(state.task.name.clone()),
            Text::raw(")\n"),
        ]
    };
    let loading = vec!['◐', '◓', '◑', '◒'];
    let mut loading_index = 0;
    while !stop.load(Ordering::Relaxed) {
        let loading = loading[loading_index % loading.len()];
        loading_index += 1;
        terminal
            .draw(|mut f| {
                let state = state.read().unwrap();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        [
                            Constraint::Length(2),
                            Constraint::Length(state.compilations.len() as u16 + 2),
                            Constraint::Length(3),
                            Constraint::Min(0),
                        ]
                        .as_ref(),
                    )
                    .split(f.size());
                Paragraph::new(header.iter())
                    .block(Block::default().borders(Borders::NONE))
                    .wrap(false)
                    .render(&mut f, chunks[0]);
                Block::default()
                    .title(" Compilations ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[1]);
                draw_compilations(&mut f, inner_block(chunks[1]), &state, loading);
                Block::default()
                    .title(" Generation ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[2]);
                draw_generations(&mut f, inner_block(chunks[2]), &state, loading);
                Block::default()
                    .title(" Evaluations ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[3]);
                draw_evaluations(&mut f, inner_block(chunks[3]), &state, loading);
            })
            .unwrap();
        // reduce the framerate to at most `FPS`
        std::thread::sleep(std::time::Duration::from_micros(1_000_000 / FPS));
    }
}

/// Get the rect of the inner rect of a block with the borders.
fn inner_block(rect: Rect) -> Rect {
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

/// Draw the content of the compilation box inside the frame.
fn draw_compilations<B>(frame: &mut Frame<B>, rect: Rect, state: &UIState, loading: char)
where
    B: Backend,
{
    let max_len = state
        .compilations
        .keys()
        .map(|k| k.file_name().unwrap().len())
        .max()
        .unwrap_or(0)
        + 4;
    let text: Vec<Text> = state
        .compilations
        .iter()
        .sorted_by_key(|(k, _)| *k)
        .flat_map(|(file, status)| {
            vec![
                Text::raw(format!(
                    "{:<max_len$}",
                    file.file_name().unwrap().to_string_lossy(),
                    max_len = max_len
                )),
                compilation_status_text(status, loading),
                Text::raw("\n"),
            ]
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Get the `Text` relative to the compilation status of a file.
fn compilation_status_text(status: &CompilationStatus, loading: char) -> Text<'static> {
    match status {
        CompilationStatus::Pending => Text::raw("..."),
        CompilationStatus::Running => Text::raw(format!("{}", loading)),
        CompilationStatus::Done { .. } => Text::styled(
            "OK",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Failed { .. } => Text::styled(
            "FAIL",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Skipped => Text::styled("skipped", Style::default().fg(Color::Yellow)),
    }
}

/// Draw the content of the generation box.
fn draw_generations<B>(frame: &mut Frame<B>, rect: Rect, state: &UIState, loading: char)
where
    B: Backend,
{
    let text: Vec<Text> = state
        .generations
        .iter()
        .sorted_by_key(|(k, _)| *k)
        .flat_map(|(_, subtask)| {
            let mut testcases: Vec<Text> = subtask
                .testcases
                .iter()
                .sorted_by_key(|(k, _)| *k)
                .map(|(_, tc)| generation_status_text(&tc.status, loading))
                .collect();
            let mut res = vec![Text::raw("[")];
            res.append(&mut testcases);
            res.push(Text::raw("]"));
            res
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Get the colored character corresponding to the status of the generation of a testcase.
fn generation_status_text(status: &TestcaseGenerationStatus, loading: char) -> Text {
    match status {
        TestcaseGenerationStatus::Pending => Text::raw("."),
        TestcaseGenerationStatus::Generating => Text::raw(format!("{}", loading)),
        TestcaseGenerationStatus::Generated => Text::styled(
            "G",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        TestcaseGenerationStatus::Validating => Text::raw(format!("{}", loading)),
        TestcaseGenerationStatus::Validated => Text::styled(
            "V",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        TestcaseGenerationStatus::Solving => Text::raw(format!("{}", loading)),
        TestcaseGenerationStatus::Solved => Text::styled(
            "S",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        TestcaseGenerationStatus::Failed => Text::styled(
            "F",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseGenerationStatus::Skipped => Text::styled("s", Style::default().fg(Color::Yellow)),
    }
}

/// Draw the content of the evaluation box.
fn draw_evaluations<B>(frame: &mut Frame<B>, rect: Rect, state: &UIState, loading: char)
where
    B: Backend,
{
    let max_len = state
        .evaluations
        .keys()
        .map(|k| k.file_name().unwrap().len())
        .max()
        .unwrap_or(0)
        + 4;
    let text: Vec<Text> = state
        .evaluations
        .keys()
        .sorted()
        .flat_map(|solution| {
            let mut texts = vec![];
            texts.push(Text::raw(format!(
                "{:<max_len$}",
                solution.file_name().unwrap().to_string_lossy(),
                max_len = max_len
            )));
            texts.push(evaluation_score(state, solution, loading));
            texts.append(&mut evaluation_line(state, solution, loading));
            texts.push(Text::raw("\n"));
            texts
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Get the colored score of a solution.
fn evaluation_score<'a>(state: &'a UIState, solution: &Path, loading: char) -> Text<'a> {
    if let Some(Some(score)) = state.evaluations.get(solution).map(|s| s.score) {
        if score == 0.0 {
            Text::styled(
                format!(" {:>3} ", score),
                Style::default().fg(Color::Red).modifier(Modifier::BOLD),
            )
        } else if (score - state.max_score).abs() < 0.001 {
            Text::styled(
                format!(" {:>3} ", score),
                Style::default().fg(Color::Green).modifier(Modifier::BOLD),
            )
        } else {
            Text::styled(
                format!(" {:>3} ", score),
                Style::default().fg(Color::Yellow).modifier(Modifier::BOLD),
            )
        }
    } else {
        Text::raw(format!("  {}  ", loading))
    }
}

/// Get the line at the right of the score of a solution.
fn evaluation_line<'a>(state: &'a UIState, solution: &Path, loading: char) -> Vec<Text<'a>> {
    state
        .task
        .subtasks
        .keys()
        .sorted()
        .flat_map(|st| subtask_evaluation_status_text(state, solution, *st, loading))
        .collect()
}

/// Get the status of a subtask, like `[AATTR]` where each letter corresponds to
/// the status of a single testcase.
fn subtask_evaluation_status_text<'a>(
    state: &'a UIState,
    solution: &Path,
    subtask: SubtaskId,
    loading: char,
) -> Vec<Text<'a>> {
    let mut texts = vec![];
    let solution = &state.evaluations[solution];
    if !solution.subtasks.contains_key(&subtask) {
        return vec![Text::raw("[---]")];
    }
    let subtask = &solution.subtasks[&subtask];
    let mut all_succeded = true;
    let mut all_completed = true;
    let mut partial = false;
    for testcase in subtask.testcases.values() {
        if !testcase.status.is_success() {
            all_succeded = false;
        }
        if !testcase.status.has_completed() {
            all_completed = false;
        }
        if testcase.status.is_partial() {
            partial = true;
        }
    }
    let par_style = if all_completed {
        if all_succeded {
            Style::default().fg(Color::Green).modifier(Modifier::BOLD)
        } else if partial {
            Style::default().fg(Color::Yellow).modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red).modifier(Modifier::BOLD)
        }
    } else {
        Style::default()
    };
    texts.push(Text::styled("[", par_style));
    for (_, testcase) in subtask.testcases.iter().sorted_by_key(|(k, _)| *k) {
        texts.push(testcase_evaluation_status_text(&testcase.status, loading));
    }
    texts.push(Text::styled("]", par_style));
    texts
}

/// Get the colored character corresponding to the status of the evaluation of a testcase.
fn testcase_evaluation_status_text(status: &TestcaseEvaluationStatus, loading: char) -> Text {
    match status {
        TestcaseEvaluationStatus::Pending => Text::raw("."),
        TestcaseEvaluationStatus::Solving => Text::raw(format!("{}", loading)),
        TestcaseEvaluationStatus::Solved => Text::raw("s"),
        TestcaseEvaluationStatus::Checking => Text::raw(format!("{}", loading)),
        TestcaseEvaluationStatus::Accepted(_) => Text::styled(
            "A",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::WrongAnswer(_) => Text::styled(
            "W",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::Partial(_) => Text::styled(
            "P",
            Style::default().fg(Color::Yellow).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::TimeLimitExceeded => Text::styled(
            "T",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::WallTimeLimitExceeded => Text::styled(
            "T",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::MemoryLimitExceeded => Text::styled(
            "M",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::RuntimeError => Text::styled(
            "R",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::Failed => Text::styled(
            "F",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .modifier(Modifier::BOLD),
        ),
        TestcaseEvaluationStatus::Skipped => Text::raw("X"),
    }
}
