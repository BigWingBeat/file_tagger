use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::{Dimensions, Gap, LineBreaking},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, MainAxisAlignment, button, flex_col, flex_item, flex_row,
        label, portal, text_button,
    },
};

use crate::{
    AppState,
    persistent_data::{PersistentData, RecentFolder},
};

pub struct LauncherState {
    recent_folders: Vec<RecentFolder>,
}

impl LauncherState {
    pub fn new(data: &PersistentData) -> miette::Result<Self> {
        data.recent_folders()
            .map(|recent_folders| Self { recent_folders })
    }

    pub fn push_recent_folder(&mut self, folder: RecentFolder) -> &[RecentFolder] {
        self.recent_folders.push(folder);
        &self.recent_folders
    }
}

fn recent_list(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // The width of these buttons shouldn't depend on the size of the displayed paths, as those will change.
    // Instead, they are always as wide as possible
    flex_col(
        state
            .launcher
            .recent_folders
            .iter()
            .rev()
            .map(|folder| {
                let path = folder.path.clone();
                // TODO: highlight on hover
                button(
                    flex_col((
                        label(folder.name.to_string_lossy().into_owned())
                            .weight(FontWeight::BOLD)
                            .line_break_mode(LineBreaking::WordWrap),
                        label(path.to_string_lossy()).line_break_mode(LineBreaking::WordWrap),
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

fn recent_list_portal(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Open Recent")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(recent_list(state)).constrain_horizontal(true),
    ))
}

/// The "open" button selects an existing folder and either creates a new database in that folder, or opens a database
/// that already exists there.
/// The "create" button selects a folder, and creates a *new* empty folder there, with a specified name, as well as
/// creating a new database in the new folder.
fn open_create_buttons(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // These buttons should be the same width
    flex_col((
        flex_row((
            flex_col((
                label("Open Folder As Database").weight(FontWeight::BOLD),
                label("Open or create a database in a folder"),
            ))
            .cross_axis_alignment(CrossAxisAlignment::End)
            .gap(Gap::ZERO)
            .flex(0.5),
            text_button("Open", |state: &mut AppState| {
                state.open_database_in_folder();
            })
            .dims(Dimensions::height(Dim::Stretch))
            .flex(0.4),
        )),
        flex_row((
            flex_col((
                label("Create New Database").weight(FontWeight::BOLD),
                label("Create a new folder with a new database"),
            ))
            .cross_axis_alignment(CrossAxisAlignment::End)
            .gap(Gap::ZERO)
            .flex(0.5),
            text_button("Create", |state: &mut AppState| {
                state.create_folder_with_database();
            })
            .dims(Dimensions::height(Dim::Stretch))
            .flex(0.4),
        )),
    ))
    .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
}

pub fn launcher(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_row((
        flex_item(recent_list_portal(state), 0.5),
        flex_item(open_create_buttons(state), 0.5),
    ))
}
