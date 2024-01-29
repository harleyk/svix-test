/// Using a repository pattern here, especially since both the service and the
/// worker will eventually make use of it. All access to the database is done
/// through functions here.
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

// TODO: Implement serde serializer
#[derive(PartialEq, Eq, Debug)]
pub enum TaskStatus {
    WaitingToStart,
    WaitingToComplete,
    Completed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            TaskStatus::WaitingToStart => "waiting_to_start",
            TaskStatus::WaitingToComplete => "waiting_to_complete",
            TaskStatus::Completed => "completed",
        }
    }
}

impl TaskStatus {
    // TODO: All of this logic could be moved into the database, but might be
    // harder to test there but easier to query on.
    fn from_timestamps(
        start_at: OffsetDateTime,
        completed_at: Option<OffsetDateTime>,
        now: OffsetDateTime,
    ) -> Self {
        if completed_at.is_some() {
            Self::Completed
        } else if start_at < now {
            Self::WaitingToStart
        } else {
            Self::WaitingToComplete
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn test_completed() {
        assert_eq!(
            TaskStatus::from_timestamps(
                datetime!(2024-01-02 00:00 +0),
                Some(datetime!(2024-01-02 00:01 +0)),
                datetime!(2024-01-02 00:00 +0)
            ),
            TaskStatus::Completed
        );
    }
    #[test]
    fn test_waiting_to_start() {
        assert_eq!(
            TaskStatus::from_timestamps(
                datetime!(2024-01-01 00:00 +0),
                None,
                datetime!(2024-01-02 00:00 +0)
            ),
            TaskStatus::WaitingToStart
        );
    }
    #[test]
    fn test_waiting_to_complete() {
        assert_eq!(
            TaskStatus::from_timestamps(
                datetime!(2024-01-02 00:00 +0),
                None,
                datetime!(2024-01-02 00:00 +0)
            ),
            TaskStatus::WaitingToComplete
        );
    }
}

pub struct Task {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub task_type: String,
    pub status: TaskStatus,
    pub start_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

pub struct WorkerTask {
    pub id: Uuid,
    pub task_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("sqlx error")]
    SqlxError(#[from] sqlx::Error),
}

// Since Repository is used by AppState, this must be cheap to clone.
#[derive(Clone)]
pub struct Repository {
    pool: PgPool,
}

impl Repository {
    pub async fn new(database_url: String) -> Self {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&database_url)
            .await
            .expect("Can't connect to database");

        Self { pool }
    }

    pub async fn new_from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:example@localhost".to_string());
        Self::new(database_url).await
    }

    pub async fn create_task(
        &self,
        task_type: &str,
        start_at: time::OffsetDateTime,
    ) -> Result<Uuid, RepositoryError> {
        // TODO: Find a better way to ensure that only UTC timestamps get used.
        assert!(start_at.offset().is_utc());
        let start_at = time::PrimitiveDateTime::new(start_at.date(), start_at.time());
        let task_insert_record = sqlx::query!(
            "INSERT INTO tasks (type, start_at) VALUES ($1, $2) RETURNING id",
            task_type,
            start_at
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(task_insert_record.id)
    }

    pub async fn show_task(&self, id: &Uuid) -> Result<Option<Task>, RepositoryError> {
        let record = sqlx::query!(
            "SELECT id, created_at, type as task_type, start_at, completed_at FROM tasks WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        if let Some(record) = record {
            let now = OffsetDateTime::now_utc();
            let start_at = record.start_at.assume_utc();
            let completed_at = record.completed_at.map(|t| t.assume_utc());
            let task = Task {
                id: record.id,
                created_at: record.created_at.assume_utc(),
                task_type: record.task_type,
                status: TaskStatus::from_timestamps(start_at, completed_at, now),
                start_at,
                completed_at,
            };
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub async fn next_worker_task(&self) -> Result<Option<WorkerTask>, RepositoryError> {
        let mut tx = self.pool.begin().await?;
        let record_to_lock = sqlx::query!(
            "SELECT id, type as task_type FROM tasks
                WHERE id = (SELECT id FROM tasks
                    WHERE start_at < now()
                    AND completed_at IS NULL
                    AND worker_assigned_at IS NULL
                    LIMIT 1)
                AND start_at < now()
                AND completed_at IS NULL
                AND worker_assigned_at IS NULL
            FOR UPDATE"
        )
        .fetch_optional(&mut *tx)
        .await?;
        let result = if let Some(record_to_lock) = record_to_lock {
            sqlx::query!(
                "UPDATE tasks SET worker_assigned_at = now() WHERE id = $1",
                record_to_lock.id
            )
            .execute(&mut *tx)
            .await?;
            Ok(Some(WorkerTask {
                id: record_to_lock.id,
                task_type: record_to_lock.task_type,
            }))
        } else {
            Ok(None)
        };
        tx.commit().await?;
        result
    }

    pub async fn complete_task(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query!("UPDATE tasks SET completed_at = now() WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
