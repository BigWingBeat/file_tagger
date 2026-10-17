use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, types::Null};

mod tags;
mod types;

use crate::{
    DB_FILE_NAME,
    app_data::RecentFolder,
    sqlite::Database,
    tags_db::tags::{META_DATA, META_TAG},
};

pub use types::{
    AnyRange, AnyRangeError, Entry, RangeError, Tag, TagData, TagDataError, TagDataType,
    TagDataTypeError,
};

/// Marker struct that should be impossible for any code outside this module to obtain an instance of
#[derive(Clone)]
pub struct NoTransaction(());

/// Marker struct that should be impossible for any code outside this module to obtain an instance of
#[derive(Clone)]
pub struct Transaction(());

/// Transaction-aware database state
#[derive(Clone)]
pub struct DatabaseState<Transaction = NoTransaction> {
    db: TagsDatabase<Transaction>,
    folder: RecentFolder,
}

impl<T> DatabaseState<T> {
    fn new(db: TagsDatabase<T>, folder: RecentFolder) -> Self {
        Self { db, folder }
    }

    pub fn active_folder(&self) -> &RecentFolder {
        &self.folder
    }

    fn change_transaction_state<Dest>(
        self,
        f: impl FnOnce(
            TagsDatabase<T>,
        ) -> Result<TagsDatabase<Dest>, (TagsDatabase<T>, rusqlite::Error)>,
    ) -> Result<DatabaseState<Dest>, (Self, rusqlite::Error)> {
        let folder = self.folder;
        match f(self.db) {
            Ok(db) => Ok(DatabaseState::new(db, folder)),
            Err((db, e)) => Err((Self::new(db, folder), e)),
        }
    }

    pub fn generate_entry(&mut self) -> Entry {
        self.db.generate_entry()
    }

    pub fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.db.get_tag_entry(tag)
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.db.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.db.search_tags_names_by_prefix(prefix)
    }
}

impl DatabaseState {
    pub fn create_temporary() -> rusqlite::Result<Self> {
        TagsDatabase::open_temporary().map(|db| Self::new(db, PathBuf::default().into()))
    }

    pub fn open_in_folder(folder: RecentFolder) -> rusqlite::Result<Self> {
        let mut db_file = folder.clone();
        db_file.path.push(DB_FILE_NAME);
        TagsDatabase::open(db_file.path).map(|db| Self::new(db, folder))
    }

    pub fn begin_transaction(self) -> Result<DatabaseState<Transaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabase::begin_transaction)
    }
}

impl DatabaseState<Transaction> {
    pub fn commit(self) -> Result<DatabaseState<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabase::commit)
    }

    pub fn rollback(self) -> Result<DatabaseState<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabase::rollback)
    }
}

/// Rust fn wrappers around SQL statements
#[derive(Clone)]
pub struct TagsDatabase<Transaction> {
    db: Database,
    transaction: Transaction,
}

impl TagsDatabase<NoTransaction> {
    fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        Database::open(path).and_then(Self::initialize_sqlite_schema)
    }

    fn open_temporary() -> rusqlite::Result<Self> {
        Database::open_temporary().and_then(Self::initialize_sqlite_schema)
    }

    fn initialize_sqlite_schema(db: Database) -> rusqlite::Result<Self> {
        const SCHEMA: &str = include_str!("../sql/schema.sql");
        db.execute_batch(SCHEMA).map(|_| Self {
            db,
            transaction: NoTransaction(()),
        })
    }

    fn begin_transaction(self) -> Result<TagsDatabase<Transaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state("BEGIN", Transaction(()))
    }
}

impl TagsDatabase<Transaction> {
    fn commit(self) -> Result<TagsDatabase<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state("COMMIT", NoTransaction(()))
    }

    fn rollback(self) -> Result<TagsDatabase<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state("ROLLBACK", NoTransaction(()))
    }
}

impl<T> TagsDatabase<T> {
    fn change_transaction_state<Dest>(
        self,
        sql: &str,
        transaction: Dest,
    ) -> Result<TagsDatabase<Dest>, (Self, rusqlite::Error)> {
        match self.db.execute(sql, ()) {
            Ok(_) => Ok(TagsDatabase {
                db: self.db,
                transaction,
            }),
            Err(e) => Err((self, e)),
        }
    }

