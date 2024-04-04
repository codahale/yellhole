create table if not exists challenge (
    challenge_id text primary key not null,
    bytes blob not null,
    created_at timestamp not null default current_timestamp
);