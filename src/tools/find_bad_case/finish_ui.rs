use crate::tools::find_bad_case::state::UIState;

/// This UI cannot do much because it is executed before the evaluation was completely done (e.g.
/// the file callbacks may not have completed yet). Instead, the actual printing is done in the main
/// of this tool.
pub struct FinishUI;

impl task_maker_format::ui::FinishUI<UIState> for FinishUI {
    fn print(_state: &UIState) {}
}
