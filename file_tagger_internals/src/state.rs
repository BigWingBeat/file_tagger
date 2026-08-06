use std::{collections::BTreeSet, path::PathBuf};

use anymore::AnyDebug;
use miette::{IntoDiagnostic, Report};
use rfd::FileHandle;

use crate::{AppData, DatabaseState, Entry, Tag, tags_db::ActiveTransactionDatabaseState};

/// The state passed to `finalize_previous_state()` will have its dyn data set to `()`, as at that point
/// the dyn data has already been moved into the new state (the one returned by `next_state()`)
struct StateTransitionFunctions<From> {
    next_state: Box<dyn FnOnce(&mut From) -> ActiveView>,
    finalize_previous_state: Box<dyn FnOnce(ActiveView)>,
}

impl<From> StateTransitionFunctions<From> {
    fn new(
        next_state: impl FnOnce(&mut From) -> ActiveView + 'static,
        finalize_previous_state: impl FnOnce(ActiveView) + 'static,
    ) -> Self {
        Self {
            next_state: Box::new(next_state),
            finalize_previous_state: Box::new(finalize_previous_state),
        }
    }

    fn transition(self, from: &mut From) -> (ActiveView, Box<dyn FnOnce(ActiveView)>) {
        ((self.next_state)(from), self.finalize_previous_state)
    }
}

