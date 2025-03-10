use std::{collections::HashMap, path::Path, sync::LazyLock};

use chrono::Local;
use serde::{Deserialize, Serialize};
use sysinfo::System;
use thiserror::Error;
use tokio::{fs, sync::Mutex};
use tracing::Level;

static SYSTEM: LazyLock<Mutex<System>> = LazyLock::new(|| {
    let mut sys = System::new_all();
    sys.refresh_all();
    Mutex::new(sys)
});

#[derive(Debug, Error)]
pub enum FilePersistenceError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Metadata Directory not provided")]
    MetadataDirectoryNotProvided,
    #[error("Artifact Directory not provided")]
    ArtifactDirectoryNotProvided,
}

pub trait FilePersistence {
    /// Save the data to a file
    async fn save_to_file(
        &self,
        data: impl AsRef<[u8]>,
        path: impl AsRef<Path>,
    ) -> Result<(), FilePersistenceError> {
        fs::write(path, data).await.map_err(|e| e.into())
    }

    /// Load the data from a file
    async fn load_from_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, FilePersistenceError>
    where
        Self: Sized,
    {
        fs::read(path).await.map_err(|e| e.into())
    }

    /// Save the metadata to a file, defaults to {self.name}_metadata.json in the metadata directory
    async fn save_metadata<T: Serialize>(
        &self,
        metadata: HashMap<String, T>,
    ) -> Result<(), FilePersistenceError> {
        let metadata_dir = if self.metadata_dir().is_none() {
            return Err(FilePersistenceError::MetadataDirectoryNotProvided);
        } else {
            // unwrap is safe here because we just checked if it is None
            self.metadata_dir().unwrap()
        };

        let serialized = serde_json::to_string(&metadata)?;

        // metadata_dir/{self.name}_metadata.json
        let metadata_path = metadata_dir.as_ref().join(format!("{}.json", self.name()));
        self.save_to_file(serialized.as_bytes(), metadata_path)
            .await
    }

    /// Load the metadata from a file
    async fn load_metadata<T: for<'a> Deserialize<'a>>(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<HashMap<String, T>, FilePersistenceError>
    where
        Self: Sized,
    {
        let serialized = self.load_from_file(path).await?;
        let metadata: HashMap<String, T> = serde_json::from_slice(&serialized)?;
        Ok(metadata)
    }

    /// Save the artifact to a file, defaults to {artifact_name}.json in the artifact directory
    async fn save_artifact<T: Serialize>(
        &self,
        artifact: T,
        artifact_name: impl AsRef<str>,
    ) -> Result<(), FilePersistenceError> {
        let artifact_dir = if self.artifact_dir().is_none() {
            return Err(FilePersistenceError::ArtifactDirectoryNotProvided);
        } else {
            // unwrap is safe here because we just checked if it is None
            self.artifact_dir().unwrap()
        };

        let serialized = serde_json::to_string(&artifact)?;
        // artifact_dir/{artifact_name}.json
        let artifact_path = artifact_dir
            .as_ref()
            .join(format!("{}.json", artifact_name.as_ref()));
        self.save_to_file(serialized.as_bytes(), artifact_path)
            .await
    }

    /// Load the artifact from a file
    async fn load_artifact<T: for<'a> Deserialize<'a>>(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<T, FilePersistenceError>
    where
        Self: Sized,
    {
        let serialized = self.load_from_file(path).await?;
        let artifact: T = serde_json::from_slice(&serialized)?;
        Ok(artifact)
    }

    /// Log event to file
    async fn log_event(&self, event: String, log_level: Level) -> Result<(), FilePersistenceError> {
        // tracing
        match log_level {
            Level::DEBUG => tracing::debug!(event),
            Level::INFO => tracing::info!(event),
            Level::WARN => tracing::warn!(event),
            Level::ERROR => tracing::error!(event),
            Level::TRACE => tracing::trace!(event),
        };

        let log_dir = if self.metadata_dir().is_none() {
            return Err(FilePersistenceError::MetadataDirectoryNotProvided);
        } else {
            // unwrap is safe here because we just checked if it is None
            self.metadata_dir().unwrap()
        };

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.f");
        // {timestamp} [log_level] {self.name}: {event}
        let log_message = format!("{} [{}] {}: {}", timestamp, log_level, self.name(), event);
        let log_path = log_dir.as_ref().join(format!("{}_events.log", self.name()));
        self.save_to_file(log_message.as_bytes(), log_path).await
    }

    /// Compress data, defaults to zstd
    async fn compress(&self, data: impl AsRef<[u8]>) -> Result<Vec<u8>, FilePersistenceError> {
        use zstd::stream::encode_all;
        // 0 is the default compression level
        encode_all(data.as_ref(), 0).map_err(|e| e.into())
    }

    /// Decompress data, defaults to zstd
    async fn decompress(&self, data: impl AsRef<[u8]>) -> Result<Vec<u8>, FilePersistenceError> {
        use zstd::stream::decode_all;
        decode_all(data.as_ref()).map_err(|e| e.into())
    }

    async fn log_used_resources(&self) -> Result<(), FilePersistenceError> {
        let mut sys = SYSTEM.lock().await;
        sys.refresh_cpu_usage();
        let cpu_usage = {
            let mut cpu_usage = 0.0;
            for cpu in sys.cpus() {
                cpu_usage += cpu.cpu_usage();
            }
            cpu_usage / sys.cpus().len() as f32
        };

        sys.refresh_memory();
        let memory_usage = sys.used_memory() as f64 / sys.total_memory() as f64;

        self.log_event(
            format!("Resource usage - Memory: {memory_usage}%, CPU: {cpu_usage}%"),
            Level::INFO,
        )
        .await
    }

    fn name(&self) -> String;

    /// Get the directory where the metadata is stored
    fn metadata_dir(&self) -> Option<impl AsRef<Path>>;

    /// Get the directory where the artifacts are stored
    fn artifact_dir(&self) -> Option<impl AsRef<Path>>;
}
