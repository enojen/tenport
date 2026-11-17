# Capabilities

What Tenport must do. Each item has a status:

- **Today**: the tenbox server (`tenbox/server`, TypeScript) does it now. Tenport must do the same.
- **Decided**: the user decided it. Tenport is built this way.
- **Proposed**: a proposal that waits for the user's yes.
- **Open**: a question with no answer yet.

## 1. Accounts

| Capability | Status |
|---|---|
| A user has a name, a GitHub login and an email | Today |
| A user calls the API with a bearer token. The server stores only the token's hash | Today |
| A user is added by hand with a server command (`user add`) | Today; login below replaces it |
| The first admin: a setting of the server names the owner's GitHub login. That person becomes admin on their first GitHub login; nobody else can. No sign-up screen, no server command | Decided |
| A person logs in from the panel, or with `tenbox login`: the CLI shows a code and the person confirms it in the browser | Decided |
| Login is with GitHub only | Open |
| An organization is Tenyard's own group of users and projects. It is not a GitHub organization | Decided |
| A user is a member of one or more organizations, as `admin` or `member` | Today |
| Data of the whole cluster, such as costs, is for an organization admin only | Today |

## 2. Projects

| Capability | Status |
|---|---|
| A project belongs to one organization and has a name and a repository | Today |
| A project is added by hand with a server command (`project add`) | Today; `tenbox init` below replaces it |
| GitHub access and the tracker belong to the project, not to the organization. They can be different for each project | Decided |
| A project gets repository access when the GitHub App is installed on its repository. The login's GitHub account only says who the person is | Decided |
| The tracker of a project is GitHub Issues or ClickUp | Decided |
| GitHub Issues needs no extra connection: the App can write issues | Proposed |
| ClickUp needs a personal token (`pk_…`) and a list id for the project. Tenport stores the token and gives it only to that project's agents | Proposed |
| ClickUp OAuth and webhooks. A webhook needs a public address: a Tailscale Funnel node that opens only the webhook path, with an HMAC check | Proposed, later |
| Each project has its own steps, from its `tenbox.yaml`. Tenport sends them with the project | Decided |

### `tenbox init` (proposed)

The CLI asks for every setting once, and Tenport stores them. After that, the person only runs
commands.

1. The CLI checks the login. With no login, it asks for one first.
2. It asks for the organization.
3. It finds the repository from the git remote.
4. It asks Tenport whether the App is installed on the repository. If not, it shows the install
   link.
5. It asks for the tracker. GitHub Issues needs no more questions. ClickUp needs the token and
   the list id.
6. Tenport saves the project. The CLI writes the project id to the `id` field of `tenbox.yaml`.

Init does not start a lane or a planner.

### The steps of a task (proposed)

The project gives the list of steps; the babysitter picks the steps of each task from it.

1. `tenbox.yaml` lists the steps. Each step has a name and one sentence that says when it is
   used. Some steps are required:

   ```yaml
   steps:
     - { name: plan, required: true }
     - { name: build, required: true }
     - { name: debug, when: "a bug: show it with a repro first" }
     - { name: eval, when: "the change touches an agent prompt or a skill" }
     - { name: pr, required: true }
     - { name: merge, required: true }
   ```

2. After the plan, the babysitter picks the steps the task needs, in order, for example
   `plan, debug, build, pr, merge` for a bug. The pick is part of the plan.
3. The user sees the steps when they approve the plan. There is no second approval.
4. The panel shows the steps of the task, not every step of the project.
5. Tenport refuses a pick that leaves out a required step or names a step that is not in the
   list. The panel only shows what it gets.
6. During the task, the babysitter can add a step, such as `debug` when a bug shows up in the
   build. It sends an event with the reason, and the panel shows the change.

## 3. Claude subscriptions

| Capability | Status |
|---|---|
| A user adds one or more Claude subscriptions in the panel. Each has a name, such as "personal" or "work" | Decided |
| An agent uses the Claude credential of the person who started it | Decided |
| Phase 1, one user: the claude.ai login, as today, and chat with Remote Control | Proposed |
| Phase 2, other users: each organization brings its own API key, and the chat moves into Tenface with the Agent SDK | Proposed |
| In phase 2, the API key belongs to the organization or to the user | Open |

What the Claude Code docs say (checked 2026-11-16):

- Remote Control (chat from the phone) needs a claude.ai login. It does not work with an API key
  or with `claude setup-token`.
- `claude setup-token` makes a token that is good for one year and "can only make model
  requests".
