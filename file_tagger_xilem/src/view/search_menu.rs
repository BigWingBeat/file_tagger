use xilem::{
    WidgetView,
    core::lens,
    masonry::properties::Dimensions,
    view::{FlexSpacer, MainAxisAlignment, flex_row, task, text_button, text_input},
};

use file_tagger_internals::{ActiveOverlay, AppState, SearchMenu};

use crate::{
    XilemAppState,
    view::{centered_box, submittable_text_input},
};

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
        text_input(
            state.search_menu.search_text.clone(),
            |state: &mut SearchMenu, text| state.search_menu.search_text = text,
        )
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
        text_button("Import Files", |state: &mut SearchMenu| {
            state.run_task(|_| {
                task(
                    |proxy, _| async move {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Select Files to Import")
                            .pick_files()
                            .await;
                        proxy.message(result);
                    },
                    |state: &mut AppState, result| {
                        state.active_overlay = ActiveOverlay::None;
                        if let Some(files) = result
                            && !files.is_empty()
                        {
                            state.import_files(&files.iter().map(Into::into).collect::<Vec<_>>())
                        }
                    },
                )
            });
        }),
    ))
    .main_axis_alignment(MainAxisAlignment::Center)
}
