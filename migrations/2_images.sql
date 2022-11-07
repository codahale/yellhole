create table image (
    image_id text primary key not null default (lower(hex(randomblob(16)))),
    original_file_ext text not null,
    processed boolean not null default false,
    created_at timestamp not null default current_timestamp
);

create index idx_image_created_at_desc on image (created_at desc) where processed;