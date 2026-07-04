use std::ops::{Deref, DerefMut};

use file_tagger_internals::{
    ActiveOverlay, ActiveView, Edit, Launcher, Loading, SearchMenu, SearchResults, TransactionApi,
};
use xilem::{AnyWidgetView, WidgetView, core::lens, view::zstack};

use crate::{
    assets::Assets,
    view::{
        edit_view, error_view, launcher_view, loading_view, search_menu_view, search_results_view,
        spinner_view,
    },
};

/// State wrapper so we can store xilem-specific state
pub struct XilemAppState {
    pub state: ActiveView,
    pub assets: Assets,
    pub active_transaction: TransactionApi,
}

pub trait LensView {
    type ParentState;
    fn view(&mut self) -> Box<AnyWidgetView<Self::ParentState>>;
}

macro_rules! impl_lens_view {
    ($variant:ident, $fn:ident) => {
        impl LensView for $variant {
            type ParentState = XilemAppState;
            fn view(&mut self) -> Box<AnyWidgetView<XilemAppState>> {
                lens($fn, |state: &mut XilemAppState| {
                    match &mut **state {
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

impl_lens_view!(Loading, loading_view);
impl_lens_view!(Launcher, launcher_view);
impl_lens_view!(SearchMenu, search_menu_view);
impl_lens_view!(SearchResults, search_results_view);
impl_lens_view!(Edit, edit_view);

impl LensView for XilemAppState {
    type ParentState = Self;
    fn view(&mut self) -> Box<AnyWidgetView<XilemAppState>> {
        match &mut **self {
            ActiveView::Loading(loading) => loading.view(),
            ActiveView::Launcher(launcher) => launcher.view(),
            ActiveView::SearchMenu(search_menu) => search_menu.view(),
            ActiveView::SearchResults(search_results) => search_results.view(),
            ActiveView::Edit(edit) => edit.view(),
        }
    }
}

impl Deref for XilemAppState {
    type Target = ActiveView;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for XilemAppState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl XilemAppState {
    pub fn new() -> Self {
        Self {
            state: ActiveView::new(),
            assets: Assets::new(),
            active_transaction: TransactionApi::new_disconnected(),
        }
    }

    pub fn app_logic(&mut self) -> impl WidgetView<XilemAppState> + use<> {
        // `update_to_next` must be called before the view is constructed to avoid state desync
        // If the state variant changes in-between then and `teardown`, we will hit the above `unreachable!`s
        self.update_to_next();
        let view = self.view();

        let overlay = match self.active_overlay() {
            ActiveOverlay::None => None,
            ActiveOverlay::Error(report) => Some(
                error_view(report, |state: &mut XilemAppState| {
                    state.set_active_overlay(ActiveOverlay::None);
                })
                .boxed(),
            ),
            ActiveOverlay::Spinner => Some(spinner_view(self).boxed()),
        };

        zstack((view, overlay))
        // if let Some(overlay) = overlay {
        //     Either::A(zstack((view, overlay)))
        // } else {
        //     Either::B(view)
        // }
    }
}
