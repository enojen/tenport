create table projects (
    id text primary key not null,
    org_id text not null references organizations (id) on delete cascade,
    name text not null,
    repo text not null,
    created_at text not null,
    created_by text not null
) strict;

create index projects_by_org on projects (org_id);
