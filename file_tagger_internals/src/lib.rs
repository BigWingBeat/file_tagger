mod app_data;
mod database;
mod state;
mod tags_db;

pub use crate::{
    app_data::{AppData, RecentFolder},
    database::{
        Buffer, Error as DatabaseError, FinalizeTransaction, Result as DatabaseResult, Table,
        Transaction, TransactionApi, TransactionImpl, TransactionResult, UntypedTable,
        UntypedTransactionApi, initialize_transaction,
    },
    state::{
        ActiveOverlay, ActiveView, Edit, EditEntry, Launcher, LauncherState, Loading, SearchMenu,
        SearchResults, SearchState,
    },
    tags_db::{DatabaseState, Entry, Tag, TagsDatabase},
};

const DB_FOLDER_NAME: &str = ".file_tagger";
const APP_DATA_FOLDER_NAME: &str = "file_tagger";
