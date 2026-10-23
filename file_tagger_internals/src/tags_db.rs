use std::path::PathBuf;

use rusqlite::{CachedStatement, OptionalExtension, types::Null};

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
pub struct NoTransaction(());

/// Marker struct that should be impossible for any code outside this module to obtain an instance of
pub struct Transaction(());

/// Transaction-aware database state
pub struct DatabaseState<Transaction = NoTransaction> {
    db: Database,
    folder: RecentFolder,
    transaction: Transaction,
}

impl<T> DatabaseState<T> {
    pub fn active_folder(&self) -> &RecentFolder {
        &self.folder
    }

    fn change_transaction_state<Dest>(
        self,
        f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<()>,
        transaction: Dest,
    ) -> Result<DatabaseState<Dest>, (Self, rusqlite::Error)> {
        match f(&self.db) {
            Ok(_) => Ok(DatabaseState {
                db: self.db,
                folder: self.folder,
                transaction,
            }),
            Err(e) => Err((self, e)),
        }
    }
}

impl DatabaseState {
    fn new(mut db: Database, folder: RecentFolder) -> rusqlite::Result<Self> {
        db.initialize_sqlite_schema()?;
        Ok(Self {
            db,
            folder,
            transaction: NoTransaction(()),
        })
    }

    pub fn create_temporary() -> rusqlite::Result<Self> {
        Database::open_temporary().and_then(|db| Self::new(db, PathBuf::default().into()))
    }

    pub fn open_in_folder(folder: RecentFolder) -> rusqlite::Result<Self> {
        let mut db_file = folder.clone();
        db_file.path.push(DB_FILE_NAME);
        Database::open(db_file.path).and_then(|db| Self::new(db, folder))
    }

    pub fn begin_transaction(self) -> Result<DatabaseState<Transaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabaseSqlExt::begin_transaction, Transaction(()))
    }
}

impl DatabaseState<Transaction> {
    pub fn commit(self) -> Result<DatabaseState<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabaseSqlExt::commit_transaction, NoTransaction(()))
    }

    pub fn rollback(self) -> Result<DatabaseState<NoTransaction>, (Self, rusqlite::Error)> {
        self.change_transaction_state(TagsDatabaseSqlExt::rollback_transaction, NoTransaction(()))
    }
}

/// Rust fn wrappers around SQL statements
pub trait TagsDatabaseSqlExt {
    fn execute_batch(&self, sql: &str) -> rusqlite::Result<()>;

    fn prepare_cached(&self, sql: &str) -> rusqlite::Result<rusqlite::CachedStatement<'_>>;

    fn savepoint_with_name<T: Into<String>>(
        &mut self,
        name: T,
    ) -> rusqlite::Result<rusqlite::Savepoint<'_>>;

    fn initialize_sqlite_schema(&mut self) -> rusqlite::Result<()> {
        const SCHEMA: &str = include_str!("../sql/schema.sql");
        self.execute_batch(SCHEMA)
    }

    fn begin_transaction(&self) -> rusqlite::Result<()> {
        self.prepare_cached("BEGIN")
            .and_then(|mut stmt| stmt.execute(()))
            .map(|_| ())
    }

    fn commit_transaction(&self) -> rusqlite::Result<()> {
        self.prepare_cached("COMMIT")
            .and_then(|mut stmt| stmt.execute(()))
            .map(|_| ())
    }

    fn rollback_transaction(&self) -> rusqlite::Result<()> {
        self.prepare_cached("ROLLBACK")
            .and_then(|mut stmt| stmt.execute(()))
            .map(|_| ())
    }

    fn generate_entry(&self) -> rusqlite::Result<Entry> {
        self.prepare_cached("INSERT INTO entries DEFAULT VALUES RETURNING entry")
            .and_then(|mut stmt| stmt.query_one((), |row| row.get::<_, i64>(0).map(Into::into)))
    }

