use std::{fmt::Debug, sync::Arc};

use usvg::Tree;

/// It's MIT?
const _LICENSE_TABLER_ICONS: &str = include_str!("../assets/license-tabler-icons");

const TAG_SVG: &str = include_str!("../assets/tag.svg");

#[derive(Clone)]
pub struct Assets {
    pub tag: Arc<Tree>,
}

impl Assets {
    pub fn new() -> Self {
        // Asset data is embedded at compile time via `include_str!` so unwrapping is probably fine
        let tag = Tree::from_str(TAG_SVG, &Default::default()).unwrap();
        Self { tag: Arc::new(tag) }
    }
}

/// Manual debug impl to avoid debug printing the SVG `tree`s, as that is extremely verbose and unhelpful
impl Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn assets_are_valid() {
        let _ = super::Assets::new();
    }
}
