alter table runs add column finished_at text;
alter table runs add column phase text;
alter table runs add column reason text;
alter table runs add column exit_code integer;
alter table runs add column oom_kill integer;
alter table runs add column output text;