- On the first start, `claude remote-control` asks "Enable Remote Control? (y/n)" and asks to
  trust the folder. With no terminal, it stops with "Workspace not trusted". The docs show no
  flag that skips this. Whether it starts in a pod without a question is not proven yet.
- Anthropic's terms do not allow a product to offer the claude.ai login to its users. Each end
  user must use their own API key, subscription or cloud provider credential. So if Tenyard is
  offered to other people, each organization brings its own API key.

## 4. Lanes and agents

| Capability | Status |
|---|---|
| Each project has one planner agent (Fable, `claude-fable-5-2`) | Decided |
| The planner opens the lanes. The panel has no "New lane" button | Decided |
| Each lane has one babysitter agent (Opus, `claude-opus-5-5`) | Decided |
| When a babysitter writes its "work done" report, Tenport wakes the planner | Decided |
| An agent runs on the user's computer or on the server. The state is always in Tenport | Decided |
| Tenport always starts a server agent. It writes "who started it" and "which project" into the pod. Claude Code does not know whose it is; Tenport tells it | Decided |
| An agent on the computer calls Tenport with the user's login token, and sends its own agent id with each command. Tenport checks that the agent is the user's, and the log says "the user, through this agent" | Decided, 2026-11-16 |
| An agent registers itself from inside its own session: Claude Code, OpenCode or another coding agent runs `tenbox planner start` or `tenbox babysitter start`. Tenport writes `agent.started` and gives an agent id; the CLI keeps it in `.tenbox/agent.json` in the agent's folder, which git ignores. One agent per folder: the planner in the project, each babysitter in its lane's worktree | Decided, 2026-11-16 |
| An agent in a pod calls Tenport with a token that Tenport gives for that run | Open: decided with the server agents |
| `tenbox planner start --server --claude personal` starts it on the server with the named subscription | Proposed |
| `tenbox lane new <name>` is run by the planner. The lane's babysitter starts on the server and uses the subscription of the person who started the planner | Proposed |
| The panel's Chat button opens the agent's session, wherever the agent runs | Proposed |
| A commit on the computer uses the user's git settings. A commit in a pod uses the App's name, with the person who started it as a `Co-authored-by` line | Proposed |
| The `agents` part of `tenbox.yaml` gives the model and the skills of the planner and the babysitter | Proposed |
| "Start planner" and "Stop" buttons in the panel, or only in the CLI. In the panel, they would be the first exception to "the panel only shows" | Open |

### Messages between agents (proposed)

The planner and the babysitters talk through Tenport. There is no second service.

1. An agent runs `tenbox message <to> "<text>"`. Tenport writes `message.sent` and opens an
   unread row for the receiver, as it does for a question.
2. The receiver reads with `tenbox inbox`, and the row closes.
3. Every message goes through Tenport, so the panel and the audit log show it.
4. An agent talks only to its own planner or to its own babysitters, so that two agents do not
   answer each other without end.

An agent that waits must wake up when a message comes. In Claude Code, `/loop` can run
`tenbox inbox` again and again; `tenbox inbox --wait` works for any coding agent. Which one works
best is tried when the agents are built.

### A lane that runs on its own (proposed)

The babysitter takes its lane's tasks to the end without the user, but never to `main`.

1. Each lane has its own branch, made from `main`. Each task opens its PR into the lane branch.
2. The babysitter merges a task's PR into the lane branch itself, only after the proof loop
   passes.
3. The user reads one PR, the lane branch into `main`, and merges it.

The proof loop of each task, in this order:

1. A real repro first: a failing test or a real run that shows the bug or the missing work.
2. A test that is red before the change and green after it.
3. The checks of the project pass.
4. A real run of the change.
5. An independent review, by an agent that did not write the code.

When a step fails again and again, the babysitter stops and asks the user (`question.asked`).

## 5. Runs on the cluster

| Capability | Status |
|---|---|
| For each Job, Tenport makes a one-hour GitHub App token for that project's repository only | Today |
| The box never sees the real token. It gets a placeholder, and the injector asks Tenport for the real token while the Job is open (`POST /run-credentials`). This route opens nothing else | Today |
| The placeholder is made from the Job's uid with a key. A changed key breaks the runs that are open at that time | Today |
| Tenport reads the boot log of each ended pod and stores it as a `run.finished` event. It lists the runs of a task | Today |
| The tracker key of a project goes to a Secret. Today the CLI writes it before each Job; Tenport will write it when an admin changes it, and keep no copy (section 9) | Decided |
| The claude.ai login of the boxes is copied from the computer to a Secret. Tenport's Claude subscriptions replace this copy | Proposed |

