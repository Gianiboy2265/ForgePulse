use std::{path::Path, time::Duration};

use forge_core::{ForgeError, Result, metrics::MetricSnapshot};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityStatus {
    pub healthy: bool,
    pub details: String,
}

impl Storage {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| ForgeError::io(parent, source))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(storage_error)?;
        MIGRATOR.run(&pool).await.map_err(storage_error)?;
        Ok(Self { pool })
    }

    pub async fn insert_metric_chunk(&self, samples: &[MetricSnapshot]) -> Result<()> {
        let Some(first) = samples.first() else {
            return Err(ForgeError::InvalidInput(
                "a metric chunk cannot be empty".to_owned(),
            ));
        };
        let Some(last) = samples.last() else {
            return Err(ForgeError::Invariant(
                "non-empty metric chunk has no final sample".to_owned(),
            ));
        };
        if samples.windows(2).any(|pair| {
            pair[0].captured_at > pair[1].captured_at || pair[0].sequence >= pair[1].sequence
        }) {
            return Err(ForgeError::InvalidInput(
                "metric chunks must be strictly ordered".to_owned(),
            ));
        }
        let message_pack = rmp_serde::to_vec_named(samples)
            .map_err(|error| ForgeError::Storage(format!("metric encoding failed: {error}")))?;
        let compressed = zstd::stream::encode_all(message_pack.as_slice(), 3)
            .map_err(|error| ForgeError::Storage(format!("metric compression failed: {error}")))?;
        let checksum = hex::encode(Sha256::digest(&compressed));
        let sample_count = i64::try_from(samples.len())
            .map_err(|_| ForgeError::InvalidInput("too many samples in metric chunk".to_owned()))?;
        sqlx::query(
            "INSERT INTO metric_chunks \
             (series, start_time_ms, end_time_ms, sample_count, schema_version, codec, \
              checksum_sha256, payload, created_at_ms) VALUES (?, ?, ?, ?, 1, 'msgpack+zstd', ?, ?, ?)",
        )
        .bind("system")
        .bind(first.captured_at.timestamp_millis())
        .bind(last.captured_at.timestamp_millis())
        .bind(sample_count)
        .bind(checksum)
        .bind(compressed)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    pub async fn latest_snapshot(&self) -> Result<Option<MetricSnapshot>> {
        let row = sqlx::query(
            "SELECT checksum_sha256, payload FROM metric_chunks \
             WHERE series = 'system' ORDER BY end_time_ms DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let checksum: String = row.try_get("checksum_sha256").map_err(storage_error)?;
        let payload: Vec<u8> = row.try_get("payload").map_err(storage_error)?;
        if hex::encode(Sha256::digest(&payload)) != checksum {
            return Err(ForgeError::Storage(
                "metric chunk checksum validation failed".to_owned(),
            ));
        }
        let decoded = zstd::stream::decode_all(payload.as_slice()).map_err(|error| {
            ForgeError::Storage(format!("metric decompression failed: {error}"))
        })?;
        let samples: Vec<MetricSnapshot> = rmp_serde::from_slice(&decoded)
            .map_err(|error| ForgeError::Storage(format!("metric decoding failed: {error}")))?;
        Ok(samples.last().cloned())
    }

    pub async fn quick_check(&self) -> Result<IntegrityStatus> {
        let result: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(IntegrityStatus {
            healthy: result == "ok",
            details: result,
        })
    }

    pub async fn pending_rollback_count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM rollback_records WHERE state IN ('pending','armed','restoring')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        u64::try_from(count)
            .map_err(|_| ForgeError::Storage("rollback count was negative".to_owned()))
    }

    pub async fn enforce_retention(&self, cutoff_time_ms: i64, batch_size: u32) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM metric_chunks WHERE id IN (\
                SELECT id FROM metric_chunks WHERE end_time_ms < ? ORDER BY end_time_ms LIMIT ?\
             )",
        )
        .bind(cutoff_time_ms)
        .bind(i64::from(batch_size))
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(result.rows_affected())
    }

    pub async fn delete_oldest_metric_chunks(&self, batch_size: u32) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM metric_chunks WHERE id IN (\
                SELECT id FROM metric_chunks ORDER BY end_time_ms LIMIT ?\
             )",
        )
        .bind(i64::from(batch_size))
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(result.rows_affected())
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

fn storage_error(error: impl std::fmt::Display) -> ForgeError {
    ForgeError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use forge_core::metrics::{CpuMetrics, MemoryMetrics};

    use super::*;

    fn sample(sequence: u64) -> MetricSnapshot {
        MetricSnapshot {
            sequence,
            captured_at: Utc::now() + chrono::Duration::milliseconds(sequence as i64),
            collection_duration_us: 1,
            sampling_interval_ms: 1000,
            dropped_samples: 0,
            cpu: CpuMetrics {
                total_percent: Some(20.0),
                logical_processor_count: 8,
            },
            memory: MemoryMetrics {
                total_physical_bytes: 1,
                available_physical_bytes: 1,
                committed_bytes: 1,
                commit_limit_bytes: 1,
                memory_load_percent: 1,
            },
            processes: Vec::new(),
            capabilities: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn migrations_and_chunk_round_trip() -> Result<()> {
        let directory = tempfile::tempdir().map_err(|source| ForgeError::io("temp", source))?;
        let storage = Storage::open(&directory.path().join("test.db")).await?;
        storage.insert_metric_chunk(&[sample(1), sample(2)]).await?;
        let latest = storage.latest_snapshot().await?;
        assert_eq!(latest.map(|value| value.sequence), Some(2));
        assert!(storage.quick_check().await?.healthy);
        Ok(())
    }
}
