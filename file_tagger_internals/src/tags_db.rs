use std::{
    cmp::Ordering,
    fmt::{Debug, Display, Formatter},
    path::{Path, PathBuf},
    str::Utf8Error,
    sync::{Arc, Mutex},
};

use estr::Estr;
use miette::IntoDiagnostic;
use scru64::{Scru64Generator, Scru64Id, generator::NodeSpec, id::RangeError};
use thiserror::Error;

use crate::{
    DB_FOLDER_NAME, Transaction,
    app_data::RecentFolder,
    database::{
        self, Buffer, Conflict, Database, DbApi, DeserError, DeserKvError, NoTransaction, Table,
    },
    serde::{
        AsBytes, Bytes, DerefProxy, FromBytes, Prefixable, Reader, Serde, SizeHint, SmallSortedSet,
        UnexpectedEof,
    },
};

#[derive(Clone)]
pub struct DatabaseState {
    database: TagsDatabase,
    folder: RecentFolder,
}

impl DatabaseState {
    pub fn active_folder(&self) -> &RecentFolder {
        &self.folder
    }

    pub fn create_temporary() -> miette::Result<Self> {
        TagsDatabase::open_temporary().map(|database| Self {
            database,
            folder: PathBuf::default().into(),
        })
    }

    pub fn open_in_folder(folder: RecentFolder) -> miette::Result<Self> {
        let mut db_folder = folder.clone();
        db_folder.path.push(DB_FOLDER_NAME);
        std::fs::create_dir(&db_folder.path)
            .or_else(|e| {
                // Ignore error if the dir already exists, as that's fine
                (e.kind() == std::io::ErrorKind::AlreadyExists)
                    .then_some(())
                    .ok_or(e)
            })
            .into_diagnostic()
            .and_then(|_| TagsDatabase::open(db_folder.path))
            .map(|database| Self { database, folder })
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }

    pub fn initialize_transaction(self) -> Result<ActiveTransactionDatabaseState, database::Error> {
        self.database
            .initialize_transaction()
            .map(|database| ActiveTransactionDatabaseState {
                database,
                folder: self.folder,
            })
    }

    pub fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.database.tag_entry_by_name(tag)
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database.search_tags_names_by_prefix(prefix)
    }
}

#[derive(Clone)]
pub struct TagsDatabase<Transaction = NoTransaction> {
    database: Database<Transaction>,
    /// Lookup which entries tags are applied to
    ///
    /// Key: tag name
    /// Value: list of entry IDs (no duplicates)
    ///
    /// Note: This is a separate table from `entries_by_data` to enable prefix lookups on the tag name. That doesn't work with
    /// `entries_by_data` because of the tuple length prefix, as to know the correct length prefix you have to know the full tag
    entries_by_tag: Table<Tag, SmallSortedSet<Entry>>,
    /// Lookup which entries have tags with specific data
    ///
    /// Key: composite (tag name + tag data)
    /// Value: list of entry IDs (no duplicates)
    ///
    /// Note: data in key is split by word for strings ("inverted index"). Tags with binary data, and tags without any data, are
    /// not present in this table at all, as there is no way to search for specific data values for such tags
    entries_by_data: Table<(Tag, Buffer), SmallSortedSet<Entry>>,
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names (no duplicates)
    tags_by_entry: Table<Entry, SmallSortedSet<Tag>>,
    /// Lookup values of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    ///
    /// Note: Tags without any associated data are not present, tag instances with empty data (e.g. empty strings) are present
    tag_values: Table<(Entry, Tag), Buffer>,
    /// Convert tag names to their associated tag entries (that is, the "meta" entry that describes that tag)
    ///
    /// Key: tag name
    /// Value: tag entry ID
    tag_entries: Table<Tag, Entry>,
    /// `Scru64Generator` already impls `Clone` itself but I'm not sure what the implications of cloning a generator are
    /// in terms of the generated IDs, so I'm making it a singleton type thing just in case
    generator: Arc<Mutex<Scru64Generator>>,
}

/// Transaction-agnostic methods
impl<T: DbApi> TagsDatabase<T> {
    fn open_tables(mut database: Database<T>) -> miette::Result<Self> {
        let entries_by_tag = database.open_table("EntriesByTag").into_diagnostic()?;
        let entries_by_data = database.open_table("EntriesByData").into_diagnostic()?;
        let tags_by_entry = database.open_table("TagsByEntry").into_diagnostic()?;
        let tag_values = database.open_table("TagValues").into_diagnostic()?;
        let tag_entries = database.open_table("TagEntries").into_diagnostic()?;
        let generator = Arc::new(Mutex::new(
            init_or_resume_generator(&database, &tags_by_entry).into_diagnostic()?,
        ));
        Ok(Self {
            database,
            entries_by_tag,
            entries_by_data,
            tags_by_entry,
            tag_values,
            tag_entries,
            generator,
        })
    }

