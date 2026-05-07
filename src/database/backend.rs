cfg_select! {
    feature = "backend_fjall" => {
        mod fjall;
        use fjall as backend;
    }
    feature = "backend_sled" => {
        mod sled;
        use sled as backend;
    }
}

use std::path::Path;

pub use backend::{Buffer, Builder, Database, Result, Table};

pub trait BuilderImpl {
    type Database;
    fn new_with_path(path: impl AsRef<Path>) -> Self;
    fn cache_size(self, bytes: u64) -> Self;
    fn compression(self, yes: bool) -> Self;
    fn temporary(self, yes: bool) -> Self;
    fn open(self) -> Result<Self::Database>;
}

pub trait DatabaseImpl {
    type Table;
    fn open_table(&self, name: &str) -> Result<Self::Table>;
}

pub trait TableImpl {
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>>;
    fn insert(&self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()>;
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
}

pub fn open(path: impl AsRef<Path>) -> Result<Database> {
    Builder::new_with_path(path)
        .compression(true)
        .temporary(false)
        .open()
}

pub fn open_temporary() -> Result<Database> {
    let id = std::process::id() as u64;
    let mut range = id..;
    const MAX_ATTEMPTS: usize = 3;
    for id in range.by_ref().take(MAX_ATTEMPTS - 1) {
        let result = try_open_temporary(id);
        if result.is_ok() {
            return result;
        }
    }
    try_open_temporary(range.start)
}

fn try_open_temporary(id: u64) -> Result<Database> {
    const UPPER_PHI: u64 = 0x9e37_79b9_0000_0001;
    let hash = id.wrapping_mul(UPPER_PHI).rotate_left(32);
    let mut path = std::env::temp_dir();
    path.push(format!("{hash:x}"));
    Builder::new_with_path(path)
        .compression(false)
        .temporary(true)
        .open()
}
