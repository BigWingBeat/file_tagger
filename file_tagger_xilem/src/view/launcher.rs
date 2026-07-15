use xilem::{
    FontWeight, WidgetView,
    core::fork,
    masonry::{
        layout::Dim,
        properties::{Dimensions, Gap},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, FlexSpacer, MainAxisAlignment, flex_col, flex_item, flex_row,
        prose, task, text_button,
    },
};

use file_tagger_internals::{Launcher, LauncherState};

use crate::view::{app_data::recent_list_portal, centered_box};

pub fn launcher_view(state: &mut Launcher) -> impl WidgetView<Launcher> + use<> {
    centered_box((launcher(state), FlexSpacer::Flex(1.0)))
}

/// The "open" button selects an existing folder and either creates a new database in that folder, or opens a database
/// that already exists there.
/// The "create" button selects a folder, and creates a *new* empty folder there, with a specified name, as well as
/// creating a new database in the new folder.
fn open_create_buttons<L: LauncherState>(state: &mut L) -> impl WidgetView<L> + use<L> {
    // These buttons should be the same width and height
    flex_col((
        flex_row((
            flex_col((
                prose("Open Folder As Database").weight(FontWeight::BOLD),
                prose("Open or create a database in a folder"),
            ))
            .gap(Gap::ZERO)
            .flex(2.0 / 3.0),
            fork(
                text_button("Open", |state: &mut L| {
                    state.start_open_dialog();
                })
                .dims(Dimensions::height(Dim::Stretch))
                .flex(1.0 / 3.0),
                state.open_dialog_active().then_some(task(
                    |proxy, _| async move {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Open Database As Folder")
                            .pick_folder()
                            .await;
                        proxy.message(result);
                    },
                    |state: &mut L, result| {
                        state.stop_open_dialog();
                        if let Some(folder) = result {
                            state.open_database_in_folder(folder.into());
                        }
                    },
                )),
            ),
        )),
        flex_row((
            flex_col((
                prose("Create New Database").weight(FontWeight::BOLD),
                prose("Create a new folder with a new database"),
            ))
            .gap(Gap::ZERO)
            .flex(2.0 / 3.0),
            fork(
                text_button("Create", |state: &mut L| {
                    state.start_create_dialog();
                })
                .dims(Dimensions::height(Dim::Stretch))
                .flex(1.0 / 3.0),
                state.create_dialog_active().then_some(task(
                    |proxy, _| async move {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Create New Folder And Database")
                            .save_file()
                            .await;
                        proxy.message(result);
                    },
                    |state: &mut L, result| {
                        state.stop_create_dialog();
                        if let Some(folder) = result {
                            state.create_folder_with_database(folder.into());
                        }
                    },
                )),
            ),
        )),
    ))
    .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
}

pub fn launcher<L: LauncherState>(state: &mut L) -> impl WidgetView<L> + use<L> {
    flex_row((
        flex_item(recent_list_portal(state), 0.5),
        flex_item(open_create_buttons(state), 0.5),
    ))
}
