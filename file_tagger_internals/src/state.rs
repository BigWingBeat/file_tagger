use std::{collections::BTreeSet, path::PathBuf};

use miette::{IntoDiagnostic, Report};

use crate::{AppData, DatabaseState, Entry, Tag};

/// Generates wrapper types for data inside the enum because dealing with `{}`-style enum variants directly is annoying
/// (Would enum variants as types makes this nicer?)
macro_rules! app_state {
	( $name:ident = $( $(#[$meta:meta])* $variant:ident { $($v:vis $field:ident: $t:ty),* $(,)* } ),* $(,)* ) => {
	    $(
            pub struct $variant<Next = $name<()>> { next_state: Option<Next>, pub active_overlay: ActiveOverlay, $($v $field: $t),* }

            impl<Next> $variant<Next> {
                pub fn new($($field: $t),*) -> Self {
                    Self { next_state: None, active_overlay: ActiveOverlay::None, $($field),* }
                }

                pub fn queue_next_state<T>(&mut self, parameters: <Self as StateTransition<T>>::Parameters)
                where
                    Self: StateTransition<T>,
                    <Self as StateTransition<T>>::Next: Into<Next>,
                {
                    self.next_state = Some(self.to_state(parameters).into())
                }
            }

            impl From<$variant<()>> for $variant {
                fn from(_value: $variant<()>) -> Self {
                    Self { next_state: None, active_overlay: ActiveOverlay::None, $($field: _value.$field),* }
                }
            }

            impl<Next> From<$variant<Next>> for $name<Next> {
                fn from(value: $variant<Next>) -> Self {
                    Self::$variant(value)
                }
            }
        )*

		pub enum $name<Next = $name<()>> {
			$( $(#[$meta])* $variant ($variant<Next>) ),*
		}

        impl From<$name<()>> for $name {
            fn from(value: $name<()>) -> Self {
                match value {
                    $( $name::$variant(inner) => Self::$variant(inner.into()) ),*
                }
            }
        }

        impl $name {
            pub fn active_overlay(&self) -> &ActiveOverlay {
                match &self {
                    $( $name::$variant(inner) => &inner.active_overlay ),*
                }
            }

            pub fn set_active_overlay(&mut self, overlay: ActiveOverlay) {
                match self {
                    $( $name::$variant(inner) => inner.active_overlay = overlay ),*
                }
            }

            pub fn update_to_next(&mut self) {
                let next = match self {
                    $( $name::$variant(inner) => inner.next_state.take() ),*
                };

                if let Some(next) = next {
                    *self = next.into();
                }
            }
        }
	};
}

/// Used by multiple states
#[derive(Clone, Default)]
pub struct LauncherDialog {
    pub open_dialog_active: bool,
    pub create_dialog_active: bool,
}

app_state! {
    ActiveView =

    /// Startup view while the persistent AppData database is being opened
    Loading {},

    /// No database is open. Buttons for opening/creating a database
    Launcher {
        pub persistent: AppData,
        pub dialog: LauncherDialog,
    },
    /// A database is open. Buttons for opening/creating a database, plus a search bar.
    /// Automatically open previously opened database to this view on startup, if possible
    SearchMenu {
        pub persistent: AppData,
        pub database: DatabaseState,
        pub dialog: LauncherDialog,
        pub search_bar: String,
        pub import_dialog_active: bool,
    },
    /// Grid of search results, plus a search bar, and button to go back to `SearchMenu`
    SearchResults {
        pub database: DatabaseState,
        persistent: AppData,
        entries: Vec<EditEntry>,
    },
    /// Edit tags of entries, and create new entries (tags) to use
    Edit {
        pub database: DatabaseState,
        persistent: AppData,
        entries: Vec<EditEntry>,
        pub tag_search_bar_state: String,
        pub tag_create_name_state: String,
    },
}

pub trait StateTransition<T> {
    type Parameters;
    type Next;
    fn to_state(&self, parameters: Self::Parameters) -> Self::Next;
}

macro_rules! impl_state_transition {
    ($from:ident, $to:ident, parameters: [ $($field:ident: $param:ty),* ], clones: [ $($clone:ident),* ], defaults: [ $($default:ident),* ]) => {
        impl<Next, T> StateTransition<$to<T>> for $from<Next> {
            type Parameters = ( $($param,)* );
            type Next = $to<()>;
            fn to_state(&self, ( $($field,)* ): ( $($param,)* )) -> Self::Next {
                $to {
                    next_state: None,
                    active_overlay: ActiveOverlay::None,
                    $( $field, )*
                    $( $clone: self.$clone.clone(), )*
                    $( $default: Default::default(), )*
                }
            }
        }
    };
}

impl_state_transition!(
    Launcher,
    SearchMenu,
    parameters: [database: DatabaseState],
    clones: [persistent, dialog],
    defaults: [search_bar, import_dialog_active]
);

impl_state_transition!(
    SearchMenu,
    SearchMenu,
    parameters: [database: DatabaseState],
    clones: [persistent, dialog],
    defaults: [search_bar, import_dialog_active]
);

impl_state_transition!(
    SearchMenu,
    SearchResults,
    parameters: [],
    clones: [database, persistent],
    defaults: [entries]
);

impl_state_transition!(
    SearchMenu,
    Edit,
    parameters: [],
    clones: [database, persistent],
    defaults: [entries, tag_search_bar_state, tag_create_name_state]
);

impl_state_transition!(
    Edit,
    SearchMenu,
    parameters: [],
    clones: [database, persistent],
    defaults: [dialog, search_bar, import_dialog_active]
);

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

pub struct AppState {
    pub active_view: ActiveView,
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
    pub fn new() -> Self {
        match AppData::open() {
            Ok(persistent) => Self {
                active_view: Loading {
                    next_state: Some(Launcher::new(persistent, Default::default()).into()),
                    active_overlay: ActiveOverlay::None,
                }
                .into(),
            },
            Err(e) => Self {
                active_view: Loading {
                    next_state: None,
                    active_overlay: ActiveOverlay::Error(e),
                }
                .into(),
            },
        }
    }

    pub fn active_overlay(&self) -> &ActiveOverlay {
        self.active_view.active_overlay()
    }

    pub fn set_active_overlay(&mut self, overlay: ActiveOverlay) {
        self.active_view.set_active_overlay(overlay);
    }
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

impl Launcher {
    /// The user picks a folder, and a database is created or opened in that folder.
    /// Queues the [`SearchMenu` ]state.
    pub fn open_database_in_folder(&mut self, folder: PathBuf) {
        self.open_database(folder);
    }

    /// The user is presented with a "save file dialog", and a new folder, plus a database in that folder, are created accordingly
    /// Queues the [`SearchMenu`] state.
    pub fn create_folder_with_database(&mut self, folder: PathBuf) {
        std::fs::create_dir(&folder).unwrap();
        self.open_database(folder);
    }

    fn open_database(&mut self, folder: PathBuf) {
        let folder = set_err!(
            self,
            self.persistent.push_recent_folder(folder).into_diagnostic()
        );
        let database = set_err!(self, DatabaseState::open_in_folder(folder.clone()));
        self.queue_next_state::<SearchMenu>((database,));
    }

    pub fn start_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.open_dialog_active = true;
    }

    pub fn stop_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.open_dialog_active = false;
    }

    pub fn start_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.create_dialog_active = true;
    }

    pub fn stop_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.create_dialog_active = false;
    }
}

impl SearchMenu {
    /// The user picks a folder, and a database is created or opened in that folder
    /// Queues the [`SearchMenu`] state.
    pub fn open_database_in_folder(&mut self, folder: PathBuf) {
        self.open_database(folder);
    }

    /// The user is presented with a "save file dialog", and a new folder, plus a database in that folder, are created accordingly
    /// Queues the [`SearchMenu`] state.
    pub fn create_folder_with_database(&mut self, folder: PathBuf) {
        std::fs::create_dir(&folder).unwrap();
        self.open_database(folder);
    }

    fn open_database(&mut self, folder: PathBuf) {
        if self.database.active_folder().path == folder {
            return;
        }

        let folder = set_err!(
            self,
            self.persistent.push_recent_folder(folder).into_diagnostic()
        );
        let database = set_err!(self, DatabaseState::open_in_folder(folder.clone()));
        self.queue_next_state::<SearchMenu>((database,));
    }

    pub fn start_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.open_dialog_active = true;
    }

    pub fn stop_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.open_dialog_active = false;
    }

    pub fn start_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.create_dialog_active = true;
    }

    pub fn stop_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.create_dialog_active = false;
    }

    pub fn start_import_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.import_dialog_active = true;
    }

    pub fn import_dialog_active(&self) -> bool {
        self.import_dialog_active
    }

    pub fn stop_import_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.import_dialog_active = false;
    }

    /// Queues the [`SearchResults`] state.
    pub fn search_results(&mut self) {
        self.queue_next_state::<SearchResults>(());
    }

    /// Queues the [`Edit`] state.
    pub fn edit_entries(&mut self) {
        self.queue_next_state::<Edit>(());
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    /// Queues the [`Edit`] state.
    pub fn import_files(&mut self, files: &[PathBuf]) {
        self.queue_next_state::<Edit>(());
    }
}

