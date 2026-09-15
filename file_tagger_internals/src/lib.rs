mod app_data;
mod database;
mod files;
mod serde;
mod state;
mod tags_db;

pub use crate::{
    app_data::{AppData, RecentFolder},
    database::{
        Buffer, Conflict, Error as DatabaseError, Result as DatabaseResult, Table, Transaction,
        UntypedTable,
    },
    files::{create_folder_and_db, import_files, open_folder_as_db},
    state::{
        ActiveOverlay, ActiveView, Edit, EditEntry, Launcher, LauncherState, SearchMenu,
        SearchResults, SearchState, UnrecoverableError,
    },
    tags_db::{AnyRange, DatabaseState, Entry, Tag, TagsDatabase},
};

// We could use the Cargo env var macros for this but as far as I'm aware there aren't any for workspace name
const DB_FOLDER_NAME: &str = ".file_tagger";
const APP_DATA_FOLDER_NAME: &str = "file_tagger";

#[cfg(test)]
mod test {
    macro_rules! serde_roundtrip {
        ($ty:ty, $value:expr, $bytes:expr) => {{
            #[allow(clippy::type_complexity, reason = "shut up")]
            let value: $ty = $value;
            let bytes = crate::serde::AsBytes::as_bytes(&value);
            assert_eq!(bytes.as_ref(), $bytes);
            #[allow(clippy::type_complexity, reason = "shut up")]
            let roundtrip: $ty = crate::serde::BytesInto::bytes_into(&bytes).unwrap();
            assert_eq!(roundtrip, value);
        }};
    }
    pub(crate) use serde_roundtrip;
}