/// Generates wrapper types for data inside the enum because dealing with `{}`-style enum variants directly is annoying
/// (Would enum variants as types makes this nicer?)
macro_rules! app_state {
	( $name:ident = $( $(#[$meta:meta])* $variant:ident { $($v:vis $field:ident: $t:ty),* $(,)* } ),* $(,)* ) => {
	    $(
            pub struct $variant {
                next_state: Option<StateTransitionFunctions<Self>>,
                pub active_overlay: ActiveOverlay,
                pub data: Box<dyn AnyDebug>,
                $($v $field: $t),*
            }

            impl $variant {
                pub fn new<T: AnyDebug>(data: T, $($field: $t),*) -> Self {
                    let data = Box::new(data);
                    Self { next_state: None, active_overlay: ActiveOverlay::None, data, $($field),* }
                }
            }

            impl $variant {
                pub fn queue_next_state<To, Parameters>(&mut self, parameters: Parameters)
                where
                    (): StateTransition<Self, To, Parameters>,
                {
                    let transition_fns = <()>::make_transition_fns(parameters);
                    self.next_state = Some(transition_fns);
                }
            }

            // impl From<$variant<()>> for $variant {
            //     fn from(value: $variant<()>) -> Self {
            //         Self { next_state: None, active_overlay: ActiveOverlay::None, data: value.data, $($field: value.$field),* }
            //     }
            // }

            impl From<$variant> for $name {
                fn from(value: $variant) -> Self {
                    Self::$variant(value)
                }
            }
        )*

		pub enum $name {
			$( $(#[$meta])* $variant ($variant) ),*
		}

        // impl From<$name<()>> for $name {
        //     fn from(value: $name<()>) -> Self {
        //         match value {
        //             $( $name::$variant(inner) => Self::$variant(inner.into()) ),*
        //         }
        //     }
        // }

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

            pub fn data(&self) -> &dyn AnyDebug {
                match self {
                    $( $name::$variant(inner) => &*inner.data ),*
                }
            }

            pub fn data_mut(&mut self) -> &mut dyn AnyDebug {
                match self {
                    $( $name::$variant(inner) => &mut *inner.data ),*
                }
            }

            pub fn set_data<T: AnyDebug>(&mut self, data: T) -> Box<dyn AnyDebug> {
                self.set_data_dyn(Box::new(data))
            }

            pub fn set_data_dyn(&mut self, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
                match self {
                    $( $name::$variant(inner) => std::mem::replace(&mut inner.data, data) ),*
                }
            }
        }

        impl $name {
            /// Update self to the queued variant stored in `next_state`.
            /// The parameters for `map_data` are the current variant, the next variant, and the current dyn data.
            /// Both variants have their own dyn data fields set to `()` for the scope of `map_data`'s execution.
            pub fn update_to_next<F>(&mut self, map_data: F) -> bool
            where
                F: FnOnce(&Self, &Self, Box<dyn AnyDebug>) -> Box<dyn AnyDebug>
            {
                let transition = match self {
                    $( $name::$variant(inner) => inner.next_state.take().map(|transition| transition.transition(inner)) ),*
                };

                transition.is_some_and(|(mut next, finalize)| {
                    // This variant will be dropped when we assign to `*self` below, so clobbering its `data` here is fine
                    let old_data = self.set_data(());
                    let new_data = map_data(self, &next, old_data);
                    next.set_data_dyn(new_data);
                    let previous = std::mem::replace(self, next);
                    (finalize)(previous);
                    true
                })
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

/// Used by multiple states
#[derive(Clone, Default)]
pub struct SearchMenuState {
    pub search_bar: String,
    pub import_dialog_active: bool,
}

app_state! {
    ActiveView =

    /// Oh no
    UnrecoverableError {},

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
        pub search: SearchMenuState,
    },

    /// Grid of search results, plus a search bar, and button to go back to `SearchMenu`
    SearchResults {
        persistent: AppData,
        pub database: DatabaseState,
        pub search: SearchMenuState,
        entries: Vec<EditEntry>,
    },

    /// Edit tags of entries, and create new entries (tags) to use
    Edit {
        persistent: AppData,
        pub database: ActiveTransactionDatabaseState,
        entries: Vec<EditEntry>,
        pub tag_search_bar_state: String,
        pub tag_create_name_state: String,
    },
}

pub trait StateTransition<From, To, Parameters> {
    fn make_transition_fns(parameters: Parameters) -> StateTransitionFunctions<From>;
}

macro_rules! impl_state_transition {
    ($from:ident -> $to:ident, parameters: [ $($field:ident: $param:ty),* ], clones: [ $($clone:ident),* ], defaults: [ $($default:ident),* ], finalize: $($finalize:tt)*) => {
        impl StateTransition<$from, $to, ( $($param,)* )> for () {
            fn make_transition_fns(parameters: ( $($param,)* )) -> StateTransitionFunctions<$from> {
                StateTransitionFunctions::<$from>::new(
                    |from| {
                        let ( $($field,)* ) = parameters;
                        $to {
                            next_state: None,
                            active_overlay: ActiveOverlay::None,
                            data: Box::new(()),
                            $( $field, )*
                            $( $clone: from.$clone.clone().into(), )*
                            $( $default: Default::default(), )*
                        }.into()
                    },
                    $($finalize)*
                )
            }
        }
    };
}

impl_state_transition!(
    Launcher -> SearchMenu,
    parameters: [database: DatabaseState],
    clones: [persistent, dialog],
    defaults: [search],
    finalize: |_| {}
);

impl_state_transition!(
    SearchMenu -> SearchMenu,
    parameters: [database: DatabaseState],
    clones: [persistent, dialog],
    defaults: [search],
    finalize: |_| {}
);

impl_state_transition!(
    SearchMenu -> SearchResults,
    parameters: [],
    clones: [database, persistent, search],
    defaults: [entries],
    finalize: |_| {}
);

impl_state_transition!(
    SearchMenu -> Edit,
    parameters: [],
    clones: [database, persistent],
    defaults: [entries, tag_search_bar_state, tag_create_name_state],
    finalize: |_| {}
);

pub struct Commit;

impl_state_transition!(
    Edit -> SearchMenu,
    parameters: [__: Commit],
    clones: [database, persistent],
    defaults: [dialog, search],
    finalize: |state| { if let ActiveView::Edit(edit) = state { edit.database.commit(); } }
);

pub struct Rollback;

impl_state_transition!(
    Edit -> SearchMenu,
    parameters: [__: Rollback],
    clones: [database, persistent],
    defaults: [dialog, search],
    finalize: |state| { if let ActiveView::Edit(edit) = state { edit.database.rollback(); } }
);

impl_state_transition!(
    SearchResults -> SearchResults,
    parameters: [],
    clones: [database, persistent, search],
    defaults: [entries],
    finalize: |_| {}
);

impl_state_transition!(
    SearchResults -> Edit,
    parameters: [],
    clones: [database, persistent],
    defaults: [entries, tag_search_bar_state, tag_create_name_state],
    finalize: |_| {}
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

impl ActiveView {
    pub fn new<T: AnyDebug>(data: T) -> Self {
        match AppData::open() {
            Ok(persistent) => Launcher::new(data, persistent, Default::default()).into(),
            Err(e) => UnrecoverableError {
                next_state: None,
                active_overlay: ActiveOverlay::Error(e),
                data: Box::new(data),
            }
            .into(),
        }
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

pub trait LauncherState: 'static {
    fn persistent(&mut self) -> &mut AppData;

    fn start_open_dialog(&mut self);
    fn open_dialog_active(&self) -> bool;
    fn handle_open_dialog(&mut self, result: Option<FileHandle>);

    fn start_create_dialog(&mut self);
    fn create_dialog_active(&self) -> bool;
    fn handle_create_dialog(&mut self, result: Option<FileHandle>);

    fn open_database_in_folder(&mut self, folder: PathBuf);
    fn create_folder_with_database(&mut self, folder: PathBuf);
}

impl Launcher {
    fn open_database(&mut self, folder: PathBuf) {
        let folder = set_err!(
            self,
            self.persistent.push_recent_folder(folder).into_diagnostic()
        );
        let database = set_err!(self, DatabaseState::open_in_folder(folder.clone()));
        self.queue_next_state::<SearchMenu, _>((database,));
    }
}

impl LauncherState for Launcher {
    fn persistent(&mut self) -> &mut AppData {
        &mut self.persistent
    }

    fn start_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.open_dialog_active = true;
    }

    fn open_dialog_active(&self) -> bool {
        self.dialog.open_dialog_active
    }

    fn handle_open_dialog(&mut self, result: Option<FileHandle>) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.open_dialog_active = false;
        if let Some(folder) = result {
            self.open_database_in_folder(folder.into());
        }
    }

    fn start_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.create_dialog_active = true;
    }

    fn create_dialog_active(&self) -> bool {
        self.dialog.create_dialog_active
    }

    fn handle_create_dialog(&mut self, result: Option<FileHandle>) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.create_dialog_active = false;
        if let Some(folder) = result {
            self.create_folder_with_database(folder.into());
        }
    }

    /// The user picks a folder, and a database is created or opened in that folder.
    /// Queues the [`SearchMenu` ]state.
    fn open_database_in_folder(&mut self, folder: PathBuf) {
        self.open_database(folder);
    }

    /// The user is presented with a "save file dialog", and a new folder, plus a database in that folder, are created accordingly
    /// Queues the [`SearchMenu`] state.
    fn create_folder_with_database(&mut self, folder: PathBuf) {
        std::fs::create_dir(&folder).unwrap();
        self.open_database(folder);
    }
}

impl SearchMenu {
    fn open_database(&mut self, folder: PathBuf) {
        if self.database.active_folder().path == folder {
            return;
        }

        let folder = set_err!(
            self,
            self.persistent.push_recent_folder(folder).into_diagnostic()
        );
        let database = set_err!(self, DatabaseState::open_in_folder(folder.clone()));
        self.queue_next_state::<SearchMenu, _>((database,));
    }
}

impl LauncherState for SearchMenu {
    fn persistent(&mut self) -> &mut AppData {
        &mut self.persistent
    }

    fn start_open_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.open_dialog_active = true;
    }

    fn open_dialog_active(&self) -> bool {
        self.dialog.open_dialog_active
    }

    fn handle_open_dialog(&mut self, result: Option<FileHandle>) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.open_dialog_active = false;
        if let Some(folder) = result {
            self.open_database_in_folder(folder.into());
        }
    }

    fn start_create_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.dialog.create_dialog_active = true;
    }

    fn create_dialog_active(&self) -> bool {
        self.dialog.create_dialog_active
    }

    fn handle_create_dialog(&mut self, result: Option<FileHandle>) {
        self.active_overlay = ActiveOverlay::None;
        self.dialog.create_dialog_active = false;
        if let Some(folder) = result {
            self.create_folder_with_database(folder.into());
        }
    }

    /// The user picks a folder, and a database is created or opened in that folder
    /// Queues the [`SearchMenu`] state.
    fn open_database_in_folder(&mut self, folder: PathBuf) {
        self.open_database(folder);
    }

    /// The user is presented with a "save file dialog", and a new folder, plus a database in that folder, are created accordingly
    /// Queues the [`SearchMenu`] state.
    fn create_folder_with_database(&mut self, folder: PathBuf) {
        std::fs::create_dir(&folder).unwrap();
        self.open_database(folder);
    }
}

pub trait SearchState: 'static {
    fn search_bar(&mut self) -> &mut String;

    fn start_import_dialog(&mut self);
    fn import_dialog_active(&self) -> bool;
    fn handle_import_dialog(&mut self, result: Option<Vec<FileHandle>>);

    fn search_results(&mut self);
    fn edit_entries(&mut self);
    fn import_files(&mut self, files: &[PathBuf]);
}

