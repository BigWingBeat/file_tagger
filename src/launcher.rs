use std::path::PathBuf;

use xilem::{
    FontWeight, WidgetView,
    masonry::{
        layout::{Dim, Length},
        properties::{Dimensions, Gap, LineBreaking},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, MainAxisAlignment, button, flex_col, flex_item, flex_row,
        label, portal, text_button,
    },
};

struct RecentFolder {
    name: String,
    path: PathBuf,
}

pub struct LauncherState {
    recent_folders: Vec<RecentFolder>,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
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
            text_button("Open", |state: &mut LauncherState| {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open Database As Folder")
                    .pick_folder()
                {
                    state.recent_folders.push(RecentFolder {
                        name: path.file_name().unwrap().to_string_lossy().into_owned(),
                        path,
                    });
                }
            })
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
            text_button("Create", |state: &mut LauncherState| {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Create New Folder And Database")
                    .save_file()
                {
                    state.recent_folders.push(RecentFolder {
                        name: path.file_name().unwrap().to_string_lossy().into_owned(),
                        path,
                    });
                }
            })
            .dims(Dimensions::height(Dim::Stretch))
            .flex(0.4),
        )),
    ))
    .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
}

pub fn launcher(state: &mut LauncherState) -> impl WidgetView<LauncherState> + use<> {
    flex_row((
        flex_item(recent_list_portal(state), 0.5),
        flex_item(open_create_buttons(state), 0.5),
    ))
}