    fn generate_entry(&mut self) -> rusqlite::Result<Entry> {
        self.db
            .prepare_cached("INSERT INTO entries DEFAULT VALUES RETURNING entry")
            .and_then(|mut stmt| stmt.query_one((), |row| row.get::<_, i64>(0).map(Into::into)))
    }

    /// If this tag already exists, this is effectively a no-op.
    fn create_tag(&mut self, tag: Tag) -> rusqlite::Result<()> {
        let savepoint = self.db.savepoint_with_name("create_tag")?;
        let entry = self.generate_entry()?;
        self.db
            .prepare_cached("INSERT INTO tags VALUES(?, ?)")
            .and_then(|mut stmt| stmt.execute((entry, tag)))?;
        savepoint.commit()
    }

    /// Returns `None` if the tag does not exist.
    fn get_tag_entry(&self, tag: &Tag) -> rusqlite::Result<Option<Entry>> {
        self.db
            .prepare_cached("SELECT tag_entry FROM tags WHERE tag = ?")
            .and_then(|mut stmt| {
                stmt.query_one([tag], |row| row.get::<_, i64>(0).map(Into::into))
                    .optional()
            })
    }

    fn tag_exists(&self, tag: &Tag) -> rusqlite::Result<bool> {
        self.db
            .prepare_cached("VALUES(EXISTS(SELECT * from tags WHERE tag = ?))")
            .and_then(|mut stmt| stmt.query_one([tag], |row| row.get::<_, bool>(0)))
    }

    /// Returns `None` if the tag entry does not exist
    fn get_tag_metadata(&self, tag_entry: Entry) -> rusqlite::Result<Option<TagDataType>> {
        self.db
            .prepare_cached("SELECT data FROM tag_data WHERE (entry, tag) = (?, ?)")
            .and_then(|mut stmt| {
                stmt.query_one((tag_entry, *META_DATA), |row| row.get::<_, TagDataType>(0))
                    .optional()
            })
    }

    /// Returns `None` if the given tag is not applied to the given entry
    fn get_tag_data(&self, entry: Entry, tag: Tag) -> rusqlite::Result<Option<TagData>> {
        self.db
            .prepare_cached("SELECT data FROM tag_data WHERE (entry, tag) = (?, ?)")
            .and_then(|mut stmt| {
                stmt.query_one((entry, tag), |row| row.get::<_, TagData>(0))
                    .optional()
            })
    }

    /// If this tag is already applied to this entry, this is effectively a no-op. For tags with values, it updates the value.
    ///
    /// You can think of this as creating a new instance of the tag.
    fn insert_tag_on_entry(&mut self, tag: Tag, entry: Entry) -> rusqlite::Result<()> {
        let data = Null;
        self.db.prepare_cached("INSERT INTO tag_data VALUES(?, ?, ?) ON CONFLICT DO UPDATE SET data = excluded.data")
        .and_then(|mut stmt| stmt.execute((entry, tag, data))).map(|_| ())
    }

    fn insert_tag_data_on_entry(
        &mut self,
        entry: Entry,
        tag: Tag,
        data: TagData,
    ) -> rusqlite::Result<()> {
        self.db.prepare_cached("INSERT INTO tag_data VALUES(?, ?, ?) ON CONFLICT DO UPDATE SET data = excluded.data")
        .and_then(|mut stmt| stmt.execute((entry, tag, data))).map(|_| ())
    }

    /// If the tag does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_tag(&mut self, tag: Tag) -> rusqlite::Result<()> {
        self.get_tag_entry(&tag)
            .and_then(|entry| entry.map_or(Ok(()), |entry| self.delete_entry(entry)))
    }

    /// If the entry does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_entry(&mut self, entry: Entry) -> rusqlite::Result<()> {
        self.db
            .prepare_cached("DELETE FROM entries WHERE entry = ?")
            .and_then(|mut stmt| stmt.execute([entry]))
            .map(|_| ())
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = rusqlite::Result<Tag>> {
        self.db
            .prepare_cached("SELECT tag FROM tags WHERE instr(tag, ?) = 1")
            .and_then(|mut stmt| stmt.query([prefix]))
    }
}
