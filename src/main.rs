use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Router;
use axum_extra::routing::Resource;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod repository;
use repository::Repository;
use uuid::Uuid;

// AppState must be cheap to clone, usually by using an Arc on the field.
#[derive(Clone)]
struct AppState {
    repository: Repository,
}

#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("repository error")]
    RepositoryError(#[from] repository::RepositoryError),
    #[error("json rejection")]
    JsonRejection(#[from] JsonRejection),
    #[error("validation rejection")]
    ValidationRejection(String),
    #[error("not found")]
    NotFound,
}

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
struct AppJson<T>(T);

impl<T> IntoResponse for AppJson<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::JsonRejection(rejection) => (rejection.status(), rejection.body_text()),
            AppError::ValidationRejection(message) => (StatusCode::BAD_REQUEST, message),
            AppError::NotFound => (StatusCode::NOT_FOUND, String::from("Can't find task")),
            AppError::RepositoryError(err) => {
                tracing::error!(%err, "error in repository");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    String::from("Something went wrong"),
                )
            }
        };
        (status, AppJson(ErrorResponse { message })).into_response()
    }
}

#[derive(Deserialize)]
struct TasksCreateParams {
    #[serde(rename = "type")]
    task_type: String,
    #[serde(with = "time::serde::rfc3339")]
    start_at: time::OffsetDateTime,
}

#[derive(Serialize, Clone)]
struct TasksCreateResponse {
    // TODO: Make this a UUID with some serde option to pick the format
    id: String,
}

#[axum::debug_handler]
async fn tasks_create(
    State(state): State<AppState>,
    AppJson(params): AppJson<TasksCreateParams>,
) -> Result<AppJson<TasksCreateResponse>, AppError> {
    let repository = state.repository;

    // TODO: Find a better system for doing validation in general
    let task_type = match params.task_type.as_str() {
        "foo" | "bar" | "baz" => params.task_type,
        _ => {
            return Err(AppError::ValidationRejection(String::from(
                "Task type not supported",
            )))
        }
    };

    let start_at = params.start_at.checked_to_offset(time::UtcOffset::UTC);
    if let Some(start_at) = start_at {
        let task_id = repository.create_task(&task_type, start_at).await?;
        let tasks_create_response = TasksCreateResponse {
            id: task_id.simple().to_string(),
        };
        Ok(AppJson(tasks_create_response))
    } else {
        Err(AppError::ValidationRejection(String::from(
            "Could not convert start_at to UTC",
        )))
    }
}

#[derive(Serialize)]
struct TasksShowResponse {
    // TODO: Make this a UUID with some serde option to pick the format
    id: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(rename = "type")]
    task_type: String,
    status: String,
    #[serde(with = "time::serde::rfc3339")]
    start_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    completed_at: Option<OffsetDateTime>,
}
#[axum::debug_handler]
async fn tasks_show(
    State(state): State<AppState>,
    // TODO: Parse the UUID here
    Path(id): Path<String>,
) -> Result<AppJson<TasksShowResponse>, AppError> {
    let id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return Err(AppError::NotFound),
    };
    let repository = state.repository;
    let task = repository.show_task(&id).await?;
    if let Some(task) = task {
        Ok(AppJson(TasksShowResponse {
            id: task.id.simple().to_string(),
            created_at: task.created_at,
            task_type: task.task_type,
            status: task.status.as_str().to_owned(),
            start_at: task.start_at,
            completed_at: task.completed_at,
        }))
    } else {
        Err(AppError::NotFound)
    }
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

    let tasks_resource = Resource::named("tasks")
        .create(tasks_create)
        .show(tasks_show);

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
