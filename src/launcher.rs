use std::path::PathBuf;

use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::{Dimensions, Gap, LineBreaking},
        theme::{ZYNC_600, ZYNC_800, ZYNC_900},
    },
    palette::css::TRANSPARENT,
    style::{Padding, Style},
    view::{
        CrossAxisAlignment, FlexExt, MainAxisAlignment, button, flex_col, flex_item, flex_row,
        label, portal, text_button, text_input,
    },
};

struct RecentFolder {
    name: String,
    path: PathBuf,
}

pub struct LauncherState {
    active_folder: String,
    search_text: String,
    recent_folders: Vec<RecentFolder>,
}

impl Default for LauncherState {
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
                RecentFolder {
                    name: "Testing extremely long path and name".into(),
                    path: "/home/bigwingbeat/Documents/game assets/rain world/Assets/Resources/atlases/".into(),
                },
            ],
        }
    }
}

fn active_folder_name(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    label(state.active_folder.clone())
        .weight(FontWeight::BOLD)
        .text_size(20.0)
        .dims(Dimensions::width(Dim::Stretch))
}

fn search_bar(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    // Hoist the `text_input` styling to the enclosing `flex_row` so the button looks like it's inside the text box
    flex_row((
        text_input(
            state.search_text.clone(),
            |state: &mut LauncherState, text| state.search_text = text,
        )
        .placeholder("Search files by tag")
        .border_width(0.0)
        .background(TRANSPARENT)
        .flex(1.0),
        text_button("🔍", |_| {})
            .corner_radius(f64::INFINITY) // circle
            .border_width(0.0)
            // Manually tuned padding to make it look centered and circular
            .padding(Padding {
                top: 5.0,
                ..Padding::horizontal(8.5)
            }),
    ))
    .gap(Length::const_px(1.0))
    // The text input has its own padding, this is just for the button
    .padding(Padding {
        right: 12.0,
        ..Padding::vertical(2.0)
    })
    // Border and corner radius the same as the text input
    .border(ZYNC_600, 1.0)
    .corner_radius(4.0)
}

fn recent_list(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    // The width of these buttons shouldn't depend on the size of the displayed paths, as those will change.
    // Instead, they are always as wide as possible
    flex_col(
        state
            .recent_folders
            .iter()
            .map(|folder| {
                // TODO: highlight on hover
                button(
                    flex_col((
                        label(folder.name.clone())
                            .weight(FontWeight::BOLD)
                            .line_break_mode(LineBreaking::WordWrap),
                        label(folder.path.to_string_lossy())
                            .line_break_mode(LineBreaking::WordWrap),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .gap(Length::const_px(0.0)),
                    |_| {},
                )
                .border_width(0.0)
            })
            .collect::<Vec<_>>(),
    )
    .gap(Length::const_px(1.0))
    .dims(Dimensions::width(Dim::Stretch))
}

fn recent_list_portal(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    flex_col((
        label("Open Recent")
            .weight(FontWeight::BOLD)
            .text_size(20.0),
        portal(recent_list(state)),
    ))
}

/// The "open" button selects an existing folder and either creates a new database in that folder, or opens a database
/// that already exists there.
/// The "create" button selects a folder, and creates a *new* empty folder there, with a specified name, as well as
/// creating a new database in the new folder.
fn open_create_buttons(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    // These buttons should be the same width
    flex_col((
        flex_row((
            flex_col((
                label("Open Folder As Database").weight(FontWeight::BOLD),
                label("Create or open a database in a folder"),
            ))
            .cross_axis_alignment(CrossAxisAlignment::End)
            .gap(Gap::ZERO)
            .flex(0.5),
            text_button("Open", |_: &mut LauncherState| {})
                .dims(Dimensions::height(Dim::Stretch))
                .flex(0.4),
        )),
        flex_row((
            flex_col((
                label("Create New Database").weight(FontWeight::BOLD),
                label("Create a new folder and database"),
            ))
            .cross_axis_alignment(CrossAxisAlignment::End)
            .gap(Gap::ZERO)
            .flex(0.5),
            text_button("Create", |_: &mut LauncherState| {})
                .dims(Dimensions::height(Dim::Stretch))
                .flex(0.4),
        )),
    ))
    .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
}

pub fn launcher_view(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    // Center the inner `flex_col`
    flex_col(
        // Centered and fixed size
        flex_col((
            active_folder_name(state),
            search_bar(state),
            flex_row((
                flex_item(recent_list_portal(state), 0.5),
                flex_item(open_create_buttons(state), 0.5),
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
