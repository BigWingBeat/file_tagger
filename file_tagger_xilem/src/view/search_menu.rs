use xilem::{
    WidgetView,
    masonry::properties::Dimensions,
    view::{MainAxisAlignment, flex_row, task, text_button, text_input},
};

use file_tagger_internals::{ActiveOverlay, AppState};

use crate::{XilemAppState, view::submittable_text_input};

pub fn search_bar(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    submittable_text_input(
        text_input(
            state.search_menu.search_text.clone(),
            |state: &mut XilemAppState, text| state.search_menu.search_text = text,
        )
        .on_enter(|state: &mut XilemAppState, _| state.search_results())
        .placeholder("Search database by tag"),
        text_button("🔍", |state: &mut XilemAppState| state.search_results()),
        Dimensions::AUTO,
    )
}

pub fn edit_buttons(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    flex_row((
        text_button("Edit Entries", |state: &mut XilemAppState| {
            state.edit_entries()
        }),
        text_button("Import Files", |state: &mut XilemAppState| {
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
