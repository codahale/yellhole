create table session (
    session_id text primary key not null,
    as_json text not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp
);
