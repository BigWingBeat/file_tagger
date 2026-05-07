use std::path::PathBuf;

use xilem::{
    AnyWidgetView, EventLoop, FontWeight, WidgetView, WindowOptions, Xilem,
    masonry::theme::ZYNC_900,
    palette::css::RED,
    style::Style,
    view::{flex_col, label},
    winit::error::EventLoopError,
};

use crate::{
    database::DatabaseState,
    edit::EditState,
    launcher::LauncherState,
    search_menu::SearchMenuState,
    search_results::SearchResultsState,
    view::{launcher_view, search_menu_view, search_results_view},
};

mod database;
mod edit;
mod launcher;
mod search_menu;
mod search_results;
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
    /// Edit tags of entries
    Edit,
}

struct AppState {
    active_view: ActiveView,
    search_menu: SearchMenuState,
    launcher: LauncherState,
    search_results: SearchResultsState,
    database: DatabaseState,
    edit: EditState,
}

impl AppState {
    fn open_database_in_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open Database As Folder")
            .pick_folder()
        {
            self.database.open(&folder).unwrap();
            self.search_menu(folder);
        }
    }

    fn create_folder_with_database(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Create New Folder And Database")
            .save_file()
        {
            self.database.open(&folder).unwrap();
            self.search_menu(folder);
        }
    }

    fn open_recent(&mut self, folder: PathBuf) {
        self.database.open(&folder).unwrap();
        self.search_menu(folder);
    }

    fn search_menu(&mut self, folder: PathBuf) {
        self.active_view = ActiveView::SearchMenu;
        let name = folder.file_name().unwrap().to_string_lossy().into_owned();
        self.search_menu.active_folder = name.clone();
        self.launcher.push_recent_folder(name, folder);
    }

    fn search_results(&mut self) {
        self.active_view = ActiveView::SearchResults;
    }

    fn import_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Select Files to Import")
            .pick_files()
        {
            self.active_view = ActiveView::Edit;
            // self.edit.entries = paths;
        }
    }

    fn edit_tags(&mut self) {
        self.active_view = ActiveView::Edit;
    }
}

impl AppState {
    fn new() -> miette::Result<Self> {
        DatabaseState::create_temporary().map(|database| Self {
            active_view: Default::default(),
            search_menu: Default::default(),
            launcher: Default::default(),
            search_results: Default::default(),
            database,
            edit: Default::default(),
        })
    }

    fn app_logic(&mut self) -> Box<AnyWidgetView<Self>> {
        match self.active_view {
            ActiveView::Launcher => launcher_view(self).boxed(),
            ActiveView::SearchMenu => search_menu_view(self).boxed(),
            ActiveView::SearchResults => search_results_view(self).boxed(),
            ActiveView::Edit => todo!(),
        }
    }

    fn run_app(self) -> Result<(), EventLoopError> {
        Xilem::new_simple(self, Self::app_logic, WindowOptions::new("File Tagger"))
            .run_in(EventLoop::with_user_event())
    }
}

struct FuckedState(miette::Report);

impl FuckedState {
    fn app_logic(&mut self) -> impl WidgetView<Self> + use<> {
        flex_col(
            label(format!("{}", self.0))
                .weight(FontWeight::EXTRA_BLACK)
                .color(RED),
        )
        .main_axis_alignment(xilem::view::MainAxisAlignment::Center)
        .background_color(ZYNC_900)
    }

    fn run_app(error: miette::Report) -> Result<(), EventLoopError> {
        Xilem::new_simple(
            Self(error),
            Self::app_logic,
            WindowOptions::new("File Tagger"),
        )
        .run_in(EventLoop::with_user_event())
    }
}

fn main() -> Result<(), EventLoopError> {
    AppState::new().map_or_else(FuckedState::run_app, AppState::run_app)
}
