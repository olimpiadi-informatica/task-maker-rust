use crate::tools::find_bad_case::state::UIState;

pub struct FinishUI;

impl task_maker_format::ui::FinishUI<UIState> for FinishUI {
    fn print(_state: &UIState) {}
}
