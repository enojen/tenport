create table runs (
    name text primary key,
    project_id text not null references projects (id) on delete cascade,
    task_id text not null,
    command text not null,
    class text not null,
    created_at text not null,
    created_by text not null
) strict;

create index runs_by_task on runs (project_id, task_id);
