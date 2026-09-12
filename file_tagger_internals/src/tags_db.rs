use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use miette::{IntoDiagnostic, miette};
use scru64::{Scru64Generator, generator::NodeSpec};

mod tags;
mod types;

use crate::{
    DB_FOLDER_NAME, Transaction,
    app_data::RecentFolder,
    database::{
        self, Buffer, Conflict, Database, DbApi, DeserError, DeserKvError, NoTransaction, Table,
    },
    serde::{AsBytes, BytesInto, FromBytes, Serde, SmallSortedSet},
    tags_db::tags::{META_DATA, META_TAG},
};

pub use types::{
    AnyRange, AnyRangeError, Entry, EntryParseError, RangeError, Tag, TagData, TagDataError,
    TagDataType, TagDataTypeError,
};

#[derive(Clone)]
pub struct DatabaseState {
    db: TagsDatabase,
    folder: RecentFolder,
}

impl DatabaseState {
    pub fn active_folder(&self) -> &RecentFolder {
        &self.folder
    }

    pub fn create_temporary() -> miette::Result<Self> {
        TagsDatabase::open_temporary().map(|db| Self {
            db,
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
            .map(|db| Self { db, folder })
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.db.generate_entry()
    }

    pub fn initialize_transaction(self) -> Result<ActiveTransactionDatabaseState, database::Error> {
        self.db
            .initialize_transaction()
            .map(|db| ActiveTransactionDatabaseState {
                db,
                folder: self.folder,
            })
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

#[derive(Clone)]
pub struct TagsDatabase<Transaction = NoTransaction> {
    db: Database<Transaction>,
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
    entries_by_data: Table<(Tag, TagData), SmallSortedSet<Entry>>,
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names (no duplicates)
    tags_by_entry: Table<Entry, SmallSortedSet<Tag>>,
    /// Lookup data of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    ///
    /// Note: Tags without any associated data are not present, tag instances with empty data (e.g. empty strings) are present
    tag_data: Table<(Entry, Tag), TagData>,
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
    fn open_tables(mut db: Database<T>) -> miette::Result<Self> {
        let entries_by_tag = db.open_table("EntriesByTag").into_diagnostic()?;
        let entries_by_data = db.open_table("EntriesByData").into_diagnostic()?;
        let tags_by_entry = db.open_table("TagsByEntry").into_diagnostic()?;
        let tag_data = db.open_table("TagData").into_diagnostic()?;
        let tag_entries = db.open_table("TagEntries").into_diagnostic()?;
        let generator = Arc::new(Mutex::new(
            init_or_resume_generator(&db, &tags_by_entry).into_diagnostic()?,
        ));

        let mut db = Self {
            db,
            entries_by_tag,
            entries_by_data,
            tags_by_entry,
            tag_data,
            tag_entries,
            generator,
        };
        db.initialize_well_known_tags()?;
        Ok(db)
    }

    fn initialize_well_known_tags(&mut self) -> miette::Result<()> {
        Ok(())
    }

    /// If this tag already exists, this is effectively a no-op.
    fn create_tag(&mut self, tag: Tag) -> miette::Result<()> {
        // Update entries_by_tag: create empty value set (unless it already exists)
        // entries_by_data: Not updated as new tags aren't applied to anything
        // tags_by_entry: Not updated as new tags aren't applied to anything
        // tag_data: Not updated as new tags aren't applied to anything
        // Insert into tag_entries (unless it already exists)

        self.db
            .ensure_exists(&mut self.entries_by_tag, &tag)
            .into_diagnostic()?;

        // TODO: update_fetch
        let entry = self
            .db
            .fetch_update(&mut self.tag_entries, &tag, |entry| {
                Some(entry.unwrap_or_else(|| self.generator.generate_entry()))
            })
            .into_diagnostic()?
            // The above closure always returns `Some`
            .unwrap();

        // self.insert_tag_data_on_entry(entry, *META_TAG, tag.as_bytes().as_ref().into())?;
        self.db
            .insert(
                &mut self.tag_data,
                &(entry, *META_TAG),
                &TagData::String(tag.as_str().into()),
            )
            .into_diagnostic()?;

        Ok(())
    }

    fn get_tag_entry(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.db.get(&self.tag_entries, tag)
    }

    fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.get_tag_entry(tag).map(|entry| entry.is_some())
    }

    fn get_tag_metadata(
        &self,
        tag_entry: Entry,
    ) -> Result<Option<TagDataType>, DeserError<TagDataType>> {
        // We do this to special-case the deserialization because `TagData` cannot be
        // deserialized normally, and does not include a variant for `TagDataType` anyway
        let table = &self.tag_data;
        // SAFETY: `Table` is `#[repr(transparent)]` and we are only transmuting the phantomdata/type parameters
        let table = unsafe {
            std::mem::transmute::<&Table<(Entry, Tag), TagData>, &Table<(Entry, Tag), Buffer>>(
                table,
            )
        };

        self.db
            .get(table, &(tag_entry, *META_DATA))
            .map_err(DeserError::map_type)
            .and_then(|b| b.map(|b| b.bytes_into().map_err(Into::into)).transpose())
    }

    fn get_tag_data(&self, entry: Entry, tag: Tag) -> miette::Result<Option<TagData>> {
        let tag_entry = self
            .get_tag_entry(&tag)
            .into_diagnostic()
            .and_then(|entry| entry.ok_or_else(|| miette!("missing tag entry")))?;

        let metadata = self
            .get_tag_metadata(tag_entry)
            .into_diagnostic()
            .and_then(|data| data.ok_or_else(|| miette!("missing tag metadata")))?;

        // We do this to special-case the deserialization because `TagData` cannot be deserialized normally
        let table = &self.tag_data;
        // SAFETY: `Table` is `#[repr(transparent)]` and we are only transmuting the phantomdata/type parameters
        let table = unsafe {
            std::mem::transmute::<&Table<(Entry, Tag), TagData>, &Table<(Entry, Tag), Buffer>>(
                table,
            )
        };

        self.db
            .get(table, &(entry, tag))
            .map_err(DeserError::map_type)
            .and_then(|data| {
                data.map(|data| TagData::deserialize(data, &metadata).map_err(Into::into))
                    .transpose()
            })
            .into_diagnostic()
    }

    /// If this tag is already applied to this entry, this is effectively a no-op. For tags with values, it updates the value.
    ///
    /// You can think of this as creating a new instance of the tag.
    fn insert_tag_on_entry(&mut self, tag: Tag, entry: Entry) -> miette::Result<()> {
        // Update entries_by_tag: insert entry into value set
        // entries_by_data: Not updated as this tag has no data
        // Update tags_by_entry: insert tag into value set
        // tag_data: Not updated as this tag has no data
        // tag_entries: Not updated as the tag itself is not being mutated, just a new instance being created

        let tag_entry = self
            .get_tag_entry(&tag)
            .into_diagnostic()
            .and_then(|entry| entry.ok_or_else(|| miette!("missing tag entry")))?;

        let metadata = self
            .get_tag_metadata(tag_entry)
            .into_diagnostic()
            .and_then(|data| data.ok_or_else(|| miette!("missing tag metadata")))?;

        if !matches!(metadata, TagDataType::None) {
            return Err(miette!("can't insert without data on tag with data"));
        }

        self.db
            .fetch_update_single(&mut self.entries_by_tag, &tag, &entry)
            .into_diagnostic()?;

        self.db
            .fetch_update_single(&mut self.tags_by_entry, &entry, &tag)
            .into_diagnostic()?;

        Ok(())
    }

    fn insert_tag_data_on_entry(
        &mut self,
        entry: Entry,
        tag: Tag,
        data: TagData,
    ) -> miette::Result<()> {
        // Update entries_by_tag: insert entry into value set
        // Update entries_by_data: insert entry into value set
        // Update tags_by_entry: insert tag into value set
        // Insert into tag_data
        // tag_entries: Not updated as the tag itself is not being mutated, just a new instance being created

        let tag_entry = self
            .get_tag_entry(&tag)
            .into_diagnostic()
            .and_then(|entry| entry.ok_or_else(|| miette!("missing tag entry")))?;

        let metadata = self
            .get_tag_metadata(tag_entry)
            .into_diagnostic()
            .and_then(|data| data.ok_or_else(|| miette!("missing tag metadata")))?;

        if metadata.verify_data(&data).is_err() {
            return Err(miette!("invalid data"));
        }

        self.db
            .fetch_update_single(&mut self.entries_by_tag, &tag, &entry)
            .into_diagnostic()?;

        let key = (tag, data);
        self.db
            .fetch_update_single(&mut self.entries_by_data, &key, &entry)
            .into_diagnostic()?;

        let (tag, data) = key;
        self.db
            .fetch_update_single(&mut self.tags_by_entry, &entry, &tag)
            .into_diagnostic()?;

        let key = (entry, tag);
        self.db
            .insert(&mut self.tag_data, &key, &data)
            .into_diagnostic()
    }

    /// If the tag does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_tag(&mut self, tag: Tag) -> miette::Result<()> {
        self.get_tag_entry(&tag)
            .into_diagnostic()
            .and_then(|entry| entry.map_or(Ok(()), |entry| self.delete_entry(entry)))
    }

    /// If the entry does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_entry(&mut self, entry: Entry) -> miette::Result<()> {
        // Take from tags_by_entry: taken value is used to update entries_by_tag
        // Update entries_by_tag: Use value taken from tags_by_entry to find all keys to update, then remove entry from value sets
        // entries_by_data: Use value taken from tags_by_entry to find all keys to update, then remove entry from value sets
        // tag_data: Prefix search to find all keys to remove
        // Remove from tag_entries: Use retrieved tag name to find key to remove

        // Metatag "tag_name": lookup tag_data: (entry, "tag_name") -> deser to tag name for tag entries, then delete_tag_name

        todo!()
    }

    /// Should never be called directly. This is a helper method for `delete_entry`.
    /// If the tag does not exist, this is effectively a no-op.
    ///
    /// Potentially very destructive! Use with care.
    fn delete_tag_name(&mut self, tag: Tag) -> miette::Result<()> {
        // Take from entries_by_tag: taken value is used to update tags_by_entry
        // Remove from entries_by_data: Prefix search to find all keys to remove
        // Update tags_by_entry: Use value taken from entries_by_tag to find all keys to update, then remove tag from value sets
        // Remove from tag_data: Use value taken from entries_by_tag to find all keys to remove
        // Remove from tag_entries

        let entries = self
            .db
            .take(&mut self.entries_by_tag, &tag)
            .into_diagnostic()?
            .unwrap_or_default();

        for result in self.db.prefix(&self.entries_by_data, &tag) {
            let (key, _) = result.into_diagnostic()?;
            self.db
                .remove(&mut self.entries_by_data, &key)
                .into_diagnostic()?;
        }

        for &entry in &entries {
            self.db
                .fetch_update(&mut self.tags_by_entry, &entry, |mut values| {
                    if let Some(values) = &mut values {
                        let _ = values.remove(&tag);
                    }
                    values
                })
                .into_diagnostic()?;

            let key = (entry, tag);
            self.db.remove(&mut self.tag_data, &key).into_diagnostic()?;
        }

        self.db
            .remove(&mut self.tag_entries, &tag)
            .into_diagnostic()
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.db
            .prefix(&self.tag_entries, prefix)
            .map(|kv| kv.map(|(k, _)| k))
    }
}

/// Ditto
impl<T> TagsDatabase<T> {
    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    fn generate_entry(&mut self) -> Entry {
        self.generator.generate_entry()
    }
}

pub trait EntryGeneratorExt {
    fn generate_entry(&mut self) -> Entry;
}

impl EntryGeneratorExt for Arc<Mutex<Scru64Generator>> {
    fn generate_entry(&mut self) -> Entry {
        // See: `scru64::new_sync()`
        const DELAY: std::time::Duration = std::time::Duration::from_millis(64);
        loop {
            if let Some(id) = self.lock().unwrap().generate() {
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
        self.db.initialize_transaction().map(|db| TagsDatabase {
            db,
            entries_by_tag: self.entries_by_tag,
            entries_by_data: self.entries_by_data,
            tags_by_entry: self.tags_by_entry,
            tag_data: self.tag_data,
            tag_entries: self.tag_entries,
            generator: self.generator,
        })
    }
}

/// Yes transaction methods
impl TagsDatabase<Transaction> {
    fn commit(self) -> (TagsDatabase, Result<Result<(), Conflict>, database::Error>) {
        let (db, result) = self.db.commit();
        (
            TagsDatabase {
                db,
                entries_by_tag: self.entries_by_tag,
                entries_by_data: self.entries_by_data,
                tags_by_entry: self.tags_by_entry,
                tag_data: self.tag_data,
                tag_entries: self.tag_entries,
                generator: self.generator,
            },
            result,
        )
    }

    fn rollback(self) -> TagsDatabase {
        TagsDatabase {
            db: self.db.rollback(),
            entries_by_tag: self.entries_by_tag,
            entries_by_data: self.entries_by_data,
            tags_by_entry: self.tags_by_entry,
            tag_data: self.tag_data,
            tag_entries: self.tag_entries,
            generator: self.generator,
        }
    }
}

pub struct ActiveTransactionDatabaseState {
    db: TagsDatabase<Transaction>,
    folder: RecentFolder,
}

impl ActiveTransactionDatabaseState {
    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.db.generate_entry()
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

    pub fn commit(self) -> (DatabaseState, Result<Result<(), Conflict>, database::Error>) {
        let (db, result) = self.db.commit();
        (
            DatabaseState {
                db,
                folder: self.folder,
            },
            result,
        )
    }

    pub fn rollback(self) -> DatabaseState {
        DatabaseState {
            db: self.db.rollback(),
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
