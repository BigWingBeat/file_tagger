use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, theme::ZYNC_600},
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{FlexExt, MainAxisAlignment, flex_row, task, text_button, text_input},
};

use file_tagger_internals::{ActiveOverlay, AppState};

use crate::XilemAppState;

pub fn search_bar(state: &mut XilemAppState) -> impl WidgetView<XilemAppState> + use<> {
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    // TODO: on-hover styling (see: <https://github.com/linebender/xilem/issues/1786>)
    flex_row((
        text_input(
            state.search_menu.search_text.clone(),
            |state: &mut XilemAppState, text| state.search_menu.search_text = text,
        )
        .on_enter(|state: &mut XilemAppState, _| state.search_results())
        .placeholder("Search database by tag")
        .border_width(0.px())
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |state: &mut XilemAppState| state.search_results())
            .corner_radius(f64::MAX.px()) // circle
            .border_width(0.px())
            // Manually tuned padding to make it look centered and circular
            .padding(Padding {
                top: 5.px(),
                ..Padding::horizontal(8.px())
            }),
    ))
    .gap(1.px())
    // The text input has its own padding, this is just for the button
    .padding(Padding {
        right: 12.px(),
        ..Padding::vertical(2.px())
    })
    // Border and corner radius the same as the text input
    .border(ZYNC_600, 1.px())
    .corner_radius(4.px())
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
