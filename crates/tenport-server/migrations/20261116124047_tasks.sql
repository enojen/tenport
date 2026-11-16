create table tasks (
    project_id text not null references projects (id) on delete cascade,
    id text not null,
    title text,
    url text,
    created_at text not null,
    created_by text not null,
    closed_at text,
    closed_by text,
    primary key (project_id, id)
) strict;
