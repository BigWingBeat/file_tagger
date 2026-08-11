mod app_data;
mod database;
mod files;
mod serde;
mod state;
mod tags_db;

pub use crate::{
    app_data::{AppData, RecentFolder},
    database::{
        Buffer, Error as DatabaseError, FinalizeTransaction, Result as DatabaseResult, Table,
        Transaction, TransactionApi, TransactionImpl, TransactionResult, UntypedTable,
        UntypedTransactionApi, initialize_transaction,
    },
    files::{create_folder_and_db, import_files, open_folder_as_db},
    state::{
        ActiveOverlay, ActiveView, Edit, EditEntry, Launcher, LauncherState, SearchMenu,
        SearchResults, SearchState, UnrecoverableError,
    },
    tags_db::{DatabaseState, Entry, Tag, TagsDatabase},
};

// We could use the Cargo env var macros for this but as far as I'm aware there aren't any for workspace name
const DB_FOLDER_NAME: &str = ".file_tagger";
const APP_DATA_FOLDER_NAME: &str = "file_tagger";
