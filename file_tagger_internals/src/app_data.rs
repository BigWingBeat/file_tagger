use std::{ffi::OsString, path::PathBuf};

use miette::IntoDiagnostic;
use thiserror::Error;

use crate::{
    FOLDER_NAME,
    database::{self, Database, InlineStrVec, Table},
};

#[derive(Clone)]
pub struct RecentFolder {
    pub name: OsString,
    pub path: PathBuf,
}

impl Eq for RecentFolder {}

impl PartialEq for RecentFolder {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl From<PathBuf> for RecentFolder {
    fn from(path: PathBuf) -> Self {
        Self {
            name: path.file_name().unwrap_or(path.as_os_str()).to_owned(),
            path,
        }
    }
}

/// Persistent data associated with the application, such as settings and "open recent" history
pub struct AppData {
    database: Database,
    table: Table<DataKey, InlineStrVec>,
    recent_folders: Vec<RecentFolder>,
}

impl AppData {
    pub fn open() -> miette::Result<Self> {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(FOLDER_NAME);
        database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_tables(database: Database) -> miette::Result<Self> {
        let table = Table::open(&database, "AppData").into_diagnostic()?;
        let recent_folders = table.get(&DataKey::RecentFolders).map(|result| {
            result
                .unwrap_or_else(InlineStrVec::empty)
                .iter()
                .map(PathBuf::from)
                .map(PathBuf::into)
                .collect()
        })?;
        Ok(Self {
            database,
            table,
            recent_folders,
        })
    }

    pub fn push_recent_folder(&mut self, folder: PathBuf) -> &RecentFolder {
        let folder = folder.into();
        // Doing a linear search is fine as this vec is always small.
        // We can't just use `contains()` here as we need to get a reference to the element.
        // For some reason getting and using the index is needed to workaround the borrow checker
        if let Some(i) = self
            .recent_folders
            .iter()
            .enumerate()
            .find_map(|(i, f)| (*f == folder).then_some(i))
        {
            &self.recent_folders[i]
        } else {
            self.recent_folders.push_mut(folder)
        }
    }

    pub fn recent_folders(&self) -> &[RecentFolder] {
        &self.recent_folders
    }

    pub fn write_recent_folders(&self) -> database::Result<()> {
        let buffer = self
            .recent_folders
            .iter()
            .filter_map(|folder| folder.path.to_str())
            .collect();
        self.table.insert(&DataKey::RecentFolders, &buffer)
    }
}

enum DataKey {
    RecentFolders,
}

#[derive(Error, Debug)]
enum DataKeyParseError {
    #[error("Invalid key")]
    InvalidKey,
}

impl AsRef<[u8]> for DataKey {
    fn as_ref(&self) -> &[u8] {
        match self {
            DataKey::RecentFolders => b"RecentFolders",
        }
    }
}

impl TryFrom<&[u8]> for DataKey {
    type Error = DataKeyParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"RecentFolders" => Ok(Self::RecentFolders),
            _ => Err(DataKeyParseError::InvalidKey),
        }
    }
}
