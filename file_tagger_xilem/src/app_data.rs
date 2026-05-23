use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::{Dimensions, LineBreaking},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, portal, prose},
};

use file_tagger_internals::AppState;

pub fn recent_list(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // The width of these buttons shouldn't depend on the size of the displayed paths, as those will change.
    // Instead, they are always as wide as possible
    flex_col(
        state
            .persistent
            .recent_folders()
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
                .border_width(Length::const_px(0.0))
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
