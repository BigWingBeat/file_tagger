use std::path::PathBuf;

use miette::{IntoDiagnostic, MietteHandlerOpts};
use xilem::{
    AnyWidgetView, EventLoop, FontWeight, WidgetView, WindowOptions, Xilem,
    masonry::{parley::GenericFamily, theme::ZYNC_900},
    palette::css::RED,
    style::Style,
    view::{flex_col, label},
    winit::error::EventLoopError,
};

use crate::{
    database::DatabaseState,
    edit::EditState,
    launcher::LauncherState,
    persistent_data::{PersistentData, RecentFolder},
    search_menu::SearchMenuState,
    search_results::SearchResultsState,
    view::{launcher_view, search_menu_view, search_results_view},
};

mod database;
mod edit;
mod launcher;
mod persistent_data;
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
    persistent: PersistentData,
    edit: EditState,
}

impl AppState {
    fn open_database_in_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open Database As Folder")
            .pick_folder()
        {
            self.database.open_in_folder(folder.clone()).unwrap();
            self.search_menu(folder);
        }
    }

    fn create_folder_with_database(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Create New Folder And Database")
            .save_file()
        {
            std::fs::create_dir(&folder).unwrap();
            self.database.open_in_folder(folder.clone()).unwrap();
            self.search_menu(folder);
        }
    }

    fn open_recent(&mut self, folder: PathBuf) {
        self.database.open_in_folder(folder.clone()).unwrap();
        self.search_menu(folder);
    }

    fn search_menu(&mut self, folder: PathBuf) {
        self.active_view = ActiveView::SearchMenu;
        let folder = RecentFolder::from(folder);
        self.search_menu.active_folder = folder.name.to_string_lossy().into_owned();
        self.launcher.push_recent_folder(folder);
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
        let database = DatabaseState::create_temporary()?;
        let persistent = PersistentData::open().into_diagnostic()?;
        let launcher = LauncherState::new(&persistent)?;
        Ok(Self {
            active_view: Default::default(),
            search_menu: Default::default(),
            launcher,
            search_results: Default::default(),
            database,
            persistent,
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
            label(format!("{:?}", self.0))
                .font(GenericFamily::Monospace)
                .weight(FontWeight::BOLD)
                .text_size(20.0)
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

fn set_error_handler() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            MietteHandlerOpts::new()
                .terminal_links(false)
                .width(80)
                .wrap_lines(true)
                .break_words(true)
                .with_cause_chain()
                // .without_cause_chain()
                .show_related_errors_as_nested()
                // .show_related_errors_as_siblings()
                .color(false)
                .unicode(true)
                .force_graphical(true)
                .context_lines(1)
                .build(),
        )
    }))
    .unwrap();
}

fn main() -> miette::Result<()> {
    set_error_handler();
    AppState::new()
        .map_or_else(FuckedState::run_app, AppState::run_app)
        .into_diagnostic()
}
