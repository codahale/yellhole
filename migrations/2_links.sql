create table link (
    link_id text primary key not null default (lower(hex(randomblob(16)))),
    title text not null,
    url text not null,
    description text,
    created_at timestamp not null default current_timestamp
);

create index idx_link_created_at_desc on link (created_at desc);

insert into link (title, url) values (
    "VSCode Remote Development using SSH",
    "https://code.visualstudio.com/docs/remote/ssh"
);