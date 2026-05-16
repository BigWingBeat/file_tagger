use std::path::PathBuf;

mod app_data;
mod database;
mod tags_db;

pub use crate::{
    app_data::{AppData, RecentFolder},
    tags_db::{DatabaseState, Entry},
};

const FOLDER_NAME: &str = ".file_tagger";

#[derive(Default)]
pub enum ActiveView {
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

#[derive(Default)]
pub struct SearchBarState {
    pub search_text: String,
}

pub struct EditEntry {
    pub id: Entry,
    pub name: String,
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
}

pub struct AppState {
    pub active_view: ActiveView,
    pub search_menu: SearchBarState,
    pub search_results: SearchResultsState,
    pub database: DatabaseState,
    pub persistent: AppData,
    pub edit: EditState,
}

impl AppState {
    pub fn open_database_in_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open Database As Folder")
            .pick_folder()
        {
            self.search_menu(folder);
        }
    }

    pub fn create_folder_with_database(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Create New Folder And Database")
            .save_file()
        {
            std::fs::create_dir(&folder).unwrap();
            self.search_menu(folder);
        }
    }

    pub fn open_recent(&mut self, folder: PathBuf) {
        self.search_menu(folder);
    }

    pub fn search_menu(&mut self, folder: PathBuf) {
        self.active_view = ActiveView::SearchMenu;
        let folder = RecentFolder::from(folder);
        self.database.open_in_folder(folder.clone()).unwrap();
        self.persistent.recent_folders.push(folder);
        self.persistent.write_recent_folders().unwrap();
    }

    pub fn search_results(&mut self) {
        self.active_view = ActiveView::SearchResults;
    }

    pub fn import_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Select Files to Import")
            .pick_files()
        {
            self.active_view = ActiveView::Edit;
            // self.edit.entries = paths;
        }
    }

    pub fn edit_tags(&mut self) {
        self.active_view = ActiveView::Edit;
    }

    pub fn generate_entry(&mut self) {
        let id = self.database.generate_entry();
        self.edit.entries.push(EditEntry {
            id,
            name: String::new(),
        });
    }
}

impl AppState {
    pub fn new() -> miette::Result<Self> {
        let database = DatabaseState::create_temporary()?;
        let persistent = AppData::open()?;
        Ok(Self {
            active_view: Default::default(),
            search_menu: Default::default(),
            search_results: Default::default(),
            database,
            persistent,
            edit: Default::default(),
        })
    }
}
