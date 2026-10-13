use std::{
    ops::{Deref, DerefMut},
    path::Path,
};

use rusqlite::{OpenFlags, PrepFlags, Statement, config::DbConfig::*};

pub const PRAGMA_APPLICATION_ID: i32 = 0x25b9fbdb;
pub const PRAGMA_USER_VERSION: i32 = match i32::from_str_radix(env!("CARGO_PKG_VERSION_MAJOR"), 10)
{
    Ok(i) => i,
    Err(_) => unreachable!(),
};

pub struct Connection(rusqlite::Connection);

impl Connection {
    pub fn open_existing<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let flags = OpenFlags::default() - OpenFlags::SQLITE_OPEN_CREATE;
        rusqlite::Connection::open_with_flags(path, flags).map(Self)
    }

    pub fn open_or_create<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        rusqlite::Connection::open(path).map(Self)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        rusqlite::Connection::open_in_memory().map(Self)
    }

    pub fn is_empty_db(&self) -> rusqlite::Result<bool> {
        self.query_one("SELECT count(*) == 0 FROM sqlite_schema", (), |result| {
            result.get::<_, bool>(0)
        })
    }

    pub fn configure_sane(&mut self) -> rusqlite::Result<()> {
        // <https://sqlite.org/c3ref/c_dbconfig_defensive.html>
        self.set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)?;
        self.set_db_config(SQLITE_DBCONFIG_DEFENSIVE, true)?;
        self.set_db_config(SQLITE_DBCONFIG_DQS_DML, false)?;
        self.set_db_config(SQLITE_DBCONFIG_DQS_DDL, false)?;
        self.set_db_config(SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;

        // <https://sqlite.org/pragma.html>
        self.execute_batch(
            "PRAGMA cell_size_check = true; PRAGMA journal_mode = WAL; PRAGMA locking_mode = EXCLUSIVE;",
        )
    }

    pub fn set_id_and_version(&mut self) -> rusqlite::Result<()> {
        self.execute_batch(&format!("PRAGMA application_id = {PRAGMA_APPLICATION_ID}; PRAGMA user_version = {PRAGMA_USER_VERSION};"))
    }

    pub fn verify_id_and_version(&self) -> rusqlite::Result<()> {
        let (id, version) = self.query_one(
            "SELECT * FROM pragma_application_id, pragma_user_version",
            (),
            |result| Ok((result.get::<_, i32>(0)?, result.get::<_, i32>(1)?)),
        )?;

        if id != PRAGMA_APPLICATION_ID {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
                Some(format!(
                    "This database is from a different application as it has unexpected PRAGMA application_id = {id:#x}"
                )),
            ));
        }

        if version != PRAGMA_USER_VERSION {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
                Some(format!(
                    "This database has an incompatible major version number and automatic migrations are not implemented yet (db v{version} vs app v{PRAGMA_USER_VERSION})"
                )),
            ));
        }

        Ok(())
    }

    pub fn prepare(&self, sql: &str) -> rusqlite::Result<Statement<'_>> {
        // <https://sqlite.org/c3ref/c_prepare_dont_log.html>
        self.prepare_with_flags(sql, PrepFlags::SQLITE_PREPARE_FROM_DDL)
    }
}

impl Deref for Connection {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Connection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // <https://sqlite.org/pragma.html#pragma_optimize>
        self.0.execute_batch("PRAGMA optimize");
    }
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        // Interesting misc:
        // CREATE TABLE ... STRICT, WITHOUT ROWID;
        // <https://sqlite.org/stricttables.html>
        // <https://sqlite.org/withoutrowid.html>
        Connection::open_or_create(path).and_then(Self::new)
    }

    pub fn open_temp() -> rusqlite::Result<Self> {
        Connection::open_in_memory().and_then(Self::new)
    }

    pub fn new(mut connection: Connection) -> rusqlite::Result<Self> {
        connection.configure_sane()?;
        if connection.is_empty_db()? {
            connection.set_id_and_version()?;
        } else {
            connection.verify_id_and_version()?;
        }
        Ok(Self { connection })
    }
}
