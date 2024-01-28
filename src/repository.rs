/// Using a repository pattern here, especially since both the service and the
/// worker will eventually make use of it. All access to the database is done
/// through functions here.
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RepositoryError {
    #[error("sqlx error")]
    SqlxError(#[from] sqlx::Error),
}

// Since Repository is used by AppState, this must be cheap to clone.
#[derive(Clone)]
pub(crate) struct Repository {
    pool: PgPool,
}

impl Repository {
    pub(crate) async fn new(database_url: String) -> Self {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&database_url)
            .await
            .expect("Can't connect to database");

        Self { pool }
    }

    pub(crate) async fn new_from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:example@localhost".to_string());
        Self::new(database_url).await
    }

    pub(crate) async fn create_task(
        &self,
        task_type: String,
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
}
