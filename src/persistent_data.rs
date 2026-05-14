use std::{ffi::OsString, path::PathBuf};

use miette::IntoDiagnostic;
use thiserror::Error;
use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::{Dimensions, LineBreaking},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, portal, prose},
};

use crate::{
    AppState,
    database::{self, Database, InlineStrVec, Table},
};

#[derive(Clone)]
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
    database: Database,
    table: Table<DataKey, InlineStrVec>,
    pub recent_folders: Vec<RecentFolder>,
}

pub fn recent_list(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // The width of these buttons shouldn't depend on the size of the displayed paths, as those will change.
    // Instead, they are always as wide as possible
    flex_col(
        state
            .persistent
            .recent_folders
            .iter()
            .rev()
            .map(|folder| {
                let path = folder.path.clone();
                // TODO: highlight on hover
                button(
                    flex_col((
                        prose(folder.name.to_string_lossy().into_owned())
                            .weight(FontWeight::BOLD)
                            .line_break_mode(LineBreaking::WordWrap),
                        prose(path.to_string_lossy()).line_break_mode(LineBreaking::WordWrap),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .gap(Length::const_px(0.0)),
                    move |state: &mut AppState| {
                        state.open_recent(path.clone());
                    },
                )
                .border_width(0.0)
            })
            .collect::<Vec<_>>(),
    )
    .gap(Length::const_px(1.0))
    .dims(Dimensions::width(Dim::Stretch))
}

pub fn recent_list_portal(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        prose("Open Recent")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(recent_list(state)).constrain_horizontal(true),
    ))
}

impl PersistentData {
    pub fn open() -> miette::Result<Self> {
        let mut path = dirs::data_local_dir().unwrap();
        path.push(env!("CARGO_BIN_NAME"));
        database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_tables(database: Database) -> miette::Result<Self> {
        let table = Table::open(&database, "PersistentData").into_diagnostic()?;
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
