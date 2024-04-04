create table if not exists note (
    note_id text primary key not null,
    body text not null,
    created_at timestamp not null default current_timestamp
);

create index if not exists idx_note_created_at_desc on note (created_at desc);