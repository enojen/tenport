create table users (
    id text primary key not null,
    name text not null,
    email text,
    github text not null unique,
    created_at text not null,
    created_by text not null
) strict;
