use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{Builder, JoinHandle};
use std::time::SystemTime;

use failure::Error;
use itertools::Itertools;
use termion::event::{Event, Key};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::AlternateScreen;
use tui::backend::{Backend, TermionBackend};
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::{Frame, Terminal};

use task_maker_dag::ExecutionStatus;
use task_maker_exec::ExecutorWorkerStatus;

use crate::ioi::finish_ui::FinishUI;
use crate::ioi::*;
use crate::ui::{UIExecutionStatus, UIMessage, UI};

/// The framerate of the UI.
const FPS: u64 = 30;
/// After how many seconds rotate the list of workers if they don't fit on the screen.
const ROTATION_DELAY: u64 = 1;

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
        self.state
            .write()
            .expect("UI state lock is poisoned")
            .apply(message);
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.ui_thread
            .take()
            .expect("UI finished more than once")
            .join()
            .expect("UI thread failed");
        // at this point the terminal should be restored
        let state = self.state.read().expect("State lock is poisoned");
        FinishUI::print(&state);
    }
}

impl Drop for CursesUI {
    fn drop(&mut self) {
        // tell the ui to stop and wait for it, the terminal will be released.
        self.stop.store(true, Ordering::Relaxed);
        if self.ui_thread.is_some() {
            // try not to panic during unwind
            let _ = self.ui_thread.take().unwrap().join();
        }
    }
}