    /// If this tag already exists, this is effectively a no-op.
    fn create_tag(&mut self, tag: Tag) -> rusqlite::Result<()> {
        let savepoint = self.savepoint_with_name("create_tag")?;
        let entry = savepoint.generate_entry()?;
        savepoint
            .prepare_cached("INSERT INTO tags VALUES(?, ?)")
            .and_then(|mut stmt| stmt.execute((entry, tag)))?;
        savepoint.commit()
    }

    /// Returns `None` if the tag does not exist.
    fn get_tag_entry(&self, tag: &Tag) -> rusqlite::Result<Option<Entry>> {
        self.prepare_cached("SELECT tag_entry FROM tags WHERE tag = ?")
            .and_then(|mut stmt| {
                stmt.query_one([tag], |row| row.get::<_, i64>(0).map(Into::into))
                    .optional()
            })
    }

    fn tag_exists(&self, tag: &Tag) -> rusqlite::Result<bool> {
        self.prepare_cached("VALUES(EXISTS(SELECT * from tags WHERE tag = ?))")
            .and_then(|mut stmt| stmt.query_one([tag], |row| row.get::<_, bool>(0)))
    }

    /// Returns `None` if the tag entry does not exist
    fn get_tag_metadata(&self, tag_entry: Entry) -> rusqlite::Result<Option<TagDataType>> {
        self.prepare_cached("SELECT data FROM tag_data WHERE (entry, tag) = (?, ?)")
            .and_then(|mut stmt| {
                stmt.query_one((tag_entry, *META_DATA), |row| row.get::<_, TagDataType>(0))
                    .optional()
            })
    }

    /// Returns `None` if the given tag is not applied to the given entry
    fn get_tag_data(&self, entry: Entry, tag: Tag) -> rusqlite::Result<Option<TagData>> {
        self.prepare_cached("SELECT data FROM tag_data WHERE (entry, tag) = (?, ?)")
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
        self.prepare_cached(
            "INSERT INTO tag_data VALUES(?, ?, ?) ON CONFLICT DO UPDATE SET data = excluded.data",
        )
        .and_then(|mut stmt| stmt.execute((entry, tag, data)))
        .map(|_| ())
    }

    fn insert_tag_data_on_entry(
        &mut self,
        entry: Entry,
        tag: Tag,
        data: TagData,
    ) -> rusqlite::Result<()> {
        self.prepare_cached(
            "INSERT INTO tag_data VALUES(?, ?, ?) ON CONFLICT DO UPDATE SET data = excluded.data",
        )
        .and_then(|mut stmt| stmt.execute((entry, tag, data)))
        .map(|_| ())
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
        self.prepare_cached("DELETE FROM entries WHERE entry = ?")
            .and_then(|mut stmt| stmt.execute([entry]))
            .map(|_| ())
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> rusqlite::Result<TagNamesByPrefixIter<'_>> {
        let mut parameter = String::with_capacity(prefix.len() + 1);
        parameter.push_str(prefix);
        parameter.push('%');
        let stmt = self.prepare_cached("SELECT tag FROM tags WHERE tag LIKE ?")?;
        Ok(TagNamesByPrefixIter { stmt, parameter })
    }
}

impl TagsDatabaseSqlExt for rusqlite::Connection {
    fn execute_batch(&self, sql: &str) -> rusqlite::Result<()> {
        Self::execute_batch(self, sql)
    }

    fn prepare_cached(&self, sql: &str) -> rusqlite::Result<rusqlite::CachedStatement<'_>> {
        Self::prepare_cached(self, sql)
    }

    fn savepoint_with_name<T: Into<String>>(
        &mut self,
        name: T,
    ) -> rusqlite::Result<rusqlite::Savepoint<'_>> {
        Self::savepoint_with_name(self, name)
    }
}

pub struct TagNamesByPrefixIter<'conn> {
    stmt: CachedStatement<'conn>,
    parameter: String,
}

impl<'conn> TagNamesByPrefixIter<'conn> {
    pub fn iter(&mut self) -> rusqlite::Result<impl Iterator<Item = rusqlite::Result<Tag>>> {
        self.stmt
            .query_map([&self.parameter], |row| row.get::<_, Tag>(0))
    }
}
