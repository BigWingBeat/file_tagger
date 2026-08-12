use std::any::TypeId;

use file_tagger_internals::{
    ActiveOverlay, ActiveView, Edit, Launcher, SearchMenu, SearchResults, UnrecoverableError,
};

use anymore::AnyDebug;
use xilem::{
    AnyWidgetView, WidgetView,
    core::lens,
    view::{label, zstack},
};

use crate::{
    assets::Assets,
    view::{
        edit_view, error_view, launcher_view, search_menu_view, search_results_view, spinner_view,
    },
};

/// Gets put in the dyn trait box in the active view type
#[derive(Debug)]
pub struct XilemAppData {
    pub assets: Assets,
}

pub trait AppData {
    type Data: AnyDebug + 'static;
    fn data(&self) -> &Self::Data;
    fn data_mut(&mut self) -> &mut Self::Data;

    fn map_data<To>(data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug>
    where
        To: AppData,
        Self::Data: Into<To::Data>,
    {
        let data = data
            .downcast::<Self::Data>()
            .expect("Type of dynamic data changed unexpectedly");
        let data: To::Data = (*data).into();
        Box::new(data)
    }
}

macro_rules! impl_app_data {
    ($variant:ty, $data:ty) => {
        impl AppData for $variant {
            type Data = $data;

            fn data(&self) -> &Self::Data {
                self.data
                    .downcast_ref()
                    .expect("Type of dynamic data changed unexpectedly")
            }

            fn data_mut(&mut self) -> &mut Self::Data {
                self.data
                    .downcast_mut()
                    .expect("Type of dynamic data changed unexpectedly")
            }
        }
    };
}

impl_app_data!(UnrecoverableError, XilemAppData);
impl_app_data!(Launcher, XilemAppData);
impl_app_data!(SearchMenu, XilemAppData);
impl_app_data!(SearchResults, XilemAppData);
impl_app_data!(Edit, XilemAppData);

// Combinatorics yayy
macro_rules! generate_map_data_fn {
    (variants: [$($variant:ident),* $(,)*], unreachable: [$($from:ident -> $to:ident),* $(,)*] $(,)*) => {
        generate_map_data_fn!(@0 $($variant),* | ($($variant),*) | ($(($from, $to)),*));
    };
    (@0 $($variant1:ident),* | $variant2:tt | $unreachable:tt) => {
        generate_map_data_fn!(@1 [$(($variant1, $variant2)),*] | $unreachable | $);
    };
    (@1 [$(($variant1:ident, ($($variant2:ident),*))),*] | ($(($from:ident, $to:ident)),*) | $d:tt) => {
        fn map_data(from: TypeId, to: TypeId, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
            macro_rules! generated_macro_to_match_unreachable_transitions {
                $(($from $to) => { unreachable!() };)*
                ($d from:ident $d to:ident) => { <$d from as AppData>::map_data::<$d to>(data) };
            }

            $(
                $(
                    if from == TypeId::of::<$variant1>() && to == TypeId::of::<$variant2>() {
                        return generated_macro_to_match_unreachable_transitions!($variant1 $variant2);
                    }
                )*
            )*

            unreachable!("You probably forgot to put a state type in the variants list")
        }
    }
}

generate_map_data_fn!(
    variants: [
        UnrecoverableError,
        Launcher,
        SearchMenu,
        SearchResults,
        Edit,
    ],
    // Formerly used: for if a state has a different dyn data type that means some state transitions have no possible conversion
    unreachable: [],
);

pub trait LensView {
    type ParentState;
    fn view(&mut self) -> Box<AnyWidgetView<Self::ParentState>>;
}

macro_rules! impl_lens_view {
    ($variant:ident, $fn:ident) => {
        impl LensView for $variant {
            type ParentState = ActiveView;
            fn view(&mut self) -> Box<AnyWidgetView<ActiveView>> {
                lens($fn, |state: &mut ActiveView| {
                    match state {
                        ActiveView::$variant(inner) => inner,
                        // See <https://github.com/linebender/xilem/issues/1418>
                        _ => unreachable!(
                            "State was changed in-between view construction and view build/rebuild/message"
                        ),
                    }
                })
                .boxed()
            }
        }
    };
}

impl_lens_view!(Launcher, launcher_view);
impl_lens_view!(SearchMenu, search_menu_view);
impl_lens_view!(SearchResults, search_results_view);
impl_lens_view!(Edit, edit_view);

impl LensView for UnrecoverableError {
    type ParentState = ActiveView;
    fn view(&mut self) -> Box<AnyWidgetView<Self::ParentState>> {
        label("").boxed()
    }
}

impl LensView for ActiveView {
    type ParentState = Self;
    fn view(&mut self) -> Box<AnyWidgetView<ActiveView>> {
        match self {
            ActiveView::UnrecoverableError(error) => error.view(),
            ActiveView::Launcher(launcher) => launcher.view(),
            ActiveView::SearchMenu(search_menu) => search_menu.view(),
            ActiveView::SearchResults(search_results) => search_results.view(),
            ActiveView::Edit(edit) => edit.view(),
        }
    }
}

pub type AppState = ActiveView;

pub fn init_state() -> AppState {
    let data = XilemAppData {
        assets: Assets::new(),
    };
    AppState::new(data)
}

pub fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // `update_to_next` must be called before the view is constructed to avoid state desync
    // If the state variant changes in-between then and `teardown`, we will hit the above `unreachable!`s
    state.update_to_next(map_data);

    let view = state.view();
    let overlay = match state.active_overlay() {
        ActiveOverlay::None => None,
        ActiveOverlay::Error(report) => Some(
            error_view(report, |state: &mut AppState| {
                if let ActiveView::UnrecoverableError(_) = state {
                    std::process::exit(1)
                } else {
                    state.set_active_overlay(ActiveOverlay::None)
                }
            })
            .boxed(),
        ),
        ActiveOverlay::Spinner => Some(spinner_view(state).boxed()),
    };

    zstack((view, overlay))
}
