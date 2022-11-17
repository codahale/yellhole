create table passkey (
    passkey_id blob primary key not null,
    public_key_sec1 blob not null,
    created_at timestamp not null default current_timestamp
);

drop table credential;
delete from session;