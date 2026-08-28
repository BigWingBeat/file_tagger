use std::{any::TypeId, collections::BTreeSet, path::PathBuf};

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
                next_state: Option<Box<dyn FnOnce(Self) -> $name>>,
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
                    To: 'static,
                    Self: DynStateTransition<To>,
                {
                    self.next_state = Some(Box::new(DynStateTransition::transition));
                }

                pub fn queue_next_state_with<To, Parameters>(&mut self, parameters: Parameters)
                where
                    Parameters: 'static,
                    Self: DynStateTransitionWith<To, Parameters>,
                {
                    // State transitions work in a deferred manner: they are queued during a frame, then applied later. In each state type,
                    // the `next_state` field stores the function that performs the state transition until it is actually used.
                    //
                    // For some reason, `FnOnce` seems to be an exception to the rule that trait objects can't move `self` by value.
                    // We rely on this to allow consuming parameter values passed in at the call site where a state transition is queued,
                    // by using closure variable capturing to magically store those parameters in the trait object, until they are consumed
                    // by-value when the `FnOnce` is called during [`ActiveView::update_to_next`].
                    self.next_state = Some(Box::new(|from| DynStateTransitionWith::transition(from, parameters)));
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

            pub fn variant_type_id(&self) -> TypeId {
                match self {
                    $( $name::$variant(_) => TypeId::of::<$variant>() ),*
                }
            }

            /// Update self to the queued variant stored in `next_state`.
            /// Returns whether or not anything actually happened.
            ///
            /// `map_data` takes `TypeId`s, because taking `Discriminant`s is unhelpful, and taking the from/to `Self`s is
            /// impossible. There's no way to get `Discriminant`s statically, so the only thing you could do with them is
            /// check if this is an identity transition or not. As for taking the actual from/to enum values, we can't do that
            /// because constructing the new state requires consuming the previous state by-value, making it impossible to
            /// reference them both at the same time.
            pub fn update_to_next<F>(&mut self, map_data: F) -> bool
            where
                F: FnOnce(TypeId, TypeId, Box<dyn AnyDebug>) -> Box<dyn AnyDebug>
            {
                let from = self.variant_type_id();
                let mut transition_data = None;

                replace_with::replace_with_or_abort(self, |state| {
                    match state {
                        $(
                            $name::$variant(mut inner) if inner.next_state.is_some() => {
                                transition_data = Some(std::mem::replace(&mut inner.data, Box::new(())));
                                // This unwrap will never fail because of the `is_some()` check in the pattern guard.
                                // We could remove it if it was possible to write the pattern guard as
                                // `if let Some(transition) = inner.next_state.take()`, but sadly that doesn't compile.
                                (inner.next_state.take().unwrap())(inner)
                            }
                        ),*
                        _ => state
                    }
                });

                transition_data.is_some_and(|old_data| {
                    let to = self.variant_type_id();
                    let new_data = map_data(from, to, old_data);
                    self.set_data_dyn(new_data);
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

impl UnrecoverableError {
    pub fn new_error(e: Report) -> Self {
        Self {
            active_overlay: ActiveOverlay::Error(e),
            ..Self::new()
        }
    }

    pub fn new_error_with<T: AnyDebug>(e: Report, data: T) -> Self {
        Self {
            active_overlay: ActiveOverlay::Error(e),
            ..Self::new_with(data)
        }
    }
}

/// To get Clippy to shut up
impl Default for UnrecoverableError {
    fn default() -> Self {
        Self::new()
    }
}

/// A specific, infallible state transition
pub trait StateTransition<To> {
    fn transition(self) -> To;
}

/// A dynamic state transition that may not actually transition to the advertised target state
pub trait DynStateTransition<To> {
    fn transition(self) -> ActiveView;
}

/// `StateTransition` is more specific than `DynStateTransition`, so we just bound on
/// the latter and rely on this blanket impl to allow also using impls of the former
impl<From, To> DynStateTransition<To> for From
where
    From: StateTransition<To>,
    To: Into<ActiveView>,
{
    fn transition(self) -> ActiveView {
        self.transition().into()
    }
}

/// A specific, infallible state transition, with parameters
pub trait StateTransitionWith<To, Parameters> {
    fn transition(self, parameters: Parameters) -> To;
}

/// A dynamic state transition, with parameters, that may not actually transition to the advertised target state
pub trait DynStateTransitionWith<To, Parameters> {
    fn transition(self, parameters: Parameters) -> ActiveView;
}

/// `StateTransitionWith` is more specific than `DynStateTransitionWith`, so we just bound on
/// the latter and rely on this blanket impl to allow also using impls of the former
impl<From, To, Parameters> DynStateTransitionWith<To, Parameters> for From
where
    From: StateTransitionWith<To, Parameters>,
    To: Into<ActiveView>,
{
    fn transition(self, parameters: Parameters) -> ActiveView {
        self.transition(parameters).into()
    }
}

impl StateTransitionWith<SearchMenu, DatabaseState> for Launcher {
    fn transition(self, db: DatabaseState) -> SearchMenu {
        SearchMenu::new(
            self.persistent.clone(),
            db,
            self.dialog.clone(),
            Default::default(),
        )
    }
}

// Identity transitions are valid
impl StateTransitionWith<SearchMenu, DatabaseState> for SearchMenu {
    fn transition(self, db: DatabaseState) -> SearchMenu {
        SearchMenu::new(
            self.persistent.clone(),
            db,
            self.dialog.clone(),
            Default::default(),
        )
    }
}

impl StateTransition<SearchResults> for SearchMenu {
    fn transition(self) -> SearchResults {
        SearchResults::new(
            self.persistent.clone(),
            self.database.clone(),
            self.search.clone(),
            Default::default(),
        )
    }
}

/// Fallible state transitions are possible with the `Dyn`-variants of the state transition traits
impl DynStateTransition<Edit> for SearchMenu {
    fn transition(self) -> ActiveView {
        match self.database.initialize_transaction().into_diagnostic() {
            Ok(db) => Edit::new(
                self.persistent.clone(),
                db,
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .into(),
            Err(e) => UnrecoverableError::new_error(e).into(),
        }
    }
}

/// Parameter for this state transition that is just used as a marker to indicate what should be done with the transaction.
/// Using distinct parameter types allows these two transitions, with the same `from` and `to` states, to be
/// distinguished at the type-system level.
pub struct Commit;

impl StateTransitionWith<SearchMenu, Commit> for Edit {
    fn transition(self, _: Commit) -> SearchMenu {
        let (db, result) = self.database.commit();
        let mut state = SearchMenu::new(
            self.persistent.clone(),
            db,
            Default::default(),
            Default::default(),
        );
        if let Err(e) = result.into_diagnostic() {
            state.active_overlay = ActiveOverlay::Error(e);
        }
        state
    }
}

/// Parameter for this state transition that is just used as a marker to indicate what should be done with the transaction.
/// Using distinct parameter types allows these two transitions, with the same `from` and `to` states, to be
/// distinguished at the type-system level.
pub struct Rollback;

impl StateTransitionWith<SearchMenu, Rollback> for Edit {
    fn transition(self, _: Rollback) -> SearchMenu {
        SearchMenu::new(
            self.persistent.clone(),
            self.database.rollback(),
            Default::default(),
            Default::default(),
        )
    }
}

impl StateTransition<SearchResults> for SearchResults {
    fn transition(self) -> SearchResults {
        SearchResults::new(
            self.persistent.clone(),
            self.database.clone(),
            self.search.clone(),
            Default::default(),
        )
    }
}

impl DynStateTransition<Edit> for SearchResults {
    fn transition(self) -> ActiveView {
        match self.database.initialize_transaction().into_diagnostic() {
            Ok(db) => Edit::new(
                self.persistent.clone(),
                db,
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .into(),
            Err(e) => UnrecoverableError::new_error(e).into(),
        }
    }
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
            Err(e) => UnrecoverableError::new_error_with(e, data).into(),
        }
    }

    pub fn exit_error_overlay(&mut self) {
        let ActiveOverlay::Error(_) = self.active_overlay() else {
            eprintln!("exit_error_overlay called but not in error overlay");
            return;
        };

        if let ActiveView::UnrecoverableError(_) = self {
            std::process::exit(1);
        } else {
            self.set_active_overlay(ActiveOverlay::None);
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
        set_err!(self, std::fs::create_dir(&folder).into_diagnostic());
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
        set_err!(self, std::fs::create_dir(&folder).into_diagnostic());
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
