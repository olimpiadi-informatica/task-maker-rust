use crate::ui::ioi_state::*;
use crate::ui::*;
use failure::Error;
use itertools::Itertools;
use std::io;
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

/// The framerate of the UI.
const FPS: u64 = 30;

/// The type of the terminal with its backend.
type TerminalType =
    Terminal<TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<io::Stdout>>>>>;

/// The animated UI for IOI tasks
pub struct IOICursesUI {
    /// The thread where the UI lives.
    ui_thread: Option<JoinHandle<()>>,
    /// The state of the task for the UI.
    state: Arc<RwLock<IOIUIState>>,
    /// When it becomes true the UI will stop.
    stop: Arc<AtomicBool>,
}

impl IOICursesUI {
    /// Try to make a new IOICursesUI setting up the terminal.
    pub fn new(task: UIMessage) -> Result<IOICursesUI, Error> {
        let stdout = io::stdout().into_raw_mode()?;
        let stdout = MouseTerminal::from(stdout);
        let stdout = AlternateScreen::from(stdout);
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;

        let state = Arc::new(RwLock::new(IOIUIState::new(task)));
        let state2 = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = Builder::new()
            .name("IOICursesUI thread".to_owned())
            .spawn(move || {
                ui_body(terminal, state2, stop2);
            })?;
        Ok(IOICursesUI {
            ui_thread: Some(handle),
            state,
            stop,
        })
    }
}

impl UI for IOICursesUI {
    fn on_message(&mut self, message: UIMessage) {
        self.state.write().unwrap().apply(message);
    }
}

impl Drop for IOICursesUI {
    fn drop(&mut self) {
        // tell the ui to stop and wait for it, the terminal will be released.
        self.stop.store(true, Ordering::Relaxed);
        self.ui_thread.take().unwrap().join().unwrap();
    }
}

/// WIP: the main function of the UI thread.
fn ui_body(mut terminal: TerminalType, state: Arc<RwLock<IOIUIState>>, stop: Arc<AtomicBool>) {
    let header = {
        let state = state.read().unwrap();
        [
            Text::styled(
                state.title.clone(),
                Style::default().modifier(Modifier::BOLD),
            ),
            Text::raw(" ("),
            Text::raw(state.name.clone()),
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
            })
            .unwrap();
        std::thread::sleep(std::time::Duration::from_micros(1_000_000 / FPS));
    }
}

/// Get the rect of the inner rect of a block with the borders.
fn inner_block(rect: Rect) -> Rect {
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

/// Draw the content of the compilation box.
fn draw_compilations<B>(frame: &mut Frame<B>, rect: Rect, state: &IOIUIState, loading: char)
where
    B: Backend,
{
    let max_len = state
        .compilations
        .keys()
        .map(|k| k.as_os_str().len())
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
                    file.to_string_lossy(),
                    max_len = max_len
                )),
                compilation_status_text(status, loading),
                Text::raw("\n"),
            ]
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Get the Text relative to the compilation status of a file.
fn compilation_status_text(status: &CompilationStatus, loading: char) -> Text<'static> {
    match status {
        CompilationStatus::Pending => Text::raw("..."),
        CompilationStatus::Running => Text::raw(format!("{}", loading)),
        CompilationStatus::Done => Text::styled(
            "OK",
            Style::default().fg(Color::Green).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Failed => Text::styled(
            "FAIL",
            Style::default().fg(Color::Red).modifier(Modifier::BOLD),
        ),
        CompilationStatus::Skipped => Text::styled("skipped", Style::default().fg(Color::Yellow)),
    }
}

/// Draw the content of the generation box.
fn draw_generations<B>(frame: &mut Frame<B>, rect: Rect, state: &IOIUIState, loading: char)
where
    B: Backend,
{
    let text: Vec<Text> = state
        .generations
        .iter()
        .sorted_by_key(|(k, _)| *k)
        .flat_map(|(_, subtask)| {
            let mut testcases: Vec<Text> = subtask
                .iter()
                .sorted_by_key(|(k, _)| *k)
                .map(|(_, tc)| generation_status_text(tc, loading))
                .collect();
            let mut res = vec![Text::raw("[")];
            res.append(&mut testcases);
            res.push(Text::raw("]"));
            res
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

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
