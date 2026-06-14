use std::{collections::BTreeSet, path::PathBuf};

mod app_data;
mod database;
mod tags_db;

use miette::{IntoDiagnostic, Report};

pub use crate::{
    app_data::{AppData, RecentFolder},
    database::{
        Buffer, Error as DatabaseError, FinalizeTransaction, Result as DatabaseResult, Table,
        Transaction, TransactionApi, TransactionHandle, TransactionImpl, TransactionResult,
        UntypedTable, initialize_transaction,
    },
    tags_db::{DatabaseState, Entry, Tag, TagsDatabase},
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
    pub tags: BTreeSet<Tag>,
    pub selected: bool,
}

impl EditEntry {
    pub fn selected_tags(&self) -> Option<&BTreeSet<Tag>> {
        self.selected.then_some(&self.tags)
    }

    pub fn selected_tags_mut(&mut self) -> Option<&mut BTreeSet<Tag>> {
        self.selected.then_some(&mut self.tags)
    }
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
    pub tag_search_bar_state: String,
    pub tag_create_name_state: String,
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

    pub fn set_entry_name(&mut self, index: usize, name: String) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.name = name;
        }
    }

    pub fn intersection_of_tags_of_selected_entries(&self) -> impl Iterator<Item = &Tag> {
        self.entries
            .iter()
            .filter_map(EditEntry::selected_tags)
            .min_by_key(|tags| tags.len())
            .into_iter()
            .flat_map(|smallest| {
                smallest.iter().filter(|tag| {
                    self.entries
                        .iter()
                        .filter_map(EditEntry::selected_tags)
                        .all(|tags| tags.contains(tag))
                })
            })
    }

    /// Returns if the tag was actually added to anything
    pub fn add_tag_to_selected(&mut self, tag: &Tag) -> bool {
        let mut any = false;
        for tags in self
            .entries
            .iter_mut()
            .filter_map(EditEntry::selected_tags_mut)
        {
            any |= tags.insert(tag.clone());
        }
        any
    }

    pub fn remove_tag_from_selected(&mut self, tag: &Tag) {
        for tags in self
            .entries
            .iter_mut()
            .filter_map(EditEntry::selected_tags_mut)
        {
            tags.remove(tag);
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
    ($this:ident, $result:expr $(,)?) => {{
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
            self.persistent.write_recent_folders().into_diagnostic(),
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

    pub fn edit_create_tag_entry(&mut self) {
        if self.edit.tag_create_name_state.is_empty() {
            return;
        }

        let id = self.database.generate_entry();
        let name = std::mem::take(&mut self.edit.tag_create_name_state);
        self.edit.entries.push(EditEntry {
            id,
            name,
            tags: BTreeSet::new(),
            selected: true,
        });
    }

    pub fn edit_try_add_searched_tag_to_selected(&mut self) {
        let tag = self.edit.tag_search_bar_state.as_str().into();
        let tag_exists = set_err!(self, self.database.tag_exists(&tag));
        if tag_exists && self.edit.add_tag_to_selected(&tag) {
            self.edit.tag_search_bar_state.clear();
        }
    }

    pub fn edit_tag_prefix_search_results(&mut self) -> impl Iterator<Item = Tag> {
        self.database
            .search_tags_names_by_prefix(&self.edit.tag_search_bar_state)
            .map_while(|result| match result {
                Ok(tag) => Some(tag),
                Err(e) => {
                    self.active_overlay = ActiveOverlay::Error(e);
                    None
                }
            })
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
