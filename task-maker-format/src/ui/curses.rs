//! Utilities for writing UIs with Curses.

use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::SystemTime;

use anyhow::Error;
use itertools::Itertools;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use termion::event::{Event, Key};
use termion::input::{MouseTerminal, TermRead};
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::layout::Rect;
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::{Frame, Terminal};

use task_maker_exec::{ExecutorStatus, ExecutorWorkerStatus};

use crate::ui::{CompilationStatus, FinishUI, UIMessage, UIStateT, UI};

/// The framerate of the UI.
pub(crate) const FPS: u64 = 30;
/// After how many seconds rotate the list of workers if they don't fit on the screen.
pub(crate) const ROTATION_DELAY: u64 = 1;

/// The type of the terminal with its backend.
pub type FrameType<'a> =
    Frame<'a, TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<io::Stdout>>>>>;

lazy_static! {
    /// Green color.
    pub static ref GREEN: Style = Style::default()
        .fg(Color::LightGreen)
        .modifier(Modifier::BOLD);
    /// Red color.
    pub static ref RED: Style = Style::default()
        .fg(Color::LightRed)
        .modifier(Modifier::BOLD);
    /// Blue color.
    pub static ref BLUE: Style = Style::default()
        .fg(Color::LightBlue)
        .modifier(Modifier::BOLD);
    /// Yellow color.
    pub static ref YELLOW: Style = Style::default()
        .fg(Color::LightYellow)
        .modifier(Modifier::BOLD);
    /// Orange color.
    pub static ref ORANGE: Style = Style::default()
        .fg(Color::Rgb(255, 165, 0))
        .modifier(Modifier::BOLD);
    /// Bold.
    pub static ref BOLD: Style = Style::default()
        .modifier(Modifier::BOLD);
}

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
                                send_ctrl_c();
                                return;
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

/// Get the rect of the inner rect of a block with the borders.
pub fn inner_block(rect: Rect) -> Rect {
    if rect.width < 2 || rect.height < 2 {
        return Rect::new(rect.x + 1, rect.y + 1, 0, 0);
    }
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

/// Draw the compilation block.
pub(crate) fn draw_compilations<'a, I>(
    frame: &mut FrameType,
    rect: Rect,
    compilations: I,
    loading: char,
) where
    I: Iterator<Item = (&'a PathBuf, &'a CompilationStatus)>,
{
    let compilations: Vec<_> = compilations.collect();
    let max_len = compilations
        .iter()
        .map(|(k, _)| k.file_name().expect("Invalid file name").len())
        .max()
        .unwrap_or(0)
        + 4;
    let text: Vec<Text> = compilations
        .iter()
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
pub(crate) fn compilation_status_text(status: &CompilationStatus, loading: char) -> Text<'static> {
    match status {
        CompilationStatus::Pending => Text::raw("... "),
        CompilationStatus::Running => Text::raw(format!("{}   ", loading)),
        CompilationStatus::Done { .. } => Text::styled("OK  ", *GREEN),
        CompilationStatus::Failed { .. } => Text::styled("FAIL", *RED),
        CompilationStatus::Skipped => Text::styled("skip", *YELLOW),
    }
}

/// Render a block with the specified title.
pub fn render_block<S: AsRef<str>>(frame: &mut FrameType, rect: Rect, title: S) {
    Block::default()
        .title(title.as_ref())
        .title_style(*BLUE)
        .borders(Borders::ALL)
        .render(frame, rect);
}

/// Draw the server status block.
pub fn render_server_status(
    frame: &mut FrameType,
    rect: Rect,
    status: Option<&ExecutorStatus<SystemTime>>,
    loading: char,
    frame_index: usize,
) {
    let title = " Server status ";
    render_block(frame, rect, title);
    draw_server_status_summary(
        frame,
        Rect::new(
            rect.x + title.len() as u16 + 2,
            rect.y,
            rect.width.saturating_sub(title.len() as u16 + 2),
            1,
        ),
        status,
    );
    draw_server_status(
        frame,
        inner_block(rect),
        status,
        loading,
        frame_index / FPS as usize / ROTATION_DELAY as usize,
    );
}

/// Draw the summary of the server status on the border of the block.
fn draw_server_status_summary(
    frame: &mut FrameType,
    rect: Rect,
    status: Option<&ExecutorStatus<SystemTime>>,
) {
    let status = if let Some(status) = status {
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
fn draw_server_status(
    frame: &mut FrameType,
    rect: Rect,
    status: Option<&ExecutorStatus<SystemTime>>,
    loading: char,
    mut rotation_index: usize,
) {
    let status = if let Some(status) = status {
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
    // avoid drawing if the screen is too small
    if rect.height == 0 {
        return;
    }
    let num_workers = workers.len();
    let chunks = workers
        .into_iter()
        .cycle()
        .skip(rotation_index)
        .take(num_workers)
        .chunks(rect.height as usize);
    for (rect, chunk) in rects.into_iter().zip(&chunks) {
        draw_workers_chunk(frame, rect, &chunk.collect_vec(), loading);
    }
}

/// Draw a chunk of workers in the specified rectangle.
fn draw_workers_chunk(
    frame: &mut FrameType,
    rect: Rect,
    workers: &[&ExecutorWorkerStatus<SystemTime>],
    loading: char,
) {
    let max_len = workers
        .iter()
        .map(|worker| worker.name.len())
        .max()
        .unwrap_or(0);
    let text: Vec<Text> = workers
        .iter()
        .flat_map(|worker| {
            let worker_name = format!("- {:<max_len$} ", worker.name, max_len = max_len);
            let worker_name_len = worker_name.len();
            let mut texts = vec![Text::raw(worker_name)];

            if let Some(job) = &worker.current_job {
                let duration =
                    (job.duration.elapsed().map(|d| d.as_millis()).unwrap_or(0) as f64) / 1000.0;
                let mut line = format!("{} {} ({:.2}s)", loading, job.job, duration);
                if worker_name_len + line.len() > rect.width as usize {
                    let extra_len = worker_name_len + line.len() - rect.width as usize;
                    // there is not enough space for the job name, even with the ellipsis
                    if extra_len + 3 > job.job.len() {
                        line = (&line[0..rect.width as usize]).into();
                    } else {
                        let job_name = &job.job[0..job.job.len() - extra_len - 3];
                        line = format!("{} {}... ({:.2}s)", loading, job_name, duration);
                    }
                }
                texts.push(Text::raw(line));
            }
            texts.push(Text::raw("\n"));
            texts
        })
        .collect();
    Paragraph::new(text.iter()).wrap(false).render(frame, rect);
}

/// Send to the current process `SIGINT`, letting it exit gracefully.
fn send_ctrl_c() {
    let pid = std::process::id();
    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT) {
        error!("Failed to send SIGINT to {}: {}", pid, e);
    }
}
