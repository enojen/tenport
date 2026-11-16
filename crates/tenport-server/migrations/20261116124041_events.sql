create table events (
    id text primary key not null,
    type text not null,
    subject text not null,
    actor text not null,
    at text not null,
    body text not null
) strict;

create index events_by_subject on events (subject, id);