    /// If this tag already exists, this is effectively a no-op.
    fn create_tag(&mut self, tag: Tag) -> miette::Result<()> {
        // Update entries_by_tag: create empty value set (unless it already exists)
        // entries_by_data: Not updated as new tags aren't applied to anything
        // tags_by_entry: Not updated as new tags aren't applied to anything
        // tag_values: Not updated as new tags aren't applied to anything
        // Insert into tag_entries

        self.database
            .ensure_exists(&mut self.entries_by_tag, &tag)
            .into_diagnostic()?;

        let entry = self.generate_entry();

        self.database
            .insert(&mut self.tag_entries, &tag, &entry)
            .into_diagnostic()?;

        Ok(())
    }

    /// If the entry does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_entry(&mut self, entry: Entry) -> miette::Result<()> {
        todo!()
    }

    /// If the tag does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_tag(&mut self, tag: Tag) -> miette::Result<()> {
        // Take from entries_by_tag: taken value is used to update tags_by_entry
        // Remove from entries_by_data: Prefix search to find all keys to remove
        // Update tags_by_entry: Use value taken from entries_by_tag to find all keys to update, then remove tag from value sets
        // Remove from tag_values: Use value taken from entries_by_tag to find all keys to remove
        // Take from tag_entries: taken value is used to delete the tag entry

        // TODO: also delete the tag entry

        let entries = self
            .database
            .take(&mut self.entries_by_tag, &tag)
            .into_diagnostic()?
            .unwrap_or_default();

        for result in self.database.prefix(&self.entries_by_data, &tag) {
            let (key, _) = result.into_diagnostic()?;
            self.database
                .remove(&mut self.entries_by_data, &key)
                .into_diagnostic()?;
        }

        for &entry in &entries {
            self.database
                .fetch_update(&mut self.tags_by_entry, &entry, |mut values| {
                    if let Some(values) = &mut values {
                        values.remove(&tag);
                    }
                    values
                })
                .into_diagnostic()?;

            let key = (entry, tag);
            self.database
                .remove(&mut self.tag_values, &key)
                .into_diagnostic()?;
        }

        let entry = self
            .database
            .take(&mut self.tag_entries, &tag)
            .into_diagnostic()?;

        if let Some(entry) = entry {
            self.delete_entry(entry)
        } else {
            Ok(())
        }
    }

    /// If this tag is already applied to this entry, this is effectively a no-op. For tags with values, it updates the value.
    ///
    /// You can think of this as creating a new instance of the tag.
    fn insert_tag_on_entry(&mut self, tag: Tag, entry: Entry) -> miette::Result<()> {
        // Update entries_by_tag: insert entry into value set
        // Update entries_by_data: insert entry into value set
        // Update tags_by_entry: insert tag into value set
        // Insert into tag_values
        // tag_entries: Not updated as the tag itself is not being mutated, just a new instance being created

        let value = Buffer::default();

        self.database
            .fetch_update_single(&mut self.entries_by_tag, &tag, &entry)
            .into_diagnostic()?;

        let key = (tag, value);
        self.database
            .fetch_update_single(&mut self.entries_by_data, &key, &entry)
            .into_diagnostic()?;

        let (tag, value) = key;
        self.database
            .fetch_update_single(&mut self.tags_by_entry, &entry, &tag)
            .into_diagnostic()?;

        let key = (entry, tag);
        self.database
            .insert(&mut self.tag_values, &key, &value)
            .into_diagnostic()
    }

    fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.database.get(&self.tag_entries, tag)
    }

    fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database
            .get(&self.tag_entries, tag)
            .map(|entry| entry.is_some())
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database
            .prefix(&self.tag_entries, prefix)
            .map(|kv| kv.map(|(k, _)| k))
    }
}

/// Ditto
impl<T> TagsDatabase<T> {
    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    fn generate_entry(&mut self) -> Entry {
        // See: `scru64::new_sync()`
        const DELAY: std::time::Duration = std::time::Duration::from_millis(64);
        loop {
            if let Some(id) = self.generator.lock().unwrap().generate() {
                return id.into();
            } else {
                eprintln!("sleeping to generate entry ID");
                std::thread::sleep(DELAY);
            }
        }
    }
}

