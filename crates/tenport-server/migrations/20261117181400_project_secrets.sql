create table project_secrets (
    project_id text not null references projects (id) on delete cascade,
    secret text not null,
    changed_at text not null,
    changed_by text not null,
    primary key (project_id, secret)
) strict;
