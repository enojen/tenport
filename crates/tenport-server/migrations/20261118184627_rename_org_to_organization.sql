alter table memberships rename column org_id to organization_id;
alter table projects rename column org_id to organization_id;

drop index projects_by_org;
create index projects_by_organization on projects (organization_id);

update events
set body = json_remove(json_set(body, '$.organization', json_extract(body, '$.org')), '$.org')
where type in ('member.added', 'project.added');
