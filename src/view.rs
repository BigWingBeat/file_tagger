use xilem::{
    WidgetView,
    masonry::{
        layout::Length,
        theme::{ZYNC_800, ZYNC_900},
    },
    style::Style,
    view::{Flex, FlexSequence, MainAxisAlignment, flex_col},
};

pub fn centered_box<Seq, State>(seq: Seq) -> impl WidgetView<State> + use<Seq, State>
where
    State: 'static,
    Seq: FlexSequence<State>,
    Flex<Seq, State>: WidgetView<State>,
    <Flex<Seq, State> as WidgetView<State>>::Widget: Sized,
{
    // Center the inner `flex_col`
    flex_col(
        // Centered and fixed size
        flex_col(seq)
            .main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .dims((Length::const_px(1000.0), Length::const_px(375.0)))
            .padding(10.0)
            .background_color(ZYNC_800),
    )
    .main_axis_alignment(MainAxisAlignment::Center)
    .background_color(ZYNC_900)
}
