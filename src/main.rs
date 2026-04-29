use std::path::PathBuf;

use xilem::{
    EventLoop, FontWeight, WidgetView, WindowOptions, Xilem,
    masonry::{
        layout::{Dim, Length},
        properties::Dimensions,
        theme::{ZYNC_600, ZYNC_800, ZYNC_900},
    },
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{
        CrossAxisAlignment, FlexExt, MainAxisAlignment, button, flex_col, flex_row, label, portal,
        text_button, text_input,
    },
    winit::error::EventLoopError,
};

struct RecentFolder {
    name: String,
    path: PathBuf,
}

struct AppState {
    active_folder: String,
    search_text: String,
    recent_folders: Vec<RecentFolder>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_folder: "Folder".to_owned(),
            search_text: String::new(),
            recent_folders: vec![
                RecentFolder {
                    name: "Pictures".into(),
                    path: "~/Pictures".into(),
                },
                RecentFolder {
                    name: "Videos".into(),
                    path: "~/Videos".into(),
                },
                RecentFolder {
                    name: "Music".into(),
                    path: "~/Music".into(),
                },
                RecentFolder {
                    name: "Science Papers".into(),
                    path: "~/Documents/Science Papers".into(),
                },
            ],
        }
    }
}

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    let search_bar = flex_row((
        text_input(state.search_text.clone(), |state: &mut AppState, text| {
            state.search_text = text
        })
        .placeholder("Search files by tag")
        .border_width(0.0)
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |_| {})
            .corner_radius(f64::INFINITY) // circle
            .border_width(0.0)
            .padding(Padding {
                top: 5.0,
                ..Padding::horizontal(8.5)
            }),
    ))
    .gap(Length::const_px(1.0))
    .padding(Padding {
        right: 12.0,
        ..Padding::vertical(2.0)
    })
    .border(ZYNC_600, 1.0)
    .corner_radius(4.0);

    // The width of these buttons shouldn't depend on the size of the displayed paths, as those will change.
    // Instead, they are always as wide as possible
    let recent_list = flex_col((
        label("Open Recent")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(
            flex_col(
                state
                    .recent_folders
                    .iter()
                    .map(|folder| {
                        button(
                            flex_col((
                                label(folder.name.clone()).weight(FontWeight::BOLD),
                                label(folder.path.to_string_lossy()),
                            ))
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .gap(Length::const_px(0.0)),
                            |_| {},
                        )
                        .border_width(0.0)
                        // .dims(Dimensions::width(Dim::Stretch))
                    })
                    .collect::<Vec<_>>(),
            )
            .gap(Length::const_px(1.0))
            .dims(Dimensions::width(Dim::Stretch)),
        ),
    ))
    .flex(0.5);

    // Center the inner `flex_col`
    flex_col(
        // Centered and fixed size
        flex_col((
            label(state.active_folder.clone())
                .weight(FontWeight::BOLD)
                .text_size(20.0)
                .dims(Dimensions::width(Dim::Stretch)),
            search_bar,
            flex_row((
                recent_list,
                flex_col((
                    text_button("Open Database From Folder", |_: &mut AppState| {}),
                    text_button("Create New Database", |_: &mut AppState| {}),
                ))
                .main_axis_alignment(MainAxisAlignment::Center)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .flex(0.5),
            )),
        ))
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .dims((Length::const_px(1000.0), Length::const_px(375.0)))
        .padding(10.0)
        .background_color(ZYNC_800),
    )
    .main_axis_alignment(MainAxisAlignment::Center)
    .background_color(ZYNC_900)
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        AppState::default(),
        app_logic,
        WindowOptions::new("File Tagger"),
    );
    app.run_in(EventLoop::with_user_event())
}
