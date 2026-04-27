use std::path::PathBuf;

use xilem::{
    EventLoop, FontWeight, TextAlign, WidgetView, WindowOptions, Xilem,
    masonry::{
        peniko::color::{AlphaColor, HueDirection},
        properties::types::Length,
        theme::{ZYNC_500, ZYNC_700, ZYNC_800, ZYNC_900},
    },
    palette::css::TRANSPARENT,
    style::Style,
    view::{
        Axis, CrossAxisAlignment, MainAxisAlignment, flex, flex_col, flex_row, label, portal,
        prose, sized_box, text_button, text_input, transformed,
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
    flex_col(
        sized_box(
            flex_col((
                sized_box(
                    label(state.active_folder.clone())
                        .weight(FontWeight::BOLD)
                        .text_size(20.0),
                )
                .expand_width(),
                flex_row((
                    text_input(state.search_text.clone(), |state: &mut AppState, text| {
                        state.search_text = text
                    })
                    .placeholder("Search posts by tag"),
                    transformed(
                        text_button("🔍", |_| {})
                            .corner_radius(25.0)
                            .border_width(0.0),
                    )
                    .translate((-80.0, 0.0)),
                )),
                flex_row((
                    flex_col((
                        label("Open Recent")
                            .weight(FontWeight::BOLD)
                            .text_size(20.0),
                        portal(
                            flex_col(
                                state
                                    .recent_folders
                                    .iter()
                                    .enumerate()
                                    .map(|(i, folder)| {
                                        flex_col((
                                            label(folder.name.clone()).weight(FontWeight::BOLD),
                                            label(folder.path.to_string_lossy()),
                                        ))
                                        .cross_axis_alignment(CrossAxisAlignment::Start)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .cross_axis_alignment(CrossAxisAlignment::Start),
                        ),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start),
                    flex_col((
                        text_button("Open Database From Folder", |data: &mut AppState| {}),
                        text_button("Create New Database", |data: &mut AppState| {}),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Fill),
                ))
                .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                .must_fill_major_axis(true),
            ))
            .main_axis_alignment(MainAxisAlignment::Center),
        )
        .width(Length::const_px(600.0))
        .height(Length::const_px(200.0))
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
