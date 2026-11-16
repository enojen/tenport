create table tokens (
    id text primary key not null,
    user_id text not null references users (id) on delete cascade,
    name text not null,
    hash text not null unique,
    created_at text not null,
    created_by text not null
) strict;

create index tokens_by_user on tokens (user_id);
