use file_tagger_internals::{
    ActiveOverlay, ActiveView, Edit, Launcher, SearchMenu, SearchResults, TransactionApi,
    UnrecoverableError,
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
pub struct NoTransactionData {
    pub assets: Assets,
}

/// Gets put in the dyn trait box in the active view type
#[derive(Debug)]
pub struct TransactionData {
    pub assets: Assets,
    pub active_transaction: TransactionApi,
}

impl From<TransactionData> for NoTransactionData {
    fn from(data: TransactionData) -> Self {
        Self {
            assets: data.assets,
        }
    }
}

pub trait AppData {
    type Data: AnyDebug + 'static;
    fn data(&self) -> &Self::Data;
    fn data_mut(&mut self) -> &mut Self::Data;
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

impl_app_data!(UnrecoverableError, NoTransactionData);
impl_app_data!(Launcher, NoTransactionData);
impl_app_data!(SearchMenu, NoTransactionData);
impl_app_data!(SearchResults, NoTransactionData);
impl_app_data!(Edit, TransactionData);

trait MapAppData {
    fn map_data(self, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug>;
}

impl<From, To> MapAppData for (&From, &To)
where
    From: AppData,
    To: AppData,
    From::Data: Into<To::Data>,
{
    fn map_data(self, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
        let data = data
            .downcast::<From::Data>()
            .expect("Type of dynamic data changed unexpectedly");
        let data: To::Data = (*data).into();
        Box::new(data)
    }
}

// Manual impl for the two special cases, we would need negative trait bounds (i.e. `where From: !StateTransition<Next = To>`) to generalize these
impl MapAppData for (&SearchMenu, &Edit) {
    fn map_data(self, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
        let data = data
            .downcast::<NoTransactionData>()
            .expect("Type of dynamic data changed unexpectedly");
        let data = TransactionData {
            assets: data.assets,
            active_transaction: self.1.database.inner_db_handle().initialize_transaction(),
        };
        Box::new(data)
    }
}

impl MapAppData for (&SearchResults, &Edit) {
    fn map_data(self, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
        let data = data
            .downcast::<NoTransactionData>()
            .expect("Type of dynamic data changed unexpectedly");
        let data = TransactionData {
            assets: data.assets,
            active_transaction: self.1.database.inner_db_handle().initialize_transaction(),
        };
        Box::new(data)
    }
}

// Combinatorics yayy
macro_rules! generate_map_data_fn {
    (variants: [$($variant:ident),* $(,)*], unreachable: [$($from:ident -> $to:ident),* $(,)*] $(,)*) => {
        generate_map_data_fn!(@0 $($variant),* | ($($variant),*) | ($(($from, $to)),*));
    };
    (@0 $($variant1:ident),* | $variant2:tt | $unreachable:tt) => {
        generate_map_data_fn!(@1 [$(($variant1, $variant2)),*] | $unreachable | $);
    };
    (@1 [$(($variant1:ident, ($($variant2:ident),*))),*] | ($(($from:ident, $to:ident)),*) | $d:tt) => {
        fn map_data(from: &ActiveView, to: &ActiveView, data: Box<dyn AnyDebug>) -> Box<dyn AnyDebug> {
            macro_rules! generated_macro_to_match_unreachable_transitions {
                $(($d from_inner:ident $d to_inner:ident $from $to) => { unreachable!() };)*
                ($d from_inner:ident $d to_inner:ident $d from:ident $d to:ident) => { ($from_inner, $to_inner).map_data(data) };
            }

            use ActiveView::*;
            match (from, to) {
                $($(($variant1(_from), $variant2(_to)) => { generated_macro_to_match_unreachable_transitions!(_from _to $variant1 $variant2) } )*)*
            }
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
    unreachable: [
        UnrecoverableError -> Edit,
        Launcher -> Edit,
    ],
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
    let data = NoTransactionData {
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
    // if let Some(overlay) = overlay {
    //     Either::A(zstack((view, overlay)))
    // } else {
    //     Either::B(view)
    // }
}
