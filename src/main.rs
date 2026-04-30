use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem, core::lens, view::flex_col,
    winit::error::EventLoopError,
};

use crate::launcher::LauncherState;

mod launcher;

#[derive(Default)]
enum ActiveView {
    #[default]
    Launcher,
    SearchResults,
}

#[derive(Default)]
struct AppState {
    active_view: ActiveView,
    launcher: LauncherState,
}

fn app_logic(state: &mut AppState) -> Box<AnyWidgetView<AppState>> {
    use crate::launcher::launcher_view;
    use ActiveView::*;
    match state.active_view {
        Launcher => lens(launcher_view, |state: &mut AppState| &mut state.launcher).boxed(),
        SearchResults => flex_col(()).boxed(),
    }
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        AppState::default(),
        app_logic,
        WindowOptions::new("File Tagger"),
    );
    app.run_in(EventLoop::with_user_event())
}
