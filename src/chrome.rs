use pinboard::Pinboard;
use std::sync::LazyLock;

pub(crate) struct ChromeVersion {
    pub(crate) chrome_version: u16,
    pub(crate) timestamp: u32,
}

pub(crate) static CHROME_VERSION: LazyLock<Pinboard<ChromeVersion>> =
    LazyLock::new(|| Pinboard::new_empty());