## 6. Events, costs and notices

| Capability | Status |
|---|---|
| The CLI sends an event each time a task moves. Tenport stores each event once, in one transaction | Today |
| Tenport watches the Nodes and stores an event when a worker comes or goes, with its Hetzner type and hourly price | Today |
| Tenport watches the task volumes and stores their size and price | Today |
| A closed task's volume is deleted after a grace period. An idle open volume sends a notice to ntfy | Today |
| The costs of a time window: what the workers and the task volumes cost, and each task's share | Today |
| A `/metrics` page for Prometheus with the workers and the volumes that are up | Today |
| A copy of the database to a file (`backup`), which goes to Object Storage each day | Today |
| The API documents itself: `/openapi.json` and `/docs` | Today |

### Every change is an event (decided, 2026-11-16)

Every change reaches Tenport as an event: a task step, a question of an agent, an answer, a
node that comes or goes. Tenport writes the event to a log that is never changed. In the same
transaction it updates the tables that hold the state now, and the SSE stream tells the panel.

- The log gives the history, the panel's timeline and the costs. The tables give fast reads.
- A broken table can be built again from the log. Tenport does not replay the log on each
  start: this is not full event sourcing.
- Each event type is a Rust type, with one function that updates the tables.
- Every command that changes the state sends an event, from a person or from an agent. A command
  that only reads, such as `status`, sends none. Tenport writes who did it from the token, so the
  log is the audit (decided, 2026-11-16).
- An event type that is out only gets new fields, and a new field is optional. A field never
  changes its meaning; a new meaning is a new event type. So an older CLI keeps working, and an
  event needs no version field. Tenport refuses an unknown type, and the CLI then says to update
  tenbox (decided, 2026-11-16).

A question, for example (the details wait until the work reaches it):

1. The babysitter asks with a CLI command, such as `tenbox message`. It sends `question.asked`.
2. Tenport writes the event and adds an open row to the questions table. The panel shows it.
3. The user answers on the panel. Tenport writes `question.answered` and closes the row.
4. The babysitter gets the answer.

