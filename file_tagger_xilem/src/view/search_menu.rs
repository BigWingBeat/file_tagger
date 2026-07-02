use xilem::{
    WidgetView,
    core::{fork, lens},
    masonry::properties::Dimensions,
    view::{FlexSpacer, MainAxisAlignment, flex_row, task, text_button, text_input},
};

use file_tagger_internals::SearchMenu;

use crate::view::{active_folder_name, centered_box, launcher::launcher, submittable_text_input};

pub fn search_menu_view(state: &mut SearchMenu) -> impl WidgetView<SearchMenu> + use<> {
    centered_box((
        lens(active_folder_name, |state: &mut SearchMenu| {
            &mut state.database
        }),
        search_bar(state),
        edit_buttons(state),
        launcher(state),
        FlexSpacer::Flex(1.0),
    ))
}

pub fn search_bar(state: &mut SearchMenu) -> impl WidgetView<SearchMenu> + use<> {
    submittable_text_input(
        text_input(state.search_bar.clone(), |state: &mut SearchMenu, text| {
            state.search_bar = text
        })
        .on_enter(|state: &mut SearchMenu, _| state.search_results())
        .placeholder("Search database by tag"),
        text_button("🔍", |state: &mut SearchMenu| state.search_results()),
        Dimensions::AUTO,
    )
}

pub fn edit_buttons(state: &mut SearchMenu) -> impl WidgetView<SearchMenu> + use<> {
    flex_row((
        text_button("Edit Entries", |state: &mut SearchMenu| {
            state.edit_entries()
        }),
        fork(
            text_button("Import Files", |state: &mut SearchMenu| {
                state.start_import_dialog();
                true;
            }),
            state.import_dialog_active().then_some(task(
                |proxy, _| async move {
                    let result = rfd::AsyncFileDialog::new()
                        .set_title("Select Files to Import")
                        .pick_files()
                        .await;
                    proxy.message(result);
                },
                |state: &mut SearchMenu, result| {
                    state.stop_import_dialog();
                    if let Some(files) = result
                        && !files.is_empty()
                    {
                        state.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
                    }
                },
            )),
        ),
    ))
    .main_axis_alignment(MainAxisAlignment::Center)
}
