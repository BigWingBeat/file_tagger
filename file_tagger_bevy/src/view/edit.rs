use bevy::prelude::*;
use file_tagger_internals::Edit;

use crate::state::LensState;

use super::centered_box;

pub fn edit_view(state: LensState<Edit>) -> impl Scene + use<> {
    centered_box(bsn_list![Text("Text1"), Text("Text2")])
}
