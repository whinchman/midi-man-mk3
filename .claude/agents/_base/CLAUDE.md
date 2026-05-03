# Base Agent Instructions

These instructions apply to all agent types. Your type-specific workflow follows
below this section.

## Environment

You are running inside a Docker container:
- The project is mounted at `/workspace` (your working directory)
- The framework is mounted read-only at `/opt/claude-agent`
- Your `agent.yaml` and workflow files are in `/workspace`

You may be running standalone (launched directly) or as a subagent spawned by
the Coordinator. Either way, follow your type-specific workflow and update task
files to record your work.

## Hard Rules (apply to all agent types, no exceptions)

- **Worker agents never merge to the default branch.** Coder, QA, Designer,
  Code Reviewer, and Automation agents leave all work on feature branches.
  Only the Coordinator performs merges, and only after the full pipeline
  (Code Review → QA → Integration check) has passed.

- **Worker agents never push to remote.** `git push` is performed only by the
  Coordinator, and only when `workflow.push_enabled` is `true`. If you are a
  worker agent and feel the urge to push, stop — mark your task `done` and
  report back to the Coordinator instead.

- **When in doubt, stop and report.** If you finish your assigned work and
  there is no clear next step in your instructions, mark your task `done`,
  summarize what you did, and stop. Do not invent follow-on work.

## Pre-flight Checks

Before doing any work:

A) **Read `agent.yaml`** and internalize the project configuration. Every path,
   command, and convention referenced below comes from that file.

B) Ensure you are on the correct branch for your role:
   - **Manager/Architect agents**: work on the default branch (`git.default_branch`)
   - **Worker agents** (coder, code-reviewer, designer, automation, qa): work
     in a dedicated git worktree on a feature branch

C) Ensure the working directory is clean. If there are uncommitted changes,
   stash them and warn.

D) Sync with remote:
   - If behind remote: `git pull`
   - If ahead of remote: `git push`
   - If diverged: `git pull --rebase` then `git push`

## Workflow Pipeline

Work flows through three stages:

```
BACKLOG.md → [Refinement] → TODO.md → [Implementation] → [Test] → DONE.md
```

| File | Contains | Who writes | Who reads |
|------|----------|-----------|-----------|
| `BACKLOG.md` | Raw unprocessed features, changes, and issues | Stakeholder | Coordinator, Architect |
| `TODO.md` | Refined work with plans and tasks | Coordinator | Worker agents |
| `DONE.md` | Completed and merged | Coordinator | Stakeholder |
| `BUGS.md` | Bugs found by QA / Code Reviewer | QA, Code Reviewer | Stakeholder, Coordinator |

**Critical rule**: when moving an item from one file to the next, **remove it
from the source file**. Nothing should exist in two files at once.

### Pipeline Stages

1. **Refinement** (BACKLOG → TODO): The Coordinator spawns Architect, Designer,
   and Manager subagents in sequence. The Architect produces plans, the Designer
   produces design specs, and the Manager decomposes everything into task files.
   Items from any of the three backlog sections (Features, Changes, Issues)
   move from `BACKLOG.md` to `TODO.md` with full plans and tasks.

2. **Implementation** (TODO → working): Coder, Code Reviewer, and QA subagents
   work through tasks — implementing, reviewing, and testing.

3. **Test** (working → DONE): The Automation subagent generates CI/CD, test
   infrastructure, and deployment configs. Completed items move to `DONE.md`.

### Bugs

QA and Code Reviewer agents write bugs to `workflow.bugs_file`
(`.workflow/BUGS.md`). Each bug entry should include the file, line, severity,
and reproduction steps.

## Task File Format

Tasks are the coordination protocol between agents. They live in the `.workflow/tasks/`
directory (or the path specified by `agents.tasks_dir` in `agent.yaml`).

A task file (`.workflow/tasks/<task-id>.md`) has this structure:

```markdown
# Task: <task-id>

- **Type**: architect | coder | code-reviewer | designer | automation | qa
- **Status**: pending | in-progress | done | failed
- **Branch**: feature/<task-id>
- **Source Item**: <reference to the workflow item>
- **Dependencies**: <task-ids that must complete first>

## Description
<What needs to be done>

## Acceptance Criteria
- [ ] Criterion 1

## Notes
<Feedback from coordinator, worker notes>
```

**Status transitions**:
- `pending` → `in-progress`: worker picks up the task
- `in-progress` → `done`: worker completed the task successfully
- `in-progress` → `failed`: worker could not complete the task (add reason to Notes)
- `done` → merged by coordinator (task file archived or deleted)

## Context Window Management

If you are running low on context mid-task:
1. Complete the current step
2. Commit your work
3. Note in the plan file or task file which step you stopped at
4. The next agent session will resume from that point

## Code Standards

Follow the rules in the `code_standards` section of `agent.yaml`. Read them
during pre-flight and apply them to every file you create or modify.

Display terminal commands on a single uninterrupted line (no backslash line
continuations).
