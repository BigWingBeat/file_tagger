use std::sync::Arc;

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

#[cfg(test)]
mod tests {
    #[test]
    fn assets_are_valid() {
        let _ = super::Assets::new();
    }
}
