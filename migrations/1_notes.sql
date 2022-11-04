create table note (
    note_id text primary key not null default (lower(hex(randomblob(16)))),
    body text not null,
    created_at timestamp not null default current_timestamp
);

create index idx_note_created_at_desc on note (created_at desc);

insert into note (body) values ("# Hello world!
It's me, a _tumblelog_.")