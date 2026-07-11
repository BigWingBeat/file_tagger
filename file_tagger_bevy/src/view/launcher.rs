use bevy::prelude::*;
use file_tagger_internals::Launcher;

use crate::state::LensState;

use super::centered_box;

pub fn launcher_view(state: LensState<Launcher>) -> impl Scene + use<> {
    centered_box(bsn_list![Text("Text1"), Text("Text2")])
}