impl SearchState for SearchMenu {
    fn search_bar(&mut self) -> &mut String {
        &mut self.search.search_bar
    }

    fn start_import_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.search.import_dialog_active = true;
    }

    fn import_dialog_active(&self) -> bool {
        self.search.import_dialog_active
    }

    fn handle_import_dialog(&mut self, result: Option<Vec<FileHandle>>) {
        self.active_overlay = ActiveOverlay::None;
        self.search.import_dialog_active = false;
        if let Some(files) = result
            && !files.is_empty()
        {
            self.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
        }
    }

    /// Queues the [`SearchResults`] state.
    fn search_results(&mut self) {
        self.queue_next_state::<SearchResults, _>(());
    }

    /// Queues the [`Edit`] state.
    fn edit_entries(&mut self) {
        self.queue_next_state::<Edit, _>(());
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    /// Queues the [`Edit`] state.
    fn import_files(&mut self, files: &[PathBuf]) {
        self.queue_next_state::<Edit, _>(());
    }
}

impl SearchResults {
    pub fn entries(&self) -> &[EditEntry] {
        &self.entries
    }
}

impl SearchState for SearchResults {
    fn search_bar(&mut self) -> &mut String {
        &mut self.search.search_bar
    }

    fn start_import_dialog(&mut self) {
        self.active_overlay = ActiveOverlay::Spinner;
        self.search.import_dialog_active = true;
    }

