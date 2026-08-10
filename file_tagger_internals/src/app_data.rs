use std::{ffi::OsString, path::PathBuf};

use arrayvec::ArrayVec;
use miette::IntoDiagnostic;
use thiserror::Error;

use crate::{
    APP_DATA_FOLDER_NAME,
    database::{self, AsBytes, Bytes, Database, FromBytes, InlineStrVec, SizeHint, Table},
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

const MAX_RECENTS: usize = 10;

/// Persistent data associated with the application, such as settings and "open recent" history
#[derive(Clone)]
pub struct AppData {
    database: Database,
    table: Table<DataKey, InlineStrVec>,
    /// The `ArrayVec` is boxed as with a capacity of 10 it is 488 bytes, which would more than double the size of `ActiveView`
    recent_folders: Box<ArrayVec<RecentFolder, MAX_RECENTS>>,
}

impl AppData {
    pub fn open() -> miette::Result<Self> {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(APP_DATA_FOLDER_NAME);
        database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_tables(database: Database) -> miette::Result<Self> {
        let table = Table::open(&database, "AppData").into_diagnostic()?;
        let recent_folders = table
            .get(&DataKey::RecentFolders)
            .map(|result| {
                Box::new(
                    result
                        .unwrap_or_else(InlineStrVec::empty)
                        .iter()
                        .take(MAX_RECENTS)
                        .map(PathBuf::from)
                        .filter(|p| p.exists())
                        .map(PathBuf::into)
                        .collect(),
                )
            })
            .into_diagnostic()?;
        Ok(Self {
            database,
            table,
            recent_folders,
        })
    }

    pub fn push_recent_folder(&mut self, folder: PathBuf) -> database::Result<&RecentFolder> {
        let folder = folder.into();
        // Doing a linear search is fine as this vec is always small.
        // We can't just use `contains()` here as we need to get a reference to the element.
        // For some reason getting and using the index is needed to workaround the borrow checker
        let folder: &RecentFolder = if let Some(i) = self
            .recent_folders
            .iter()
            .enumerate()
            .find_map(|(i, f)| (*f == folder).then_some(i))
        {
            &self.recent_folders[i]
        } else {
            match self.recent_folders.try_push(folder) {
                Ok(_) => self.recent_folders.last().unwrap(),
                Err(e) => {
                    // The vec is full, replace the oldest entry instead
                    let folder = e.element();
                    // This moves every entry down by 1, with the oldest becoming the newest
                    self.recent_folders.rotate_left(1);
                    // This unwrap will never fail because `try_push` only returns `Err` when the vec is full
                    // (unless `MAX_RECENTS` is 0 for some reason, but then the `rotate_left` would have panicked first)
                    let most_recent = self.recent_folders.last_mut().unwrap();
                    *most_recent = folder;
                    // Dumb borrowck nonsense
                    self.recent_folders.last().unwrap()
                }
            }
        };
        self.write_recent_folders().map(|_| folder)
    }

    pub fn recent_folders(&self) -> &[RecentFolder] {
        &self.recent_folders
    }

    fn write_recent_folders(&self) -> database::Result<()> {
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

impl SizeHint for DataKey {
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for DataKey {
    type Bytes = &'static str;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        match self {
            DataKey::RecentFolders => Bytes::Owned("RecentFolders"),
        }
    }
}

impl FromBytes for DataKey {
    type Error = DataKeyParseError;

    fn try_from(bytes: &mut &[u8]) -> Result<Self, Self::Error> {
        // Signal to the caller that we're "consuming" all the bytes
        match std::mem::take(bytes) {
            b"RecentFolders" => Ok(Self::RecentFolders),
            _ => Err(DataKeyParseError::InvalidKey),
        }
    }
}
