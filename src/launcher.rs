use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::Dim,
        properties::{Dimensions, Gap},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, MainAxisAlignment, flex_col, flex_item, flex_row, prose,
        text_button,
    },
};

use crate::AppState;

/// The "open" button selects an existing folder and either creates a new database in that folder, or opens a database
/// that already exists there.
/// The "create" button selects a folder, and creates a *new* empty folder there, with a specified name, as well as
/// creating a new database in the new folder.
fn open_create_buttons(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // These buttons should be the same width
    flex_col((
        flex_row((
            flex_col((
                prose("Open Folder As Database").weight(FontWeight::BOLD),
                prose("Open or create a database in a folder"),
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
                prose("Create New Database").weight(FontWeight::BOLD),
                prose("Create a new folder with a new database"),
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
        flex_item(crate::persistent_data::recent_list_portal(state), 0.5),
        flex_item(open_create_buttons(state), 0.5),
    ))
}
