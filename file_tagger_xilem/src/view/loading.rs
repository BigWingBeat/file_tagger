use file_tagger_internals::Loading;
use xilem::{
    WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{flex_col, prose, spinner},
};

use crate::view::centered_flex_box;

pub fn loading_view(state: &mut Loading) -> impl WidgetView<Loading> + use<> {
    centered_flex_box(flex_col((spinner().dims(40.px()), prose("Loading..."))))
}
