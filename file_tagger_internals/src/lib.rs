use std::path::PathBuf;

mod app_data;
mod database;
mod tags_db;

use miette::{IntoDiagnostic, Report};

pub use crate::{
    app_data::{AppData, RecentFolder},
    tags_db::{DatabaseState, Entry},
};

const FOLDER_NAME: &str = ".file_tagger";

#[derive(Default, Clone, Copy)]
pub enum ActiveView {
    /// No database is open. Buttons for opening/creating a database
    #[default]
    Launcher,
    /// A database is open. Buttons for opening/creating a database, plus a search bar.
    /// Automatically open previously opened database to this view on startup, if possible
    SearchMenu,
    /// Grid of search results, plus a search bar, and button to go back to `SearchMenu`
    SearchResults,
    /// Edit tags of entries, and create new entries (tags) to use
    Edit,
}

#[derive(Default)]
pub enum ActiveOverlay {
    /// No overlay is being displayed
    #[default]
    None,
    /// An error is being displayed
    Error(Report),
    /// Waiting for something to happen on another thread (e.g. async)
    Spinner,
}

#[derive(Default)]
pub struct SearchBarState {
    pub search_text: String,
}

pub struct EditEntry {
    pub id: Entry,
    pub name: String,
    pub selected: bool,
}

#[derive(Default)]
pub struct SearchResultsState {
    entries: Vec<EditEntry>,
}

impl SearchResultsState {
    pub fn entries(&self) -> &[EditEntry] {
        &self.entries
    }
}

#[derive(Default)]
pub struct EditState {
    entries: Vec<EditEntry>,
}

impl EditState {
    pub fn entries(&self) -> &[EditEntry] {
        &self.entries
    }

    pub fn clear_entries(&mut self) {
        self.entries.clear();
    }

    pub fn toggle_entry_selected(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.selected = !entry.selected;
        }
    }
}

pub struct AppState {
    pub active_view: ActiveView,
    pub active_overlay: ActiveOverlay,
    pub search_menu: SearchBarState,
    pub search_results: SearchResultsState,
    pub database: DatabaseState,
    pub persistent: AppData,
    pub edit: EditState,
}

macro_rules! set_err {
    ($this:ident, $result:expr) => {{
        let result: Result<_, Report> = $result;
        match result {
            Ok(ok) => ok,
            Err(e) => {
                $this.active_overlay = ActiveOverlay::Error(e);
                return;
            }
        }
    }};
}

impl AppState {
    /// The user picks a folder, and a database is created or opened in that folder
    pub fn open_database_in_folder(&mut self, folder: PathBuf) {
        self.open_database(folder);
    }

    /// The user is presented with a "save file dialog", and a new folder, plus a database in that folder, are created accordingly
    pub fn create_folder_with_database(&mut self, folder: PathBuf) {
        std::fs::create_dir(&folder).unwrap();
        self.open_database(folder);
    }

    fn open_database(&mut self, folder: PathBuf) {
        if self.database.active_folder().path == folder {
            return;
        }

        let folder = self.persistent.push_recent_folder(folder);
        set_err!(self, self.database.open_in_folder(folder.clone()));
        set_err!(
            self,
            self.persistent.write_recent_folders().into_diagnostic()
        );
        self.active_view = ActiveView::SearchMenu;
    }

    pub fn search_menu(&mut self) {
        self.active_view = ActiveView::SearchMenu;
    }

    pub fn search_results(&mut self) {
        self.active_view = ActiveView::SearchResults;
    }

    pub fn edit_entries(&mut self) {
        self.active_view = ActiveView::Edit;
        self.edit.clear_entries();
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    pub fn import_files(&mut self, files: &[PathBuf]) {
        self.active_view = ActiveView::Edit;
        self.edit.clear_entries();
        // self.edit.entries = paths;
    }

    pub fn generate_entry(&mut self) {
        let id = self.database.generate_entry();
        self.edit.entries.push(EditEntry {
            id,
            name: String::new(),
            selected: true,
        });
    }
}

impl AppState {
    pub fn new() -> miette::Result<Self> {
        let database = DatabaseState::create_temporary()?;
        let persistent = AppData::open()?;
        Ok(Self {
            active_view: Default::default(),
            active_overlay: Default::default(),
            search_menu: Default::default(),
            search_results: Default::default(),
            database,
            persistent,
            edit: Default::default(),
        })
    }
}
