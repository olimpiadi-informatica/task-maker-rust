use crate::ui::*;
use failure::Error;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{Builder, JoinHandle};
use termion::input::MouseTerminal;
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::Terminal;

/// The type of the terminal with its backend.
type TerminalType =
    Terminal<TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<io::Stdout>>>>>;

/// The animated UI for IOI tasks
pub struct IOICursesUI {
    /// The thread where the UI lives.
    ui_thread: Option<JoinHandle<()>>,
    /// The state of the task for the UI.
    state: Arc<RwLock<ioi_state::IOIUIState>>,
    /// When it becomes true the UI will stop.
    stop: Arc<AtomicBool>,
}

impl IOICursesUI {
    /// Try to make a new IOICursesUI setting up the terminal.
    pub fn new() -> Result<IOICursesUI, Error> {
        let stdout = io::stdout().into_raw_mode()?;
        let stdout = MouseTerminal::from(stdout);
        let stdout = AlternateScreen::from(stdout);
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;

        let state = Arc::new(RwLock::new(ioi_state::IOIUIState::new()));
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
fn ui_body(
    mut terminal: TerminalType,
    state: Arc<RwLock<ioi_state::IOIUIState>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        let text = Text::raw(format!("Num events: {}", state.read().unwrap().num_events));
        terminal
            .draw(|mut f| {
                let size = f.size();
                Paragraph::new([text].iter())
                    .block(Block::default().borders(Borders::NONE))
                    .wrap(true)
                    .render(&mut f, size)
            })
            .unwrap();
        std::thread::sleep_ms(1000);
    }
}
