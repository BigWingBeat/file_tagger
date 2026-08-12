use std::{collections::BTreeSet, path::PathBuf};

use anymore::AnyDebug;
use miette::{IntoDiagnostic, Report};
use rfd::FileHandle;

use crate::{AppData, DatabaseState, Entry, Tag, tags_db::ActiveTransactionDatabaseState};

/// Generates wrapper types for data inside the enum because dealing with `{}`-style enum variants directly is annoying
/// (Would enum variants as types makes this nicer?)
macro_rules! app_state {
	( $name:ident = $( $(#[$meta:meta])* $variant:ident { $($v:vis $field:ident: $t:ty),* $(,)* } ),* $(,)* ) => {
        // Repetition for each variant
	    $(
            // The common fields are duplicated across all variants instead of being pulled out to a wrapper struct because of
            // the limitations of Xilem's data lensing. This is the only way for these common fields to be available to views
            // that want to lens down to a specific state variant.
            pub struct $variant {
                next_state: Option<StateTransitionFunctions<Self>>,
                pub active_overlay: ActiveOverlay,
                pub data: Box<dyn AnyDebug>,
                $($v $field: $t),*
            }

            impl $variant {
                pub fn new($($field: $t),*) -> Self {
                    Self::new_with((), $($field),*)
                }

                pub fn new_with<T: AnyDebug>(data: T, $($field: $t),*) -> Self {
                    let data = Box::new(data);
                    Self { next_state: None, active_overlay: ActiveOverlay::None, data, $($field),* }
                }

                pub fn queue_next_state<To>(&mut self)
                where
                    Self: StateTransition<To, ()>,
                {
                    self.queue_next_state_with(());
                }

                pub fn queue_next_state_with<To, Parameters>(&mut self, parameters: Parameters)
                where
                    Self: StateTransition<To, Parameters>,
                {
                    let transition_fns = Self::make_transition_fns(parameters);
                    self.next_state = Some(transition_fns);
                }
            }

            impl From<$variant> for $name {
                fn from(value: $variant) -> Self {
                    Self::$variant(value)
                }
            }

            impl TryFrom<$name> for $variant {
                type Error = $name;

                fn try_from(state: $name) -> Result<Self, Self::Error> {
                    if let $name::$variant(inner) = state {
                        Ok(inner)
                    } else {
                        Err(state)
                    }
                }
            }
        )*

        // The outer enum that contains the above variants
		pub enum $name {
			$( $(#[$meta])* $variant ($variant) ),*
		}

        // Repetitions over each variant to concisely generate match blocks
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

            /// Update self to the queued variant stored in `next_state`.
            /// The parameters for `map_data` are the current variant, the next variant, and the current dyn data.
            /// Both variants have their own dyn data fields set to `()` for the scope of `map_data`'s execution.
            /// Returns whether or not anything actually happened.
            pub fn update_to_next<F>(&mut self, map_data: F) -> bool
            where
                F: FnOnce(&Self, &Self, Box<dyn AnyDebug>) -> Box<dyn AnyDebug>
            {
                let transition = match self {
                    $( $name::$variant(inner) => inner.next_state.take().map(|transition| transition.transition(inner)) ),*
                };

                transition.is_some_and(|(mut next, finalize)| {
                    // We clobber the dyn data of this variant here, before it's passed into the finalizer, but it's fine (see
                    // docs on `StateTransitionFunctions`)
                    let old_data = self.set_data(());
                    let new_data = map_data(self, &next, old_data);
                    next.set_data_dyn(new_data);
                    let previous = std::mem::replace(self, next);
                    if let Err(e) = (finalize)(previous) {
                        self.set_active_overlay(ActiveOverlay::Error(e));
                    }
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
    // Name of the outer enum
    ActiveView =

    // All of the variants

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

/// To get Clippy to shut up
impl Default for UnrecoverableError {
    fn default() -> Self {
        Self::new()
    }
}

/// State transitions work in a deferred manner: they are queued during a frame, then applied later.
/// This struct holds the functions that perform the state transition until they are actually used.
///
/// For some reason, `dyn FnOnce` seems to be an exception to the rule that trait objects can't move `self` by value.
/// We rely on this to allow consuming parameter values passed in at the call site where a state transition is queued,
/// by using closure variable capturing to magically store those parameters in the trait object, until they are consumed
/// by-value when the `dyn FnOnce` is called.
///
/// Regarding `update_to_next()`:
/// The state passed to `finalize_previous_state()` will have its dyn data set to `()`, as at that point
/// the dyn data has already been moved into the new state (the one returned by `next_state()`)
pub struct StateTransitionFunctions<From: ?Sized> {
    next_state: Box<dyn FnOnce(&mut From) -> ActiveView>,
    finalize_previous_state: Box<dyn FnOnce(ActiveView) -> Result<(), Report>>,
}

impl<From: ?Sized> StateTransitionFunctions<From> {
    fn new(
        next_state: impl FnOnce(&mut From) -> ActiveView + 'static,
        finalize_previous_state: impl FnOnce(ActiveView) -> Result<(), Report> + 'static,
    ) -> Self {
        Self {
            next_state: Box::new(next_state),
            finalize_previous_state: Box::new(finalize_previous_state),
        }
    }

    #[expect(clippy::type_complexity, reason = "shut up")]
    fn transition(
        self,
        from: &mut From,
    ) -> (
        ActiveView,
        Box<dyn FnOnce(ActiveView) -> Result<(), Report>>,
    ) {
        ((self.next_state)(from), self.finalize_previous_state)
    }
}

pub trait StateTransition<To, Parameters> {
    fn make_transition_fns(parameters: Parameters) -> StateTransitionFunctions<Self>;
}

macro_rules! impl_state_transition {
    // No params & no finalizer
    // Forwards to the following case ("No params") with an empty dummy closure for the finalizer (`|_| Ok(())`)
    (next_state: fn(&mut $from:ty) -> $to:ty = $next_state:expr $(,)*) => {
        impl_state_transition!(next_state: fn(&mut $from) -> $to = $next_state, finalize: fn($from) = |_| Ok(()));
    };
    // No params
    // Forwards to the "base" case with `()` for the params and a wrapper closure for `next_state` that transparently handles it
    (next_state: fn(&mut $from:ty) -> $to:ty = $next_state:expr, finalize: fn($from2:ty) = $finalize:expr $(,)*) => {
        impl_state_transition!(next_state: fn(&mut $from, ()) -> $to = |from, _| ($next_state as fn(&mut $from) -> $to)(from), finalize: fn($from) = $finalize);
    };
    // No finalizer
    // Forwards to the following ("base") case with an empty dummy closure for the finalizer (`|_| Ok(())`)
    (next_state: fn(&mut $from:ty, $params:ty) -> $to:ty = $next_state:expr $(,)*) => {
        impl_state_transition!(next_state: fn(&mut $from, $params) -> $to = $next_state, finalize: fn($from) = |_| Ok(()));
    };
    // "base" case, with params & a finalizer
    (next_state: fn(&mut $from:ty, $params:ty) -> $to:ty = $next_state:expr, finalize: fn($from2:ty) = $finalize:expr $(,)*) => {
        impl StateTransition<$to, $params> for $from {
            // This is called mid-frame when a state transition is queued
            fn make_transition_fns(parameters: $params) -> StateTransitionFunctions<Self> {
                StateTransitionFunctions::new(
                    // Captures the `parameters` variable so that it can be consumed later when the transition actually happens
                    move |from| {
                        // `.into()` here allows the closure to just directly return the inner variant type
                        ($next_state as fn(&mut Self, $params) -> $to)(from, parameters).into()
                    },
                    |state| {
                        let Ok(state) = state.try_into() else {
                            // We can't get rid of this `unreachable!()` due to how the call site (`update_to_next`) works.
                            // If we tried changing the outer parameter type from `ActiveView` to `$from`, we
                            // would just end up having to move this `let else + unreachable!()` to the call site (as well as
                            // make the call site more complicated in other ways too).
                            // The fundamental issue is that borrowck makes it impossible to write a version of `std::mem::replace`
                            // that looks like `fn<T>(&mut T, impl FnOnce(T) -> T)`, namely where the owned "dest" value is used
                            // to calculate the new "src" value, instead of being returned.
                            unreachable!("State changed variant before finalizer ran");
                        };
                        ($finalize as fn(Self) -> Result<(), Report>)(state)
                    },
                )
            }
        }
    };
}

impl_state_transition! {
    next_state: fn(&mut Launcher, DatabaseState) -> SearchMenu = |launcher, db| {
        SearchMenu::new(
            launcher.persistent.clone(),
            db,
            launcher.dialog.clone(),
            Default::default(),
        )
    },
}

// Identity transitions are valid
impl_state_transition!(
    next_state: fn(&mut SearchMenu, DatabaseState) -> SearchMenu = |search_menu, db| {
        SearchMenu::new(
            search_menu.persistent.clone(),
            db,
            search_menu.dialog.clone(),
            Default::default(),
        )
    },
);

impl_state_transition!(
    next_state: fn(&mut SearchMenu) -> SearchResults = |search_menu| {
        SearchResults::new(
            search_menu.persistent.clone(),
            search_menu.database.clone(),
            search_menu.search.clone(),
            Default::default(),
        )
    },
);

impl_state_transition!(
    next_state: fn(&mut SearchMenu) -> Edit = |search_menu| {
        Edit::new(
            search_menu.persistent.clone(),
            search_menu.database.clone().into(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    },
);

/// Parameter for this state transition that is just used as a marker to indicate what should be done with the transaction in
/// the finalizer. Using distinct parameter types allows these two transitions, with the same `from` and `to` states, to be
/// distinguished at the type-system level.
pub struct Commit;

impl_state_transition!(
    next_state: fn(&mut Edit, Commit) -> SearchMenu = |edit, _| {
        SearchMenu::new(
            edit.persistent.clone(),
            edit.database.clone_db(),
            Default::default(),
            Default::default(),
        )
    },
    finalize: fn(Edit) = |edit| edit.database.commit().into_diagnostic()
);

/// Parameter for this state transition that is just used as a marker to indicate what should be done with the transaction in
/// the finalizer. Using distinct parameter types allows these two transitions, with the same `from` and `to` states, to be
/// distinguished at the type-system level.
pub struct Rollback;

impl_state_transition!(
    next_state: fn(&mut Edit, Rollback) -> SearchMenu = |edit, _| {
        SearchMenu::new(
            edit.persistent.clone(),
            edit.database.clone_db(),
            Default::default(),
            Default::default(),
        )
    },
    finalize: fn(Edit) = |edit| { edit.database.rollback(); Ok(()) }
);

impl_state_transition!(
    next_state: fn(&mut SearchResults) -> SearchResults = |search_results| {
        SearchResults::new(
            search_results.persistent.clone(),
            search_results.database.clone(),
            search_results.search.clone(),
            Default::default(),
        )
    },
);

impl_state_transition!(
    next_state: fn(&mut SearchResults) -> Edit = |search_results| {
        Edit::new(
            search_results.persistent.clone(),
            search_results.database.clone().into(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    },
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

/// `self` must be passed explicitly as its own ident (`this`) due to how macros work
macro_rules! set_err {
    ($this:ident, $result:expr $(,)?) => {
        set_err!($this, $result, ())
    };
    ($this:ident, $result:expr, $return:expr $(,)?) => {{
        let result: Result<_, Report> = $result;
        match result {
            Ok(ok) => ok,
            Err(e) => {
                $this.active_overlay = ActiveOverlay::Error(e);
                return $return;
            }
        }
    }};
}

impl ActiveView {
    pub fn new<T: AnyDebug>(data: T) -> Self {
        match AppData::open() {
            Ok(persistent) => Launcher::new_with(data, persistent, Default::default()).into(),
            Err(e) => UnrecoverableError {
                active_overlay: ActiveOverlay::Error(e),
                ..UnrecoverableError::new_with(data)
            }
            .into(),
        }
    }
}

pub struct EditEntry {
    pub id: Entry,
    pub name: String,
    /// Why a `BTreeSet`? Because we need an ordered collection that also support set operations (i.e. `intersection`)
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

/// The UI that uses this stuff is present in multiple states, abstracting it behind a trait makes that UI code reusable
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
        self.queue_next_state_with::<SearchMenu, _>(database);
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
        self.queue_next_state_with::<SearchMenu, _>(database);
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

/// The UI that uses this stuff is present in multiple states, abstracting it behind a trait makes that UI code reusable
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
        self.queue_next_state::<SearchResults>();
    }

    /// Queues the [`Edit`] state.
    fn edit_entries(&mut self) {
        self.queue_next_state::<Edit>();
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    /// Queues the [`Edit`] state.
    fn import_files(&mut self, files: &[PathBuf]) {
        self.queue_next_state::<Edit>();
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
        self.queue_next_state::<SearchResults>();
    }

    /// Queues the [`Edit`] state.
    fn edit_entries(&mut self) {
        self.queue_next_state::<Edit>();
    }

    /// The user picks one or more files, and the editor is opened with new template entries for those files
    /// Queues the [`Edit`] state.
    fn import_files(&mut self, files: &[PathBuf]) {
        self.queue_next_state::<Edit>();
    }
}

impl Edit {
    /// Commits the transaction.
    /// Queues the [`SearchMenu`] state.
    pub fn save_changes(&mut self) {
        self.queue_next_state_with::<SearchMenu, _>(Commit);
    }

    /// Rolls back the transaction.
    /// Queues the [`SearchMenu`] state.
    pub fn cancel(&mut self) {
        self.queue_next_state_with::<SearchMenu, _>(Rollback);
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
        // The std `intersection` methods are only implemented for pairs of sets, but we have arbitrarily many
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
        let tag_exists = set_err!(self, self.database.tag_exists(&tag).into_diagnostic());
        if tag_exists && self.add_tag_to_selected(&tag) {
            self.tag_search_bar_state.clear();
        }
    }

    pub fn tag_prefix_search_results(&mut self) -> impl Iterator<Item = Tag> {
        self.database
            .search_tags_names_by_prefix(&self.tag_search_bar_state)
            .map_while(|result| Some(set_err!(self, result.into_diagnostic(), None)))
    }
}
