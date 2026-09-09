use std::{ffi::OsString, path::PathBuf};

use arrayvec::ArrayVec;
use miette::IntoDiagnostic;

use crate::{
    APP_DATA_FOLDER_NAME, Buffer,
    database::{Database, DeserError, Table},
    serde::{AsBytes, Bytes, FromBytes, SizeHint, SmallVec},
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
    /// The only sensible value type is `Buffer` as the actual value type varies per-key
    table: Table<ConstKey, Buffer>,
    /// The `ArrayVec` is boxed as with a capacity of 10 it is 488 bytes, which would more than double the size of `ActiveView`
    recent_folders: Box<ArrayVec<RecentFolder, MAX_RECENTS>>,
}

impl AppData {
    pub fn open() -> miette::Result<Self> {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(APP_DATA_FOLDER_NAME);
        Database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_tables(mut database: Database) -> miette::Result<Self> {
        let table = database.open_table("AppData").into_diagnostic()?;
        let recent_folders = database
            .get::<RecentFolders>(&table)
            .map(|result| {
                Box::new(
                    result
                        .unwrap_or_else(SmallVec::new)
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

    pub fn push_recent_folder(
        &mut self,
        folder: PathBuf,
    ) -> crate::database::Result<&RecentFolder> {
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

        // Write the change back to the DB
        let buffer = self
            .recent_folders
            .iter()
            .filter_map(|folder| folder.path.clone().into_string().ok())
            .collect();

        self.database
            .insert::<RecentFolders>(&mut self.table, &buffer)
            .map(|_| folder)
    }

    pub fn recent_folders(&self) -> &[RecentFolder] {
        &self.recent_folders
    }
}

/// A key that has to be known at compile time
struct ConstKey(&'static [u8]);

impl SizeHint for ConstKey {
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for ConstKey {
    type Bytes = &'static [u8];

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Owned(self.0)
    }
}

/// A static key that is associated with a specific value type
trait ConstTypedKey {
    const KEY: ConstKey;
    type Value;
}

macro_rules! impl_const_typed_key {
    ($ty:ty, $value:ty) => {
        impl ConstTypedKey for $ty {
            const KEY: ConstKey = ConstKey(stringify!($ty).as_bytes());
            type Value = $value;
        }
    };
}

/// Helper methods for working with the above type and trait
trait DatabaseExt: crate::database::DbApi {
    fn get<Key: ConstTypedKey<Value: FromBytes>>(
        &self,
        table: &Table<ConstKey, Buffer>,
    ) -> Result<Option<Key::Value>, DeserError<Key::Value>> {
        // SAFETY: `Table` is `#[repr(transparent)]` and we are only transmuting the phantomdata/type parameters
        let table = unsafe {
            std::mem::transmute::<&Table<ConstKey, Buffer>, &Table<ConstKey, Key::Value>>(table)
        };
        crate::database::DbApi::get(self, table, &Key::KEY)
    }

    fn insert<Key: ConstTypedKey<Value: AsBytes>>(
        &mut self,
        table: &mut Table<ConstKey, Buffer>,
        value: &Key::Value,
    ) -> crate::database::Result<()> {
        // SAFETY: `Table` is `#[repr(transparent)]` and we are only transmuting the phantomdata/type parameters
        let table = unsafe {
            std::mem::transmute::<&mut Table<ConstKey, Buffer>, &mut Table<ConstKey, Key::Value>>(
                table,
            )
        };
        crate::database::DbApi::insert(self, table, &Key::KEY, value)
    }
}

impl<T: crate::database::DbApi> DatabaseExt for T {}

struct RecentFolders;

impl_const_typed_key!(RecentFolders, SmallVec<String>);
