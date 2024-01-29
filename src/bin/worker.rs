use std::time::Duration;

use svix_test::repository::Repository;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

async fn foo(task_id: String) -> bool {
    tokio::time::sleep(Duration::from_secs(3)).await;
    println!("Foo {task_id}");
    true
}

async fn bar() -> bool {
    // TODO: Better error handling
    let status = reqwest::get("https://www.whattimeisitrightnow.com/")
        .await
        .unwrap()
        .status();
    println!("{}", status.as_u16());
    true
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
                let task_id = task_to_run.id.simple().to_string();
                tracing::info!(id = task_to_run.id.simple().to_string(), "starting task");
                // TODO: This bool return should probably become some sort of
                // type which can tell the difference between worker will always
                // fail, worker would like to try again and so on.
                let completed = match task_to_run.task_type.as_str() {
                    "foo" => foo(task_id).await,
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
