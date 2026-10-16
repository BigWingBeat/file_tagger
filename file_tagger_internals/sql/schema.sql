/* rowid alias so implicitly unique and table B-Tree indexed */
/* should prefer `INSERT INTO entries DEFAULT VALUES RETURNING entry` for entry ID generation */
CREATE TABLE IF NOT EXISTS entries(entry INTEGER PRIMARY KEY) STRICT;

/* tag_entry is rowid alias so unique and indexed by table B-Tree rather than less optimal index B-Tree */
/* text implicitly indexed by unique constraint */
CREATE TABLE IF NOT EXISTS tags(tag_entry INTEGER PRIMARY KEY REFERENCES entries, tag TEXT NOT NULL UNIQUE) STRICT;

/* many-to-many index between entries and tags, plus associated tag data */
/* composite primary key enforces that any single tag cannot applied to any single entry multiple times */
/* tag data type is heterogeneous between different tags but homogeneous within all instances of a single tag */
/* Insert-or-update should be done with an "upsert" clause, like so:
`INSERT INTO tag_data VALUES(...) ON CONFLICT DO UPDATE SET data = excluded.data;` */
CREATE TABLE IF NOT EXISTS tag_data(
	entry INTEGER NOT NULL REFERENCES entries,
	tag TEXT NOT NULL REFERENCES tags(tag),
	data ANY,
	PRIMARY KEY (entry, tag)
) STRICT;

/* This saves having to do a linear scan over `tag_data` when searching for entries with specific tags (the most common search),
as well as when performing foreign key constraint enforcement for `DELETE`ing from `tags` */
CREATE INDEX IF NOT EXISTS tag_data_tags ON tag_data(tag);

/* full-text-search index over plaintext-type tag data */
/* as this is a contentless fts index it should be queried with something like
`SELECT rank,tag_data.* FROM tag_data JOIN fts_tag_data ON tag_data.rowid = fts_tag_data.rowid WHERE fts_tag_data MATCH :search_query ORDER BY rank */
CREATE VIRTUAL TABLE IF NOT EXISTS fts_tag_data USING fts5(
	tag, data, content = '', contentless_delete=1, tokenize = 'unicode61 remove_diacritics 2'
);

/* Triggers for automatic maintenance of the fts index */

CREATE TRIGGER IF NOT EXISTS fts_tag_data_insert AFTER INSERT ON tag_data WHEN typeof(new.data) = 'text' BEGIN
	INSERT INTO fts_tag_data(rowid, tag, data) VALUES (new.rowid, new.tag, new.data);
END;

CREATE TRIGGER IF NOT EXISTS fts_tag_data_delete AFTER DELETE ON tag_data WHEN typeof(old.data) = 'text' BEGIN
	DELETE FROM fts_tag_data WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS fts_tag_data_update AFTER UPDATE ON tag_data WHEN typeof(new.data) = 'text' BEGIN
	UPDATE fts_tag_data SET tag = new.tag, data = new.data WHERE rowid = new.rowid;
END;
