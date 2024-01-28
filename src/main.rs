use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Router;
use axum_extra::routing::Resource;
use tokio::net::TcpListener;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod repository;
use repository::Repository;

// AppState must be cheap to clone, usually by using an Arc on the field.
#[derive(Clone)]
struct AppState {
    repository: Repository,
}

#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("repository error")]
    RepositoryError(#[from] repository::RepositoryError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::RepositoryError(err) => {
                tracing::error!(%err, "error in repository")
            }
        }
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
    }
}

#[axum::debug_handler]
async fn tasks_create(State(state): State<AppState>) -> Result<String, AppError> {
    let repository = state.repository;
    let now = time::OffsetDateTime::now_utc();
    let task_id = repository.create_task(String::from("foo"), now).await?;
    Ok(task_id.simple().to_string())
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "svix-test=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let repository = Repository::new_from_env().await;
    let state = AppState { repository };

    let tasks_resource = Resource::named("tasks").create(tasks_create);

    let app = Router::new()
        .merge(tasks_resource)
        .with_state(state)
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(10)),
        ));

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
