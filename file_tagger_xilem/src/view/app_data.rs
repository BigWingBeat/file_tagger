use file_tagger_internals::Launcher;
use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        properties::{Dimensions, LineBreaking},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, portal, prose},
};

pub fn recent_list(state: &mut Launcher) -> impl WidgetView<Launcher> + use<> {
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
                    .gap(0.px()),
                    move |state: &mut Launcher| {
                        state.open_database_in_folder(path.clone());
                    },
                )
                .border_width(0.px())
            })
            .collect::<Vec<_>>(),
    )
    .gap(1.px())
    .dims(Dimensions::width(Dim::Stretch))
}

pub fn recent_list_portal(state: &mut Launcher) -> impl WidgetView<Launcher> + use<> {
    flex_col((
        prose("Open Recent")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(recent_list(state)).constrain_horizontal(true),
    ))
}
