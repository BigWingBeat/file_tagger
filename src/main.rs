use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem, core::lens, view::flex_col,
    winit::error::EventLoopError,
};

use crate::{launcher::launcher_view, search_menu::SearchMenuState};

mod launcher;
mod search_menu;
mod view;

#[derive(Default)]
enum ActiveView {
    /// No database is open. Buttons for opening/creating a database
    #[default]
    Launcher,
    /// A database is open. Buttons for opening/creating a database, plus a search bar.
    /// Automatically open previously opened database to this view on startup, if possible
    SearchMenu,
    /// Grid of search results, plus a search bar, and button to go back to `SearchMenu`
    SearchResults,
}

#[derive(Default)]
struct AppState {
    active_view: ActiveView,
    search_menu: SearchMenuState,
}

fn app_logic(state: &mut AppState) -> Box<AnyWidgetView<AppState>> {
    use crate::search_menu::search_menu_view;
    use ActiveView::*;
    match state.active_view {
        Launcher => lens(launcher_view, |state: &mut AppState| {
            &mut state.search_menu.launcher_state
        })
        .boxed(),
        SearchMenu => lens(search_menu_view, |state: &mut AppState| {
            &mut state.search_menu
        })
        .boxed(),
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
