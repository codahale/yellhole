create table note (
    note_id text primary key not null default (lower(hex(randomblob(16)))),
    body text not null,
    created_at timestamp not null default current_timestamp
);

create index idx_note_created_at_desc on note (created_at desc);

insert into note (body) values ("# Hello world!

It's me, a _tumblelog_.");

insert into note (body) values ("[Remote Development using SSH](https://code.visualstudio.com/docs/remote/ssh)

A good walk-through on doing remote development over SSH with VS Code.
");

insert into note (body) values ("![Peeg](https://64.media.tumblr.com/14d02852cf6c48dd9b302c1ae5df2d56/2898486ed8901ad2-09/s1280x1920/866f4d046d42d14103f586e288165fe75e8a2440.jpg)

Big samesies.
");