use xilem::{
    WidgetView,
    view::{GridParams, grid, grid_item, prose},
};

struct Entry {
    name: String,
}

pub struct SearchResultsState {
    entries: Vec<Entry>,
}

impl Default for SearchResultsState {
    fn default() -> Self {
        Self {
            entries: vec![
                Entry {
                    name: "file 1".into(),
                },
                Entry {
                    name: "file 2".into(),
                },
                Entry {
                    name: "file 3".into(),
                },
                Entry {
                    name: "file 4".into(),
                },
                Entry {
                    name: "file 5".into(),
                },
                Entry {
                    name: "file 6".into(),
                },
                Entry {
                    name: "file 7".into(),
                },
                Entry {
                    name: "file 8".into(),
                },
                Entry {
                    name: "file 9".into(),
                },
                Entry {
                    name: "file 10".into(),
                },
                Entry {
                    name: "file 11".into(),
                },
                Entry {
                    name: "file 12".into(),
                },
                Entry { name: "a".into() },
                Entry { name: "b".into() },
                Entry { name: "c".into() },
                Entry { name: "d".into() },
                Entry {
                    name: "aaaaaaaaaaaaaaaaaaaaaa".into(),
                },
                Entry {
                    name: "entry name that contains many words".into(),
                },
                Entry { name: "a".into() },
                Entry { name: "b".into() },
                Entry { name: "c".into() },
                Entry { name: "d".into() },
                Entry { name: "a".into() },
                Entry { name: "b".into() },
                Entry { name: "c".into() },
                Entry { name: "d".into() },
            ],
        }
    }
}

pub fn search_results(
    state: &mut SearchResultsState,
) -> impl WidgetView<SearchResultsState> + use<> {
    // TODO: see <https://github.com/linebender/xilem/issues/1785>
    let cols = 4;
    grid(
        state
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                grid_item(
                    prose(entry.name.clone()),
                    GridParams::new((i % cols) as _, (i / cols) as _, 1, 1),
                )
            })
            .collect::<Vec<_>>(),
        cols as _,
        4,
    )
}
