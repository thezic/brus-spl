# Plan: move the tracker to GitHub Issues

## Goal

The engineering skills (`/wayfinder`, `/to-tickets`, `/to-spec`, `/triage`) read a per-repo config
to learn where issues live. That config now says **GitHub**. This plan does the rest: create the
label vocabulary, move the *open* work onto GitHub Issues, and fix every piece of prose that still
tells a reader the tracker is local markdown.

**Not a code change.** Nothing under `src/` or `src-tauri/` is touched, so no `cargo` or
`npm run build` gate applies.

## Already done (the setup skill, this session)

- `docs/agents/issue-tracker.md` — the GitHub template, plus three deltas: repo named explicitly,
  `wayfinder:build` documented as this repo's deliberate fifth ticket type, and a rule that a `bNN`
  or `NN d<n>` reference means **read the file in `.scratch/`**, not `gh`.
- `docs/agents/triage-labels.md` — the five canonical roles, mapped to themselves.
- `docs/agents/domain.md` — single-context (root `CONTEXT.md` + `docs/adr/`, neither existing yet).
- `CLAUDE.md` — an `## Agent skills` section between `## Project state` and `## Commands`.

Uncommitted. This plan's changes join them in one commit at the end, on a branch, if asked.

## Decisions already taken

| Decision | Choice |
|---|---|
| Tracker | GitHub Issues on `thezic/brus-spl` |
| Triage labels | defaults, unchanged |
| Migration scope | **open work only** — the build map + `b14` + `b15` |
| `.scratch/spl-meter-*` rename | no |

**Why open-work-only.** `docs/spec.md` is the authority for *what* to build and cites tickets by
path (`issues/b13-…`) and by mvp number (`05 d2`); `gh` cannot backdate, so 24 closed
tickets would all land stamped today, turning a real chronology into a false one. `map.md` is 667
lines, ~400 of them "Decisions so far" — a record, not a live document. The tracker earns its keep
on open work.

**Public repo.** `.scratch/` is already tracked in a public repo, so nothing becomes newly public.

---

## Step 1 — Create the labels

Ten `gh label create` calls. Colours are cosmetic; the strings are not.

```bash
# wayfinder vocabulary — the map, then the five ticket types
gh label create "wayfinder:map"       --color 5319e7 --description "A wayfinder map"
gh label create "wayfinder:research"  --color 1d76db --description "Wayfinder ticket: research"
gh label create "wayfinder:prototype" --color 1d76db --description "Wayfinder ticket: prototype"
gh label create "wayfinder:grilling"  --color 1d76db --description "Wayfinder ticket: grilling"
gh label create "wayfinder:task"      --color 1d76db --description "Wayfinder ticket: task"
gh label create "wayfinder:build"     --color 1d76db --description "Wayfinder ticket: produces code, not a decision"

# triage roles — wontfix already exists in this repo
gh label create "needs-triage"    --color d93f0b --description "Maintainer needs to evaluate this issue"
gh label create "needs-info"      --color fbca04 --description "Waiting on reporter for more information"
gh label create "ready-for-agent" --color 0e8a16 --description "Fully specified, ready for an AFK agent"
gh label create "ready-for-human" --color 0052cc --description "Requires human implementation"
```

The whole wayfinder vocabulary is created, not just the two types in use, so the next effort's map
works without a detour.

## Step 2 — The map issue

One issue, labelled `wayfinder:map`, titled **SPL Meter Build**. Expected to be `#1` (no issues and
no PRs exist yet, so the number space is untouched).

**The body is written fresh, not copied.** Copying a 667-line map whose Route is 13/15 complete
would produce two authorities and one of them would rot. The issue carries only what is still live:

- **Destination, and that it is reached** — the venue run closed it; what remains is what the talk
  asked for, not what it was missing.
- **The two open tickets**, as sub-issues.
- **The execution override** — this map carries execution; tickets produce code, hence
  `wayfinder:build`.
- **Done, for a build ticket** — the five-item list from `map.md` §Notes, verbatim, including the
  iOS `cargo check --target aarch64-apple-ios --lib` clause and the device pass.
- **Standing preferences** — "Resist complexity", and the rest of that list.
- **A pointer to the archive**: the full map, its Route, its ~400 lines of Decisions-so-far and the
  13 closed tickets stay at `.scratch/spl-meter-build/map.md`, linked as a `blob/main` URL.

Everything else — Route, Drop order, Decisions so far, Not yet specified, Out of scope — stays in
the file. It is history, and history reads better as a document than as an issue body.

## Step 3 — `b14` and `b15` as child issues

Two issues, label `wayfinder:build`, titles taken from the ticket H1s:

- **The live number is too busy** (`b14`) — expected `#2`
- **The picture's frequency axis** (`b15`) — expected `#3`

Bodies are the ticket files' `## The work` onward, carried over whole. **The arguments are the
value** — `b14`'s "why coarsening the live number is the only move left", `b15`'s "do not answer
this by adding bands" — and they get lost if summarised.

Transformations applied to each body:

| In the file | In the issue |
|---|---|
| `Parent: [SPL Meter Build](../map.md)` | a native sub-issue link to `#1` (see below) |
| `Status: open`, `Labels: …`, `Blocked by: —` | dropped — GitHub state, labels and dependencies say this now |
| `Type: build` | kept |
| `[the venue run](b13-the-venue-run.md)` and siblings | `blob/main` URLs into `.scratch/spl-meter-build/issues/` |
| `§11.4`, `§7.1`, `§17` | unchanged — the spec stays a file; one `blob/main` link to `spec.md` is added at the top of each body so the § numbers are one click from resolvable |

