# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

The repo is **`thezic/brus-spl`**. `gh` infers it from `git remote -v` when run inside a clone; it
is named here because `.claude/worktrees/` is in use and inference is one less thing to trust.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**: `gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments` then keep only `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR`, or `NONE` (drop `OWNER`/`MEMBER`/`COLLABORATOR`).
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either — resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## The closed history lives in `.scratch/`

`b01`–`b13` and the whole closed `spl-meter-mvp` effort remain markdown files under `.scratch/`.

**The spec is the one thing under there that was never history, and it is no longer under there:**
[`docs/spec.md`](../spec.md), moved out by
[#17](https://github.com/thezic/brus-spl/issues/17). It is the authority for *what* to build and
cites the closed tickets by `bNN` label and by mvp number (`05 d2`), linking the mvp ones as
`../.scratch/spl-meter-mvp/issues/…`. So it is still the way into `.scratch/` — but everything it
points at is history, and the spec itself is not.

So when a skill says "fetch the relevant ticket" and the reference is a `bNN` or an `NN d<n>`,
**read the file — don't reach for `gh`.** Only a GitHub issue number (`#N`) means the tracker.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

**Verified on this repo, 2026-08-11** — both native mechanisms work, so neither fallback below is in
use. One caveat, measured rather than assumed:

- **Sub-issues: yes.** `POST /issues/<map>/sub_issues` with `sub_issue_id` links, and
  `GET /issues/<map>/sub_issues` reads the children back immediately.
- **Dependencies: yes, but `issue_dependencies_summary` lags.** Directly after creating an edge the
  summary on the issue object still read `{"blocked_by":0,…}` while the edge was already real;
  minutes later it read `1`. **So the frontier query must gate on the
  `GET /issues/<n>/dependencies/blocked_by` list, which is accurate immediately — not on the summary
  counter.** Gating on the counter would hand out a blocked ticket as takeable.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`/`build`). Once claimed, the ticket is assigned to the driving dev.
- **`wayfinder:build` is this repo's own fifth type**, for a ticket that produces **code rather than a decision** — the override taken by the build map, whose decisions were all made by the map it succeeds. It is deliberate, not a mislabelled `task`; keep the distinction rather than creating a synonym.
- **Blocking**: GitHub's **native issue dependencies** — the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only — the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task list), drop any with an open blocker or an assignee; first in map order wins. Read blockers from `gh api repos/<owner>/<repo>/issues/<n>/dependencies/blocked_by` — **not** from `issue_dependencies_summary.blocked_by`, which lags behind edge creation on this repo (see above).
- **Claim**: `gh issue edit <n> --add-assignee @me` — the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.