    fn import_dialog_active(&self) -> bool {
        self.search.import_dialog_active
    }

    fn handle_import_dialog(&mut self, result: Option<Vec<FileHandle>>) {
        self.active_overlay = ActiveOverlay::None;
        self.search.import_dialog_active = false;
        if let Some(files) = result
            && !files.is_empty()
        {
            self.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
        }
    }

    /// Queues the [`SearchResults`] state.
    fn search_results(&mut self) {
        self.queue_next_state::<SearchResults, _>(());
    }

    /// Queues the [`Edit`] state.
    fn edit_entries(&mut self) {
        self.queue_next_state::<Edit, _>(());
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    /// Queues the [`Edit`] state.
    fn import_files(&mut self, files: &[PathBuf]) {
        self.queue_next_state::<Edit, _>(());
    }
}

impl Edit {
    /// Commits the transaction.
    /// Queues the [`SearchMenu`] state.
    pub fn save_changes(&mut self) {
        self.queue_next_state::<SearchMenu, _>(Commit);
    }

    /// Rolls back the transaction.
    /// Queues the [`SearchMenu`] state.
    pub fn cancel(&mut self) {
        self.queue_next_state::<SearchMenu, _>(Rollback);
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
        // The std `intersection` methods are only implemented for pairs of collections, but we have arbitrarily many
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
