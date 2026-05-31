use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    pub async fn open(data_dir: PathBuf) -> crate::Result<Self> {
        Ok(Self { data_dir })
    }

    pub async fn initialize(&self) -> crate::Result<()> {
        tracing::info!(path = %self.data_dir.display(), "storage initialized");
        Ok(())
    }
}
