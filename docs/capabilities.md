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
| An agent on the computer calls Tenport with the user's login token. An agent in a pod calls it with a token that Tenport gives for that run | Proposed |
| `tenbox planner start` starts the planner on the computer, in the current repository | Proposed |
| `tenbox planner start --server --claude personal` starts it on the server with the named subscription | Proposed |
| `tenbox lane new <name>` is run by the planner. The lane's babysitter starts on the server and uses the subscription of the person who started the planner | Proposed |
| The panel's Chat button opens the agent's session, wherever the agent runs | Proposed |
| A commit on the computer uses the user's git settings. A commit in a pod uses the App's name, with the person who started it as a `Co-authored-by` line | Proposed |
| The `agents` part of `tenbox.yaml` gives the model and the skills of the planner and the babysitter | Proposed |
| "Start planner" and "Stop" buttons in the panel, or only in the CLI. In the panel, they would be the first exception to "the panel only shows" | Open |

## 5. Runs on the cluster

| Capability | Status |
|---|---|
| For each Job, Tenport makes a one-hour GitHub App token for that project's repository only | Today |
| The box never sees the real token. It gets a placeholder, and the injector asks Tenport for the real token while the Job is open (`POST /run-credentials`). This route opens nothing else | Today |
| The placeholder is made from the Job's uid with a key. A changed key breaks the runs that are open at that time | Today |
| Tenport reads the boot log of each ended pod and stores it as a `job.finished` event. It lists the runs of a task | Today |
| The tracker key of a project goes to a Secret before each Job. Today the CLI writes it; Tenport will write it, because Tenport holds the key | Proposed |
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
- The cluster's manifests and Secrets: they stay in the `tenyard` repository.

## Open questions

1. Is the two-phase plan for Claude right?
2. In phase 2, does the API key belong to the organization or to the user?
3. Do "Start planner" and "Stop" go in the panel, or only in the CLI?
4. Is login with GitHub only?
5. Try Remote Control in a pod before the server work: does it start without a question?
6. Which name does a task state use: `todo` or `waiting`?
