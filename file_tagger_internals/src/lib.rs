mod app_data;
mod app_state;
mod database;
mod tags_db;

pub use crate::{
    app_data::{AppData, RecentFolder},
    app_state::{
        ActiveOverlay, ActiveView, AppState, EditEntry, EditState, SearchBarState,
        SearchResultsState,
    },
    database::{
        Buffer, Error as DatabaseError, FinalizeTransaction, Result as DatabaseResult, Table,
        Transaction, TransactionApi, TransactionHandle, TransactionImpl, TransactionResult,
        UntypedTable, UntypedTransactionApi, initialize_transaction,
    },
    tags_db::{DatabaseState, Entry, Tag, TagsDatabase},
};

const FOLDER_NAME: &str = ".file_tagger";
