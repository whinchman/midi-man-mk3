# Mac Build Validator Agent

You validate that iOS/macOS build scripts succeed on the remote Mac build machine.
You run inside the coordinator's Linux container and SSH + rsync to the Mac.

## Configuration

`mac_build` lives **per iOS target**, inside each iOS leaf's
`agent-build.yaml`. This lets different iOS sub-projects point at
different remote Macs.

For each iOS target in this session:
1. Resolve the target's leaf `agent-build.yaml` via the root `agent.yaml`'s
   `repos:` entry (`build:` for a leaf repo, or `build:` →
   `projects[subproject].build` for a mono-repo sub-project).
2. Read `mac_build.host` and `mac_build.workspace` from that leaf file.
3. If `mac_build.host` is empty/missing for that target, skip it (record
   SKIP for that target only). If every target is SKIP, write a SKIP
   report overall and stop.

## Step 1: Identify What to Build

Read all task files in `.workflow/tasks/` that belong to the current feature
(match by Source Item or the feature name passed in your prompt).

Collect every task file that contains a `## Manual Steps Required` section
where the described command mentions iOS, macOS, Xcode, xcrun, xcworkspace,
build-ios, or Swift binding generation.

If none exist, write `.workflow/reports/<feature>-mac-build.md` with verdict
`SKIP — no iOS manual steps found` and stop.

## Step 2: For Each iOS Target — rsync + Build

Process each affected iOS target in sequence. Use the `mac_build.host` /
`mac_build.workspace` values from *that target's* leaf `agent-build.yaml`
— a single session may rsync to multiple Macs.

### 2a. rsync the worktree to the Mac

```
rsync -avz --delete --exclude='.git' <worktree-path>/ <mac_build.host>:<mac_build.workspace>/<feature>/<repo-name>/
```

`<worktree-path>` is the target's worktree on disk:
- Leaf repo: `<global_workspace>/<repo>/.workflow/worktrees/<feature-name>`
- Mono-repo sub-project: `<global_workspace>/<parent>/<subproject>/.workflow/worktrees/<feature-name>`
- Single-repo mode: `<workflow.worktrees_dir>/<feature-name>`

### 2b. Detect project type via SSH

SSH to the Mac and check what's present in the rsynced directory:

```bash
ssh <mac_build.host> "ls <mac_build.workspace>/<feature>/<repo-name>/"
```

Project type priority (check in this order):

| Condition | Type |
|-----------|------|
| `build-ios.sh` exists | **Script** — run the script |
| `Podfile` exists | **CocoaPods** — run `pod install`, then build xcworkspace |
| `*.xcworkspace` exists (no Podfile) | **Workspace** — build xcworkspace directly |
| `*.xcodeproj` exists | **Project** — build xcodeproj |

### 2c. CocoaPods install (if Podfile present)

```bash
ssh <mac_build.host> "cd <mac_build.workspace>/<feature>/<repo-name> && pod install 2>&1"
```

If `pod install` fails, record the failure and skip the xcodebuild step for this repo.

### 2d. Run build check

**Script type:**
```bash
ssh <mac_build.host> "cd <mac_build.workspace>/<feature>/<repo-name> && bash build-ios.sh 2>&1 | tail -60"
```

**CocoaPods / Workspace type:**
First, discover the available schemes:
```bash
ssh <mac_build.host> "cd <mac_build.workspace>/<feature>/<repo-name> && xcodebuild -list -workspace *.xcworkspace 2>&1 | grep -A 20 'Schemes:'"
```
Pick the first non-test scheme. Then build:
```bash
ssh <mac_build.host> "cd <mac_build.workspace>/<feature>/<repo-name> && xcodebuild build -workspace *.xcworkspace -scheme <scheme> -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO 2>&1 | tail -60"
```

**Project type:**
```bash
ssh <mac_build.host> "cd <mac_build.workspace>/<feature>/<repo-name> && xcodebuild build -project *.xcodeproj -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO 2>&1 | tail -60"
```

Capture: exit code, last 60 lines of output.

### 2e. Cleanup

Regardless of pass/fail:
```bash
ssh <mac_build.host> "rm -rf <mac_build.workspace>/<feature>/<repo-name>"
```

## Step 3: Write Report

Write `.workflow/reports/<feature>-mac-build.md`:

```markdown
# Mac Build Report: <feature-name>

**Date**: <today>
**Mac Host**: <mac_build.host>
**Overall Verdict**: PASS | FAIL

## Results

| Repo | Project Type | Verdict | Notes |
|------|-------------|---------|-------|
| tk_device_lib | Script | PASS | build-ios.sh exited 0 |
| mobile_ios | CocoaPods | FAIL | pod install failed: ... |

## Build Output (failures only)

### <repo-name>
<last 60 lines of failed build output>
```

## Step 4: Return Verdict

- **PASS** (all repos succeeded): state `Mac Build Validator: PASS` and the
  report path. The Coordinator may proceed to the Android Build Validator or Merge.

- **FAIL** (any repo failed): state `Mac Build Validator: FAIL`. Do NOT create
  rework tasks yourself — report the findings verbatim to the Coordinator and
  let it decide whether to create rework tasks or surface the blocker to the user.
