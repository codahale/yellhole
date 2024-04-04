create table if not exists session (
    session_id text primary key not null,
    created_at timestamp not null default current_timestamp
);