impl SearchResults {
    pub fn entries(&self) -> &[EditEntry] {
        &self.entries
    }
}

impl Edit {
    /// Queues the [`SearchMenu`] state.
    pub fn search_menu(&mut self) {
        self.queue_next_state::<SearchMenu>(());
    }

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

    pub fn create_tag_entry(&mut self) {
        if self.tag_create_name_state.is_empty() {
            return;
        }

        let id = self.database.generate_entry();
        let name = std::mem::take(&mut self.tag_create_name_state);
        self.entries.push(EditEntry {
            id,
            name,
            tags: BTreeSet::new(),
            selected: true,
        });
    }

    pub fn try_add_searched_tag_to_selected(&mut self) {
        let tag = self.tag_search_bar_state.as_str().into();
        let tag_exists = set_err!(self, self.database.tag_exists(&tag));
        if tag_exists && self.add_tag_to_selected(&tag) {
            self.tag_search_bar_state.clear();
        }
    }

    pub fn tag_prefix_search_results(&mut self) -> impl Iterator<Item = Tag> {
        self.database
            .search_tags_names_by_prefix(&self.tag_search_bar_state)
            .map_while(|result| match result {
                Ok(tag) => Some(tag),
                Err(e) => {
                    self.active_overlay = ActiveOverlay::Error(e);
                    None
                }
            })
    }
}
