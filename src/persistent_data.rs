use std::{ffi::OsString, path::PathBuf};

use crate::database::{
    InlineStrVec, Table,
    backend::{self, DatabaseImpl},
    typed_table,
};

pub struct RecentFolder {
    pub name: OsString,
    pub path: PathBuf,
}

impl From<PathBuf> for RecentFolder {
    fn from(path: PathBuf) -> Self {
        Self {
            name: path.file_name().unwrap_or(path.as_os_str()).to_owned(),
            path,
        }
    }
}

pub struct PersistentData {
    database: backend::Database,
    table: PersistentDataTable,
}

impl PersistentData {
    pub fn open() -> backend::Result<Self> {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(env!("CARGO_BIN_NAME"));
        backend::open(path).and_then(Self::open_tables)
    }

    fn open_tables(database: backend::Database) -> backend::Result<Self> {
        let table = PersistentDataTable::open(&database)?;
        Ok(Self { database, table })
    }

    pub fn recent_folders(&self) -> miette::Result<Vec<RecentFolder>> {
        self.table.get(&DataKey::RecentFolders).map(|result| {
            result
                .unwrap_or_else(InlineStrVec::empty)
                .iter()
                .map(PathBuf::from)
                .map(PathBuf::into)
                .collect()
        })
    }

    pub fn write_recent_folders(&self, recent: &[RecentFolder]) -> backend::Result<()> {
        let buffer = recent
            .iter()
            .filter_map(|folder| folder.path.to_str())
            .collect();
        self.table.insert(&DataKey::RecentFolders, &buffer)
    }
}

enum DataKey {
    RecentFolders,
}

impl AsRef<[u8]> for DataKey {
    fn as_ref(&self) -> &[u8] {
        match self {
            DataKey::RecentFolders => "RecentFolders".as_bytes(),
        }
    }
}

typed_table! {
    struct PersistentDataTable<DataKey, InlineStrVec>;
}
