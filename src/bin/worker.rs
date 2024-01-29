use std::time::Duration;

use svix_test::repository::Repository;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

async fn foo() -> bool {
    false
}

async fn bar() -> bool {
    false
}

async fn baz() -> bool {
    false
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "worker=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let repository = Repository::new_from_env().await;
    loop {
        let next_worker_task = match repository.next_worker_task().await {
            Ok(next_worker_task) => next_worker_task,
            Err(err) => {
                tracing::error!(%err);
                continue;
            }
        };
        if let Some(task_to_run) = next_worker_task {
            let repository = repository.clone();
            tokio::task::spawn(async move {
                tracing::info!(id = task_to_run.id.simple().to_string(), "starting task");
                let completed = match task_to_run.task_type.as_str() {
                    "foo" => foo().await,
                    "bar" => bar().await,
                    "baz" => baz().await,
                    _ => false,
                };
                if completed {
                    match repository.complete_task(task_to_run.id).await {
                        Ok(_) => {
                            tracing::info!(
                                id = task_to_run.id.simple().to_string(),
                                "completed task"
                            )
                        }
                        Err(_) => tracing::error!(
                            id = task_to_run.id.simple().to_string(),
                            "could not mark task completed"
                        ),
                    }
                }
            });
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
