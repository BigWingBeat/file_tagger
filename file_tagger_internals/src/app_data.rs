use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use arrayvec::ArrayVec;
use miette::IntoDiagnostic;
use smallvec::SmallVec;

use crate::{
    APP_DATA_FOLDER_NAME,
    serde::{AsBytes, BytesInto},
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
    /// The `ArrayVec` is boxed as with a capacity of 10 it is 488 bytes, which would more than double the size of `ActiveView`
    recent_folders: Box<ArrayVec<RecentFolder, MAX_RECENTS>>,
}

impl AppData {
    fn recent_folders_path() -> PathBuf {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(APP_DATA_FOLDER_NAME);
        path.push("recent_folders");
        path
    }

    pub fn open() -> miette::Result<Self> {
        let path = Self::recent_folders_path();
        Self::open_path(&path)
    }

    fn open_path(path: impl AsRef<Path>) -> miette::Result<Self> {
        let bytes = std::fs::read(path)
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(Vec::new()),
                _ => Err(e),
            })
            .into_diagnostic()?;

        let paths: SmallVec<[String; MAX_RECENTS]> = bytes.bytes_into().into_diagnostic()?;
        let recent_folders = Box::new(
            paths
                .iter()
                .take(MAX_RECENTS)
                .map(PathBuf::from)
                .filter(|p| p.exists())
                .map(PathBuf::into)
                .collect(),
        );
        Ok(Self { recent_folders })
    }

    fn write_recent_folders(&self) -> std::io::Result<()> {
        let path = Self::recent_folders_path();
        self.write_recent_folders_to_path(&path)
    }

    fn write_recent_folders_to_path(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let buffer = self
            .recent_folders
            .clone()
            .into_iter()
            .filter_map(|folder| folder.path.into_string().ok())
            .collect::<SmallVec<[String; MAX_RECENTS]>>();

        std::fs::write(path, buffer.as_bytes())
    }

    pub fn push_recent_folder(&mut self, folder: PathBuf) -> std::io::Result<&RecentFolder> {
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
}
