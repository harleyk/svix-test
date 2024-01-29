# How to run

Copy the `.env.example` and point the DATABASE_URL at a Postgres database. The
`compose.yml` is configured to run one.

Install the `sqlx-cli` through `cargo install sqlx-cli` and then run
`sqlx database reset` to get the initial migrations installed.

Run `cargo run --bin service` to start the HTTP server. It runs on port 3000.

Example curl command line:
```
curl --header "Content-Type: application/json" --request POST 
--data '{"type": "foo", "start_at": "2024-01-29T00:06:03Z"}'
http://localhost:3000/tasks
```

Everything is under the /tasks name and at this point just create and show
have been implemented.

Run `cargo run --bin worker` to start the background worker.

# TODOs

- Find a better way to manage the repository section. Maybe a separate crate?
- Implement the rest of the API. It should be pretty easy from here but there
 is a decision about where to put the logic in the app or in the database.
- Turn more things into types rather than having things like the task types
 being passed around as strings.
- Better error handling all around.
- Metrics and other observability.
- Something to clean up tasks that get stuck.
- Retry mechanism for the tasks.
- Better testing of the things around time. Should the database be the source
 of truth around the time or should the application?
