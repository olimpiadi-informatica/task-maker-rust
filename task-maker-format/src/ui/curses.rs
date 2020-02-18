use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use failure::Error;
use termion::event::{Event, Key};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::{Frame, Terminal};

use crate::ui::{FinishUI, UIMessage, UIStateT, UI};
use std::marker::PhantomData;

/// The framerate of the UI.
pub(crate) const FPS: u64 = 30;

/// The type of the terminal with its backend.
pub(crate) type FrameType<'a> =
    Frame<'a, TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<io::Stdout>>>>>;

/// A generic animated UI for tasks, dynamically refreshing using curses as a backend.
///
/// - `State` is the type of `UIState` for this UI.
/// - `Drawer` is the drawer for the frames of the UI.
/// - `FinishUI` is the UI that prints the final results.
pub struct CursesUI<State, Drawer, Finish>
where
    State: UIStateT + Send + Sync + 'static,
    Drawer: CursesDrawer<State> + Send + Sync + 'static,
    Finish: FinishUI<State> + Send + Sync + 'static,
{
    /// The thread where the UI lives.
    ui_thread: Option<JoinHandle<()>>,
    /// The state of the task for the UI.
    state: Arc<RwLock<State>>,
    /// When it becomes true the UI will stop.
    stop: Arc<AtomicBool>,

    drawer: PhantomData<Drawer>,
    finish_ui: PhantomData<Finish>,
}

/// A drawer for the frames of the UI.
pub trait CursesDrawer<State> {
    /// Draw a frame of the UI using the provided state, onto the frame, using the loading
    /// character. Frame index is a counter of the number of frames encountered so far.
    fn draw(state: &State, frame: FrameType, loading: char, frame_index: usize);
}

impl<State, Drawer, Finish> CursesUI<State, Drawer, Finish>
where
    State: UIStateT + Send + Sync + 'static,
    Drawer: CursesDrawer<State> + Send + Sync + 'static,
    Finish: FinishUI<State> + Send + Sync + 'static,
{
    /// Make a new generic `CursesUI`.
    pub fn new(state: State) -> Result<CursesUI<State, Drawer, Finish>, Error> {
        let state = Arc::new(RwLock::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let mut ui = CursesUI {
            ui_thread: None,
            state: state.clone(),
            stop: stop.clone(),
            drawer: Default::default(),
            finish_ui: Default::default(),
        };
        let handle = ui.start(state, stop)?;
        ui.ui_thread = Some(handle);
        Ok(ui)
    }

    /// Start the drawing thread of the UI, returning the `JoinHandle` of it.
    fn start(
        &mut self,
        state: Arc<RwLock<State>>,
        stop: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, Error> {
        let stdout = io::stdout().into_raw_mode()?;
        let stdout = MouseTerminal::from(stdout);
        let stdout = AlternateScreen::from(stdout);
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;
        Ok(std::thread::Builder::new()
            .name("CursesUI thread".to_owned())
            .spawn(move || {
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
                    terminal
                        .draw(|f| {
                            let state = state.read().expect("UI state lock is poisoned");
                            Drawer::draw(&state, f, loading, loading_index);
                        })
                        .expect("Failed to draw to the screen");
                    // reduce the framerate to at most `FPS`
                    std::thread::sleep(std::time::Duration::from_micros(1_000_000 / FPS));
                    loading_index += 1;
                }
            })?)
    }
}

impl<State, Drawer, Finish> UI for CursesUI<State, Drawer, Finish>
where
    State: UIStateT + Send + Sync + 'static,
    Drawer: CursesDrawer<State> + Send + Sync + 'static,
    Finish: FinishUI<State> + Send + Sync + 'static,
{
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
        Finish::print(&state);
    }
}

impl<State, Drawer, Finish> Drop for CursesUI<State, Drawer, Finish>
where
    State: UIStateT + Send + Sync + 'static,
    Drawer: CursesDrawer<State> + Send + Sync + 'static,
    Finish: FinishUI<State> + Send + Sync + 'static,
{
    fn drop(&mut self) {
        // tell the ui to stop and wait for it, the terminal will be released.
        self.stop.store(true, Ordering::Relaxed);
        if self.ui_thread.is_some() {
            // try not to panic during unwind
            let _ = self.ui_thread.take().unwrap().join();
        }
    }
}
