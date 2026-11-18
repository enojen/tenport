drop index runs_by_task;
create index runs_by_task on runs (project_id, task_id, created_at, name);

create index unfinished_runs_by_created_at on runs (created_at, name) where finished_at is null;
