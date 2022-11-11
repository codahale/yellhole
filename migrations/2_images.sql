create table image (
    image_id text primary key not null,
    original_filename text not null,
    content_type text not null,
    created_at timestamp not null default current_timestamp
);

create index idx_image_created_at_desc on image (created_at desc);