/// The main function of the UI thread. This will keep looping until the stop command is issued via
/// the `stop` `AtomicBool`.
fn ui_body(mut terminal: TerminalType, state: Arc<RwLock<UIState>>, stop: Arc<AtomicBool>) {
    let header = {
        let state = state.read().expect("UI state lock is poisoned");
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
    let stdin = termion::async_stdin();
    let mut events = stdin.events();
    while !stop.load(Ordering::Relaxed) {
        // FIXME: handling the ^C this way inhibits the real ^C handler. Doing so the workers may
        //        not be killed properly (locally and remotely).
        if let Some(Ok(event)) = events.next() {
            match event {
                Event::Key(Key::Ctrl('c')) | Event::Key(Key::Ctrl('\\')) => {
                    drop(terminal);
                    std::process::exit(1)
                }
                _ => {}
            }
        }
        let loading = loading[loading_index % loading.len()];
        loading_index += 1;
        terminal
            .draw(|mut f| {
                let state = state.read().expect("UI state lock is poisoned");
                let header_len = 2;
                let num_compilations = state
                    .compilations
                    .iter()
                    .filter(|(k, _)| !state.evaluations.contains_key(*k))
                    .count();
                let compilations_len = if num_compilations > 0 {
                    num_compilations as u16 + 2
                } else {
                    0
                };
                let booklet_len = state
                    .booklets
                    .values()
                    .map(|s| s.dependencies.len() as u16 + 1)
                    .sum::<u16>()
                    + 2;
                let generations_len = 3;
                let evaluations_len = state.evaluations.len() as u16 + 2;
                let mut workers_len = state
                    .executor_status
                    .as_ref()
                    .map(|s| s.connected_workers.len())
                    .unwrap_or(0) as u16
                    + 2;
                let total_height = f.size().height;
                // fixed size section heights
                let top_height = header_len + compilations_len + booklet_len + generations_len;
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
                            Constraint::Length(booklet_len),
                            Constraint::Length(generations_len),
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
                    Block::default()
                        .title(" Compilations ")
                        .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                        .render(&mut f, chunks[1]);
                    draw_compilations(&mut f, inner_block(chunks[1]), &state, loading);
                }
                Block::default()
                    .title(" Statements ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[2]);
                draw_booklets(&mut f, inner_block(chunks[2]), &state, loading);
                Block::default()
                    .title(" Generation ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[3]);
                draw_generations(&mut f, inner_block(chunks[3]), &state, loading);
                Block::default()
                    .title(" Evaluations ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[4]);
                draw_evaluations(&mut f, inner_block(chunks[4]), &state, loading);
                Block::default()
                    .title(" Server status ")
                    .title_style(Style::default().fg(Color::Blue).modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .render(&mut f, chunks[5]);
                draw_server_status_summary(
                    &mut f,
                    Rect::new(chunks[5].x + 17, chunks[5].y, chunks[5].width - 17, 1),
                    &state,
                );
                draw_server_status(
                    &mut f,
                    inner_block(chunks[5]),
                    &state,
                    loading,
                    loading_index / FPS as usize / ROTATION_DELAY as usize,
                );
            })
            .expect("Failed to draw to the screen");
        // reduce the framerate to at most `FPS`
        std::thread::sleep(std::time::Duration::from_micros(1_000_000 / FPS));
    }
}

/// Get the rect of the inner rect of a block with the borders.
fn inner_block(rect: Rect) -> Rect {
    if rect.width < 2 || rect.height < 2 {
        return Rect::new(rect.x + 1, rect.y + 1, 0, 0);
    }
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
        .map(|k| k.file_name().expect("Invalid file name").len())
        .max()
        .unwrap_or(0)
        + 4;
    let text: Vec<Text> = state
        .compilations
        .iter()
        .filter(|(k, _)| !state.evaluations.contains_key(*k))
        .sorted_by_key(|(k, _)| *k)
        .flat_map(|(file, status)| {
            vec![
                Text::raw(format!(
                    "{:<max_len$}",
                    file.file_name()
                        .expect("Invalid file name")
                        .to_string_lossy(),
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
        CompilationStatus::Pending => Text::raw("... "),
        CompilationStatus::Running => Text::raw(format!("{}   ", loading)),
        CompilationStatus::Done { .. } => Text::styled(
            "OK  ",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Failed { .. } => Text::styled(
            "FAIL",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Skipped => Text::styled("skip", Style::default().fg(Color::Yellow)),
    }
}

/// Draw the content of the booklet box.
fn draw_booklets<B>(frame: &mut Frame<B>, rect: Rect, state: &UIState, loading: char)
where
    B: Backend,
{
    let text: Vec<Text> = state
        .booklets
        .keys()
        .sorted()
        .flat_map(|name| {
            let booklet = &state.booklets[name];
            let mut res = vec![
                Text::raw(format!("{:<20} ", name)),
                ui_execution_status_text(&booklet.status, loading),
                Text::raw("\n"),
            ];
            for name in booklet.dependencies.keys().sorted() {
                let dep = &booklet.dependencies[name];
                res.push(Text::raw(format!("  {:<18} ", name)));
                res.push(Text::raw("["));
                for step in dep.iter() {
                    res.push(ui_execution_status_text(&step.status, loading));
                }
                res.push(Text::raw("]\n"));
            }
            res
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

fn ui_execution_status_text(status: &UIExecutionStatus, loading: char) -> Text {
    match status {
        UIExecutionStatus::Pending => Text::raw("."),
        UIExecutionStatus::Started { .. } => Text::raw(format!("{}", loading)),
        UIExecutionStatus::Skipped => Text::raw("S"),
        UIExecutionStatus::Done { result } => match &result.status {
            ExecutionStatus::Success => Text::styled(
                "S",
                Style::default().fg(Color::Green).modifier(Modifier::BOLD),
            ),
            ExecutionStatus::InternalError(_) => Text::raw("I"),
            _ => Text::styled(
                "F",
                Style::default().fg(Color::Red).modifier(Modifier::BOLD),
            ),
        },
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
        .map(|k| k.file_name().expect("Invalid file name").len())
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
                solution
                    .file_name()
                    .expect("Invalid file name")
                    .to_string_lossy(),
                max_len = max_len
            )));
            texts.push(Text::raw(" "));
            if let Some(comp_status) = state.compilations.get(solution) {
                texts.push(compilation_status_text(comp_status, loading));
            } else {
                texts.push(Text::raw("    "));
            }
            texts.push(Text::raw(" "));
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
    let sol_state = if let Some(state) = state.evaluations.get(solution) {
        state
    } else {
        return Text::raw("  ?  ");
    };
    if let Some(score) = sol_state.score {
        if score == 0.0 {
            Text::styled(
                format!(" {:>3.0} ", score),
                Style::default().fg(Color::Red).modifier(Modifier::BOLD),
            )
        } else if (score - state.max_score).abs() < 0.001 {
            Text::styled(
                format!(" {:>3.0} ", score),
                Style::default().fg(Color::Green).modifier(Modifier::BOLD),
            )
        } else {
            Text::styled(
                format!(" {:>3.0} ", score),
                Style::default().fg(Color::Yellow).modifier(Modifier::BOLD),
            )
        }
    } else {
        let has_skipped = sol_state.subtasks.values().any(|st| {
            st.testcases
                .values()
                .any(|tc| tc.status == TestcaseEvaluationStatus::Skipped)
        });
        if has_skipped {
            Text::raw("  X  ")
        } else {
            Text::raw(format!("  {}  ", loading))
        }
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
    let par_style = if let Some(normalized_score) = subtask.normalized_score {
        if abs_diff_eq!(normalized_score, 1.0) {
            Style::default().fg(Color::Green).modifier(Modifier::BOLD)
        } else if abs_diff_eq!(normalized_score, 0.0) {
            Style::default().fg(Color::Red).modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow).modifier(Modifier::BOLD)
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

fn draw_server_status_summary<B>(frame: &mut Frame<B>, rect: Rect, state: &UIState)
where
    B: Backend,
{
    let status = if let Some(status) = &state.executor_status {
        status
    } else {
        return;
    };
    Paragraph::new(
        [
            Text::styled(" Ready ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{} ─", status.ready_execs)),
            Text::styled(" Waiting ", Style::default().modifier(Modifier::BOLD)),
            Text::raw(format!("{} ", status.waiting_execs)),
        ]
        .iter(),
    )
    .wrap(false)
    .render(frame, rect);
}

/// Draw the content of the server status box, splitting the workers in 2 groups if they don't fit,
/// and rotating them if they still don't fit.
fn draw_server_status<B>(
    frame: &mut Frame<B>,
    rect: Rect,
    state: &UIState,
    loading: char,
    mut rotation_index: usize,
) where
    B: Backend,
{
    let status = if let Some(status) = &state.executor_status {
        status
    } else {
        return;
    };
    let rects = if status.connected_workers.len() as u16 > rect.height {
        vec![
            Rect::new(rect.x, rect.y, rect.width / 2, rect.height),
            Rect::new(
                rect.x + rect.width / 2,
                rect.y,
                rect.width - rect.width / 2,
                rect.height,
            ),
        ]
    } else {
        vec![rect]
    };
    let workers: Vec<_> = status
        .connected_workers
        .iter()
        .sorted_by_key(|worker| &worker.name)
        .collect();
    // if the workers fit in the screen there is no need to rotate them
    if rect.height as usize * rects.len() >= workers.len() {
        rotation_index = 0;
    }
    let chunks = workers
        .into_iter()
        .cycle()
        .skip(rotation_index)
        .chunks(rect.height as usize);
    for (rect, chunk) in rects.into_iter().zip(&chunks) {
        draw_workers_chunk(frame, rect, &chunk.collect_vec(), loading);
    }
}

/// Draw a chunk of workers in the specified rectangle.
fn draw_workers_chunk<B>(
    frame: &mut Frame<B>,
    rect: Rect,
    workers: &[&ExecutorWorkerStatus<SystemTime>],
    loading: char,
) where
    B: Backend,
{
    let max_len = workers
        .iter()
        .map(|worker| worker.name.len())
        .max()
        .unwrap_or(0);
    let text: Vec<Text> = workers
        .iter()
        .flat_map(|worker| {
            let mut texts = vec![];
            texts.push(Text::raw(format!(
                "- {:<max_len$} ",
                worker.name,
                max_len = max_len
            )));
            if let Some(job) = &worker.current_job {
                let duration =
                    (job.duration.elapsed().map(|d| d.as_millis()).unwrap_or(0) as f64) / 1000.0;
                let mut line = format!("{} {} ({:.2}s)", loading, job.job, duration);
                if 2 + max_len + 1 + line.len() > rect.width as usize {
                    let extra_len = 2 + max_len + 1 + line.len() - rect.width as usize;
                    let job_name = &job.job[0..job.job.len() - extra_len - 3];
                    line = format!("{} {}... ({:.2}s)", loading, job_name, duration);
                }
                texts.push(Text::raw(line));
            }
            texts.push(Text::raw("\n"));
            texts
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}
