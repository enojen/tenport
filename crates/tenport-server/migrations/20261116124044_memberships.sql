create table memberships (
    org_id text not null references organizations (id) on delete cascade,
    user_id text not null references users (id) on delete cascade,
    role text not null check (role in ('admin', 'member')),
    created_at text not null,
    created_by text not null,
    primary key (org_id, user_id)
) strict;

create index memberships_by_user on memberships (user_id);
