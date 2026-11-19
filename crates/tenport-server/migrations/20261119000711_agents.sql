create table agents (
    id text primary key not null,
    user_id text not null references users (id) on delete cascade,
    kind text not null,
    name text not null,
    is_default integer not null,
    login_changed_at text,
    created_at text not null,
    created_by text not null,
    unique (user_id, name)
) strict;

create unique index default_agent_of_user on agents (user_id) where is_default;

alter table runs add column agent_id text;
