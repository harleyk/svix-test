CREATE TABLE tasks (
    id uuid PRIMARY KEY DEFAULT (gen_random_uuid()),
    created_at timestamp without time zone NOT NULL DEFAULT NOW(),
    type character varying(256) NOT NULL,
    start_at timestamp without time zone NOT NULL,
    worker_assigned_at timestamp without time zone,
    completed_at timestamp without time zone
);