`blob/main` rather than a commit-SHA permalink: the archive is not moving, and a reader wants the
current file, not a snapshot of it.

**Linking is the sub-issue capability probe.** The primary path is GitHub's sub-issues API:

```bash
gh api --method POST repos/thezic/brus-spl/issues/1/sub_issues -F sub_issue_id=<db-id of #2>
# db id, not the number:
gh api repos/thezic/brus-spl/issues/2 --jq .id
```

If that 404s or errors, take the fallback the tracker doc already documents: a task list in `#1`'s
body plus `Part of #1` at the top of each child. **Either outcome is a success for this plan** —
what matters is that the doc ends up describing what actually works here.

Neither ticket blocks the other (both are `Blocked by: —`), so no dependency edge is real.

## Step 4 — Probe the dependency API, then record the truth

`blocked_by` is the tracker doc's primary blocking mechanism and **this migration never exercises
it**, so the next wayfinder session would be the one to find out it doesn't work. Probe it with a
temporary edge on the real issues and remove it:

```bash
BLOCKER=$(gh api repos/thezic/brus-spl/issues/2 --jq .id)
gh api --method POST   repos/thezic/brus-spl/issues/3/dependencies/blocked_by -F issue_id=$BLOCKER
gh api repos/thezic/brus-spl/issues/3 --jq .issue_dependencies_summary   # expect blocked_by: 1
gh api --method DELETE repos/thezic/brus-spl/issues/3/dependencies/blocked_by/$BLOCKER
gh api repos/thezic/brus-spl/issues/3 --jq .issue_dependencies_summary   # expect blocked_by: 0
```

Net state unchanged; capability known. Then add one line to
`docs/agents/issue-tracker.md` recording what was **verified on this repo** — sub-issues yes/no,
dependencies yes/no, with the date. The fallback prose stays either way; a future GitHub change
should be caught by re-probing, not by trusting a claim.

## Step 5 — Fix the prose that still says "local markdown"

Four files. This is the step that actually decides whether a fresh agent gets it right.

**`.scratch/spl-meter-build/map.md`** — a banner under the H1 saying open work moved to
GitHub `#1` and this file is now the historical record; and the `**Tracker:** local markdown, as
before` paragraph in §Notes rewritten: the map is issue `#1`, tickets are issues,
`gh issue list --label wayfinder:build` is the query, and claiming is
`gh issue edit <n> --add-assignee @me` rather than `Status: claimed`.

**`.scratch/spl-meter-build/issues/b14-…md` and `b15-…md`** — reduced to a stub: title, a line
saying the live ticket is GitHub `#2` / `#3`, and nothing else. **This is the one judgement call
worth arguing with.** Leaving the full text in place creates two copies of a live ticket that will
drift, and the drift is silent; a stub keeps `ls issues/` complete and the `b`-numbering unbroken
while leaving exactly one authority. The alternative — delete the files — breaks inbound links from
`map.md` and `CLAUDE.md`.

**`CLAUDE.md`** — two paragraphs in `## Project state`:

- "**Implementation runs from `.scratch/spl-meter-build/`** — `map.md` is the index, `issues/b01`–`b15`
  are the tickets. **Read that map before starting work.**" → open work is GitHub `#1` and its
  sub-issues; `.scratch/spl-meter-build/` is the archive of 13 closed tickets and the reasoning;
  read both, and the sentence about the two numbering schemes stays true and stays.
- "**Two tickets are open …** [`b14`](…) … [`b15`](…)" → link the issues, keep the `b14`/`b15`
  labels in the prose (they are how the archive names them) and keep every substantive warning,
  especially *do not answer `b15`'s wish by adding bands*.

## Step 6 — Verify

```bash
gh issue list --state open --json number,title,labels,assignees \
  --jq '[.[] | {number, title, labels: [.labels[].name]}]'      # 3 issues, labels as intended
gh api repos/thezic/brus-spl/issues/1/sub_issues --jq '[.[].number]'   # [2,3] if native
gh label list | grep -E "wayfinder|needs-|ready-for-"                  # 10 labels
grep -rn "local markdown" CLAUDE.md .scratch/ docs/                    # only the historical note
```

Then extract every URL from the three issue bodies and check each resolves — a rewritten link that
404s is the most likely defect in this whole plan, and the cheapest to catch.

## Out of scope

- Full-history migration of the 13 closed `b`-tickets and the closed `spl-meter-mvp` map — decided
  against.
- Renaming `.scratch/spl-meter-*` — decided against.
- Creating `CONTEXT.md` or `docs/adr/` — `docs/agents/domain.md` says these are created lazily by
  `/domain-modeling` and their absence is to be passed over silently.
- Doing any of `b14` or `b15`'s actual work.
- Committing or pushing without being asked.

## Risks

- **Sub-issues or dependencies may be unavailable.** Mitigated: both fallbacks are already
  documented in the tracker doc, and Step 4 records which path is real.
- **Two copies of a live ticket.** The reason Step 5 stubs the files rather than leaving them.
- **Issue numbers are assumed `#1`–`#3`.** Verified empty first; if they differ, every `#N` in the
  prose changes with them, so the numbers get read back from `gh` rather than assumed after Step 3.