/// No transaction methods
impl TagsDatabase {
    fn open(path: impl AsRef<Path>) -> miette::Result<Self> {
        Database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_temporary() -> miette::Result<Self> {
        Database::open_temporary()
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn initialize_transaction(self) -> Result<TagsDatabase<Transaction>, database::Error> {
        self.database
            .initialize_transaction()
            .map(|database| TagsDatabase {
                database,
                entries_by_tag: self.entries_by_tag,
                entries_by_data: self.entries_by_data,
                tags_by_entry: self.tags_by_entry,
                tag_values: self.tag_values,
                tag_entries: self.tag_entries,
                generator: self.generator,
            })
    }
}

/// Yes transaction methods
impl TagsDatabase<Transaction> {
    fn commit(self) -> (TagsDatabase, Result<Result<(), Conflict>, database::Error>) {
        let (database, result) = self.database.commit();
        (
            TagsDatabase {
                database,
                entries_by_tag: self.entries_by_tag,
                entries_by_data: self.entries_by_data,
                tags_by_entry: self.tags_by_entry,
                tag_values: self.tag_values,
                tag_entries: self.tag_entries,
                generator: self.generator,
            },
            result,
        )
    }

    fn rollback(self) -> TagsDatabase {
        TagsDatabase {
            database: self.database.rollback(),
            entries_by_tag: self.entries_by_tag,
            entries_by_data: self.entries_by_data,
            tags_by_entry: self.tags_by_entry,
            tag_values: self.tag_values,
            tag_entries: self.tag_entries,
            generator: self.generator,
        }
    }
}

pub struct ActiveTransactionDatabaseState {
    database: TagsDatabase<Transaction>,
    folder: RecentFolder,
}

impl ActiveTransactionDatabaseState {
    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database.search_tags_names_by_prefix(prefix)
    }

    pub fn commit(self) -> (DatabaseState, Result<Result<(), Conflict>, database::Error>) {
        let (database, result) = self.database.commit();
        (
            DatabaseState {
                database,
                folder: self.folder,
            },
            result,
        )
    }

    pub fn rollback(self) -> DatabaseState {
        DatabaseState {
            database: self.database.rollback(),
            folder: self.folder,
        }
    }
}

pub trait SmallSortedSetTableExt {
    fn ensure_exists<Key: AsBytes, Value: Serde + Ord>(
        &mut self,
        table: &mut Table<Key, SmallSortedSet<Value>>,
        key: &Key,
    ) -> Result<SmallSortedSet<Value>, DeserError<SmallSortedSet<Value>>>;

    fn fetch_update_single<Key: AsBytes, Value: Serde + Clone + Ord>(
        &mut self,
        table: &mut Table<Key, SmallSortedSet<Value>>,
        key: &Key,
        value: &Value,
    ) -> Result<SmallSortedSet<Value>, DeserError<SmallSortedSet<Value>>>;
}

impl<T: DbApi> SmallSortedSetTableExt for T {
    fn ensure_exists<Key: AsBytes, Value: Serde + Ord>(
        &mut self,
        table: &mut Table<Key, SmallSortedSet<Value>>,
        key: &Key,
    ) -> Result<SmallSortedSet<Value>, DeserError<SmallSortedSet<Value>>> {
        self.fetch_update(table, key, |values| Some(values.unwrap_or_default()))
            // The above closure always returns `Some`
            .map(Option::unwrap)
    }

    fn fetch_update_single<Key: AsBytes, Value: Serde + Clone + Ord>(
        &mut self,
        table: &mut Table<Key, SmallSortedSet<Value>>,
        key: &Key,
        value: &Value,
    ) -> Result<SmallSortedSet<Value>, DeserError<SmallSortedSet<Value>>> {
        self.fetch_update(table, key, |values| {
            let mut values = values.unwrap_or_default();
            let _ = values.insert(value.clone());
            Some(values)
        })
        // The above closure always returns `Some`
        .map(Option::unwrap)
    }
}

fn init_or_resume_generator<T: DbApi, Value: FromBytes>(
    db: &Database<T>,
    table: &Table<Entry, Value>,
) -> Result<Scru64Generator, DeserKvError<Entry, Value>> {
    // Scru64 ids are always 64 bits. This parameter controls how many of those bits are allocated to a custom value we control.
    // This is useful for distributed systems with multiple nodes using the same ID space, but we are just an offline, local app,
    // so we don't need this. Instead, we set this to the smallest allowed number of bits (1), which maximizes the number of bits
    // allocated to the other (actually useful) parts of the ID.
    const NODE_ID_SIZE: u8 = 1;

    // This is infallible, but `unwrap` and similar methods aren't `const`, so we do this instead
    const DEFAULT_NODE_ID: NodeSpec = match NodeSpec::with_node_id(0, NODE_ID_SIZE) {
        Ok(id) => id,
        Err(_) => unreachable!(),
    };

    db.last_kv(table).map(|kv| {
        Scru64Generator::new(kv.map_or(DEFAULT_NODE_ID, |(latest_id, _)| {
            // This only errors if the second parameter has a bad value, which will never happen because it's a known-good literal
            NodeSpec::with_node_prev(latest_id.0, NODE_ID_SIZE).unwrap()
        }))
    })
}