The panel then writes an answer, so this is an exception to "the panel only shows" (see the open
question on the panel's buttons).

## 7. The panel's API

Rules from the panel's RAM measurements (2026-11-15). Low browser RAM is the user's first concern.

| Capability | Status |
|---|---|
| The panel reads the state over SSE | Decided |
| On connect and on each reconnect, Tenport sends the full state at once: a summary of all projects and the full task tree of the project in view | Decided |
| The summary of a project has its name, its lane counts and the question counts of each agent. Only the project in view gets its task tree | Decided |
| Finished tasks and agent notes go with a limit (the last N), so the state does not grow without end | Decided |
| The stream always answers 200 with the SSE content type. Any other answer stops the browser's retries, and the panel stays offline | Decided |
| `Cache-Control: no-cache`, and no buffering on any proxy in between | Decided |
| Tenport refuses a move that puts a task under itself or under one of its children. The panel only shows what it gets | Decided |
| One name for each task state. Today the state says `todo` and the screen says `waiting` | Open |
| One source for "needs you": the open rows of the questions table, which only events change | Decided |

## 8. What stays outside Tenport

- The git and worktree work, the checks and the PR: the CLI does them where the code is.
- The plan, build and close workflows and the skills: they stay with the agents. They write their
  state to Tenport instead of to local files.
- The cluster's manifests and its own Secrets: they stay in the `tenyard` repository. The
  Secrets `project-<project id>` are the exception: Tenport writes them (section 9).

## 9. The move from the tenbox server

Tenport takes over every job of the tenbox server, and every talk with the cluster, one PR at a
time. In the end only Tenport talks to Kubernetes: the CLI on a computer and the agent in a box
send their requests to Tenport, and a computer keeps only `TENPORT_URL` and `TENPORT_TOKEN`; no
kubeconfig, no bucket keys, no project secrets. Then `server/` and the CLI's Kubernetes code are
deleted from tenbox. The live server runs the image of tenbox commit `71dbd66`; no commit after it
changes the server.

Decided (2026-11-17):

- Tenport starts the Jobs. `task/job.yaml` stays in tenyard; Flux gives it to Tenport as a
  ConfigMap. Tenport's ServiceAccount may create, read and delete Jobs and Secrets in the
  `tenbox` namespace only.
- Tenport talks to the cluster with the `kube` crate, behind one trait with a fake for tests.
  It lists the cluster on a timer and turns each change into an event; no watch streams.
- A project secret (the tracker key, the ntfy URL) goes from the admin's CLI to Tenport, and
  Tenport writes it to the Secret `project-<project id>`. Tenport keeps no copy: not in its
  database, not in a log. The API only writes a Secret and never gives it back; it tells only
  that it is there and when it changed. Only an organization admin may write one. k3s has
  `secrets-encryption` (tenyard #127), so the Secrets are encrypted on disk.
- While both servers run, Tenport's Jobs have their own label,
  `app.kubernetes.io/managed-by: tenport` in place of `app: tenyard`, and their template sends the
  injector to Tenport, so the tenbox server never touches them. Every box pod has the label
  `box: "true"`, and Tetragon and the process metrics select it, so they see the boxes of both.
- Tenport is open on the tailnet only, like Grafana. A public domain waits for more users.
- The task home is a task archive in a bucket, no longer a volume (tenbox #313). The old volumes
  move to the bucket outside this plan, so Tenport does not watch volumes.

What the tenbox server and the CLI do, and where Tenport is:

| Today | Tenport |
|---|---|
| `GET /health`, `GET /openapi.json` | Done |
| Users, organizations, memberships (`admin` or `member`), tokens; each request needs a user token | Done |
| `project add` command: a repository joins an organization as a project | Done as `POST /projects` and `GET /projects` |
| `POST /events`: takes the task events; an id that comes twice is stored once | Done; the CLI sends `task.started` and `task.closed` |
| The CLI starts, follows, stops and deletes the Jobs with a kubeconfig | Steps 3 and 6 |
| The CLI writes the Secret `project-<project id>` before each Job | Step 4: `PUT /projects/{project}/secrets/{name}`; the CLI stops in step 6 |
| The CLI makes the presigned links of the task archive, with the bucket keys on the computer | Step 4: Tenport makes them for its own Jobs; the CLI stops in step 6 |
| `POST /run-credentials`: the injector sends a Job's placeholder and gets the real GitHub token, good for one hour and only for that project's repository. This route needs the injector token, not a user token | Step 4 |
| A placeholder Secret for each new Job | Step 4 |
| `run.finished` when a box container ends, with the OOM count and the close output from the pod log | Step 5 |
| `GET /tasks/{task}/runs`: the runs of a task | Step 5 |
| Nodes, every 30 seconds: a worker that comes or goes, with its Hetzner type and hourly price | Step 8 |
| Task volumes: their price, the deletion after the grace period and the ntfy notice | Not moved: no task volumes |
| `GET /costs?from&to`, the `costs` command and `GET /metrics` | Step 9 |
| `user add` command: a new user; the token shows once, in its output | Done as `tenport user add` |
| `backup --to <file>` command: a copy of the database. A CronJob runs it each day at 03:00 and keeps 7 days | Step 10 |
| `GET /docs`: the Swagger page | Step 10 |

The steps, in order:

1. `POST /events`, and the CLI sends its events to it. Done.
2. Tenport in the cluster, next to the tenbox server: its image, its ServiceAccount and Role,
   and a Tailscale node, in tenyard's manifests. It answers `/health` on the tailnet. The
   `user add` command comes here, so the first admin can be made in the pod. Done (2026-11-17):
   tenport #18 and tenyard #122; the first admin is made.
3. Tenport starts the Jobs. The template comes from the ConfigMaps `run-template` and
   `run-services` (tenyard #124). A run is one tenbox command in a box; its Kubernetes object is
   one Job with one pod, and the run has the Job's name. The run stays in Tenport after the Job
   is deleted. Its events are `run.started` and, in step 5, `run.finished`.
   Done (2026-11-17): the cluster client (tenport #20), `POST /runs` (tenport #21), the box
   label (tenyard #123), the Role and the ConfigMaps (tenyard #124), the runs API (tenport #24),
   and the image of tenport main in tenyard (tenyard #125), with a real run from the live
   Tenport.
   - The runs API: `GET /runs?project=<id>&task=<id>` lists the runs of a task (the CLI's
     `machine ls`); `GET /runs/{name}` gives one run: `name`, `project`, `task`, `command`,
     `class`, `created`, `phase` and `reason`; `GET /runs/{name}/log?follow=true` gives the
     log of the box container as plain text; `DELETE /runs/{name}` stops the run and deletes
     its Job and pod (the CLI's `machine stop` lists the running runs of the task and deletes
     each). `phase` uses the Kubernetes words `Running`, `Succeeded`, `Failed`, and `Unknown`
     when the Job is gone (10 minutes after it ends) and no `run.finished` is stored yet.
     `reason` is the box container's, such as `OOMKilled` or `CreateContainerConfigError`.
     404 for a run Tenport does not know, 403 for a user outside the project's organization.
     The log does not wait. When the box container has not started, it answers 409 with the
     code `box-not-started`, the reason, and `Retry-After: 5`: the client asks again after 5
     seconds. When the Job is gone, 409 with the code `job-gone`: asking again does not help.
   - Deleting a task moves to step 4: with no task volumes, a task has only its task archive
     to delete, and that needs the bucket keys of step 4.
4. Now; k3s has `secrets-encryption` (tenyard #127). What a run needs comes from Tenport: the
   project secrets, the presigned links of the task archive (the bucket keys in Tenport's
   env), the GitHub App token, the placeholder Secret and `POST /run-credentials`. Deleting a
   task, which deletes its task archive (the CLI's `machine rm`). One PR for each, in this
   order.
   - The project secrets: `PUT /projects/{project}/secrets/{name}` with `{"value": ...}`,
     `DELETE /projects/{project}/secrets/{name}`, and `GET /projects/{project}/secrets`, which
     gives each `name` and when it `changed`, never a value. A name is `tracker_api_key` or
     `ntfy_url`, the keys the Job template reads. Their events are `project.secret_changed` and
     `project.secret_deleted`, with the name only. Until step 6 the CLI still writes the whole
     Secret before each of its Jobs, with the values of the computer. Done: tenport #31.
   - The task archive: Tenport reads its bucket from `TENPORT_TASK_ARCHIVE_BUCKET` (a URL
     whose path is the bucket name), `TENPORT_TASK_ARCHIVE_BUCKET_REGION`,
     `TENPORT_TASK_ARCHIVE_BUCKET_KEY_FILE` and `TENPORT_TASK_ARCHIVE_BUCKET_SECRET_FILE`, and
     does not serve without them. Each Job gets a GET and a PUT link to
     `tasks/<project id>/<task>.tar.zst`, good for 24 hours, the same object and the same time
     as the CLI's; a Job ends after 8 hours at most.
   - Deleting a task: `DELETE /tasks/{task}?project=<id>`, with the event `task.deleted`.
   - Tenport's Jobs send their placeholders to an injector of their own, which asks Tenport;
     the injector of the tenbox server stays as it is until step 11.
5. The runs: `run.finished`, so `GET /runs?project=<id>&task=<id>` and `GET /runs/{name}` keep
   the phase after the Job is deleted. `run.finished` also keeps the JSON result of the tenbox
   command (the last JSON line of the box log) as `output`, and `GET /runs/{name}` gives it. An
   agent waits for a run with `GET /runs/{name}` until `phase` is not `Running`, and reads
   `phase`, `reason` and `output` there: it never reads a log. A followed log is for a person,
   in a terminal or in the panel.
6. The CLI stops talking to the cluster: `tenbox machine run/logs/wait/stop/rm/ls` go to
   Tenport. The kubeconfig and the bucket key file leave the computer.
   - To decide here: a client that only follows `Retry-After` asks forever when the box
     cannot start, such as with `CreateContainerConfigError`, because that reason does not
     go away alone. The 409 `box-not-started` carries the reason, so the CLI can stop on a
     reason that is an error and show it.
7. The agent in a box reaches Tenport from inside the cluster with a run token that starts runs
   in its own project only, with a limit on nested runs and on open runs at one time.
8. The cluster watch: nodes, with their Hetzner type and price.
9. Costs: `GET /costs`, the `costs` command and `GET /metrics`, with the workers and the bucket.
10. The `backup` command and `/docs`.
11. The tenbox server stops: copy its data into Tenport, then delete `server/` and the CLI's
    Kubernetes code from tenbox. Tenport's Jobs go back to the label `app: tenyard`, and the
    pod label `box: "true"` (tenyard #123) goes: Tetragon and the process metrics select
    `app: tenyard` again.

## Open questions

1. Is the two-phase plan for Claude right?
2. In phase 2, does the API key belong to the organization or to the user?
3. Do "Start planner" and "Stop" go in the panel, or only in the CLI?
4. Is login with GitHub only?
5. Try Remote Control in a pod before the server work: does it start without a question?
6. Which name does a task state use: `todo` or `waiting`?
