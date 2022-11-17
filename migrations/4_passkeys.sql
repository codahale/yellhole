create table passkey (
    passkey_id blob primary key not null,
    public_key_spki blob not null,
    created_at timestamp not null default current_timestamp
);