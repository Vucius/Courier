use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(data_dir.join("attachments"))?;
        std::fs::create_dir_all(data_dir.join("raw"))?;

        Ok(Self { data_dir })
    }

    pub fn initialize(&self) -> Result<()> {
        let db_path = self.db_path();
        let connection = rusqlite::Connection::open(&db_path)?;
        connection.execute_batch(include_str!("../../../migrations/001_init.sql"))?;
        connection.execute_batch(include_str!("../../../migrations/002_search.sql"))?;

        tracing::info!(path = %db_path.display(), "courier storage initialized");
        Ok(())
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("courier.db")
    }
}
