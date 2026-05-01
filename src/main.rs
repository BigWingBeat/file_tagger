use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem, view::flex_col,
    winit::error::EventLoopError,
};

use crate::{
    launcher::LauncherState,
    search_menu::SearchMenuState,
    view::{launcher_view, search_menu_view},
};

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
    launcher: LauncherState,
}

impl AppState {
    fn search_menu(&mut self, active_folder: String) {
        self.active_view = ActiveView::SearchMenu;
        self.search_menu.active_folder = active_folder;
    }
}

fn app_logic(state: &mut AppState) -> Box<AnyWidgetView<AppState>> {
    match state.active_view {
        ActiveView::Launcher => launcher_view(state).boxed(),
        ActiveView::SearchMenu => search_menu_view(state).boxed(),
        ActiveView::SearchResults => flex_col(()).boxed(),
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
