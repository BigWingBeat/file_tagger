use std::sync::Arc;

use usvg::Tree;

/// It's MIT?
const _LICENSE_TABLER_ICONS: &str = include_str!("../assets/license-tabler-icons");

const TAG_SVG: &str = include_str!("../assets/tag.svg");

pub struct Assets {
    pub tag: Arc<Tree>,
}

impl Assets {
    pub fn new() -> Result<Self, usvg::Error> {
        let tag = Tree::from_str(TAG_SVG, &Default::default())?;
        Ok(Self { tag: Arc::new(tag) })
    }
}
