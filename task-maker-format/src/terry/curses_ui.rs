use crate::terry::finish_ui::FinishUI;
use crate::terry::ui_state::UIState;
use crate::ui::curses::{CursesDrawer, CursesUI as GenericCursesUI, FrameType};

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
fn draw_frame(_state: &UIState, _frame: FrameType, _loading: char, _frame_index: usize) {
    unimplemented!()
}
