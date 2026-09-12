use std::sync::LazyLock;

use crate::Tag;

/// Well-known tag that is applied to tag entries.
/// - Tag Data: String value, the tag name of the tag entry it is applied to
pub static META_TAG: LazyLock<Tag> = LazyLock::new(|| "tag_name".into());

/// Well-known tag that is applied to tag entries.
/// - Tag Data: [`TagDataType`] value, the kind of data that this tag has
pub static META_DATA: LazyLock<Tag> = LazyLock::new(|| "tag_data_type".into());
