use std::any::Any;

use file_tagger_internals::{
    ActiveOverlay, ActiveView, Edit, Launcher, SearchMenu, SearchResults, TransactionApi,
    UnrecoverableError,
};
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
pub struct NoTransactionData {
    pub assets: Assets,
}

/// Gets put in the dyn trait box in the active view type
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
    type Data: 'static;
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
    fn map_data(self, data: Box<dyn Any>) -> Box<dyn Any>;
}

impl<From, To> MapAppData for (&From, &To)
where
    From: AppData,
    To: AppData,
    From::Data: Into<To::Data>,
{
    fn map_data(self, data: Box<dyn Any>) -> Box<dyn Any> {
        let data = data
            .downcast::<From::Data>()
            .expect("Type of dynamic data changed unexpectedly");
        let data: To::Data = (*data).into();
        Box::new(data)
    }
}

impl MapAppData for (&SearchMenu, &Edit) {
    fn map_data(self, data: Box<dyn Any>) -> Box<dyn Any> {
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
    fn map_data(self, data: Box<dyn Any>) -> Box<dyn Any> {
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

fn map_data(from: &ActiveView, to: &ActiveView, data: Box<dyn Any>) -> Box<dyn Any> {
    match (from, to) {
        (ActiveView::UnrecoverableError(from), ActiveView::UnrecoverableError(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::UnrecoverableError(from), ActiveView::Launcher(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::UnrecoverableError(from), ActiveView::SearchMenu(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::UnrecoverableError(from), ActiveView::SearchResults(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::UnrecoverableError(from), ActiveView::Edit(to)) => unreachable!(),
        (ActiveView::Launcher(from), ActiveView::UnrecoverableError(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::Launcher(from), ActiveView::Launcher(to)) => (from, to).map_data(data),
        (ActiveView::Launcher(from), ActiveView::SearchMenu(to)) => (from, to).map_data(data),
        (ActiveView::Launcher(from), ActiveView::SearchResults(to)) => (from, to).map_data(data),
        (ActiveView::Launcher(from), ActiveView::Edit(to)) => unreachable!(),
        (ActiveView::SearchMenu(from), ActiveView::UnrecoverableError(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::SearchMenu(from), ActiveView::Launcher(to)) => (from, to).map_data(data),
        (ActiveView::SearchMenu(from), ActiveView::SearchMenu(to)) => (from, to).map_data(data),
        (ActiveView::SearchMenu(from), ActiveView::SearchResults(to)) => (from, to).map_data(data),
        (ActiveView::SearchMenu(from), ActiveView::Edit(to)) => (from, to).map_data(data),
        (ActiveView::SearchResults(from), ActiveView::UnrecoverableError(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::SearchResults(from), ActiveView::Launcher(to)) => (from, to).map_data(data),
        (ActiveView::SearchResults(from), ActiveView::SearchMenu(to)) => (from, to).map_data(data),
        (ActiveView::SearchResults(from), ActiveView::SearchResults(to)) => {
            (from, to).map_data(data)
        }
        (ActiveView::SearchResults(from), ActiveView::Edit(to)) => (from, to).map_data(data),
        (ActiveView::Edit(from), ActiveView::UnrecoverableError(to)) => (from, to).map_data(data),
        (ActiveView::Edit(from), ActiveView::Launcher(to)) => (from, to).map_data(data),
        (ActiveView::Edit(from), ActiveView::SearchMenu(to)) => (from, to).map_data(data),
        (ActiveView::Edit(from), ActiveView::SearchResults(to)) => (from, to).map_data(data),
        (ActiveView::Edit(from), ActiveView::Edit(to)) => (from, to).map_data(data),
    }
}

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
                state.set_active_overlay(ActiveOverlay::None);
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
