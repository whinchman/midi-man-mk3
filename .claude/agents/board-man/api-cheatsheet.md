# gh API cheatsheet — board-man

Every shell invocation board-man uses, in copy-pasteable form. Substitute
`<owner>`, `<repo>`, `<number>` (project number), `<issue>` (issue number),
`<item>` (project item id), and the cached field/option IDs.

## Authentication

```
gh auth status                                        # check
gh auth refresh -s project                            # add project scope
```

## Project-level

```
gh project view <number> --owner <owner> --format json
gh project create --owner <owner> --title "<title>" --format json
gh project link <number> --owner <owner> --repo <owner/repo>
gh project field-list <number> --owner <owner> --format json
gh project field-create <number> --owner <owner> --name "Parallel Group" --data-type NUMBER --format json
gh project item-list <number> --owner <owner> --format json --limit 200
gh project item-add <number> --owner <owner> --url <issue-url> --format json
gh project item-edit --id <item> --field-id <field> --project-id <project> --single-select-option-id <option>
gh project item-edit --id <item> --field-id <field> --project-id <project> --number <int>
gh project item-edit --id <item> --field-id <field> --project-id <project> --text "<value>"
```

## Labels

```
gh label list --repo <owner/repo> --json name --jq '.[].name'
gh label create <name> --repo <owner/repo> --color <hex> --description "<desc>"
gh label edit <name> --repo <owner/repo> --color <hex> --description "<desc>"
```

## Issues

```
gh issue view <issue> --repo <owner/repo> --json number,title,body,labels,state,id
gh issue view <issue> --repo <owner/repo> --json id --jq .id           # GraphQL node ID
gh issue create --repo <owner/repo> --title "<title>" --body-file <path> --label <L1> --label <L2>
gh issue edit <issue> --repo <owner/repo> --body-file <path>
gh issue edit <issue> --repo <owner/repo> --add-label <label>
gh issue close <issue> --repo <owner/repo>
gh issue comment <issue> --repo <owner/repo> --body-file <path>
```

Comment payload (the create response is plain text; capture the URL line):
```
$ gh issue comment 42 --repo o/r --body-file /tmp/x.md
https://github.com/o/r/issues/42#issuecomment-1234567890
```
The trailing number is the comment ID.

## Pull requests

```
gh pr list --repo <owner/repo> --state open --json number,url,headRefName,comments,reviews,statusCheckRollup --limit 50
gh pr view <pr> --repo <owner/repo> --json number,body,merged,closes
```

## Sub-issues (GraphQL — no native gh subcommand as of 2.87.3)

Add a sub-issue:
```
gh api graphql -f query='
  mutation($p: ID!, $c: ID!) {
    addSubIssue(input: { issueId: $p, subIssueId: $c }) {
      subIssue { id number }
    }
  }
' -f p=<parent-node-id> -f c=<child-node-id>
```

List sub-issues of a parent:
```
gh api graphql -f query='
  query($owner: String!, $repo: String!, $num: Int!) {
    repository(owner: $owner, name: $repo) {
      issue(number: $num) {
        subIssues(first: 50) { nodes { number title state } }
      }
    }
  }
' -f owner=<owner> -f repo=<repo-name-only> -F num=<parent-issue>
```

Get a comment body by ID:
```
gh api repos/<owner>/<repo-name-only>/issues/comments/<comment-id> --jq .body
```

## Setting Status field options (replaces the entire option list)

The CLI doesn't expose this directly; use the GraphQL mutation. Two important
subtleties learned the hard way:

1. **`singleSelectOptions` is a REPLACE, not an append.** Each call wipes
   the prior list. Pass ALL desired options in a single mutation.
2. **The argument is GraphQL syntax, not JSON.** Object keys must be
   unquoted identifiers; enum values (e.g. `GRAY`) are bare. Building this
   as a JSON string and inlining it produces `Expected NAME, actual: STRING`
   parse errors.

Pattern (bash):
```
# Build the GraphQL object-literal array (NOT JSON).
OPTS_GQL=""
for opt in BACKLOG READY TODO IN-PROGRESS DONE; do
    OPTS_GQL+="{name: \"$opt\", color: GRAY, description: \"\"}, "
done
OPTS_GQL="[${OPTS_GQL%, }]"

gh api graphql -F fieldId="<status-field-id>" -f query="
    mutation(\$fieldId: ID!) {
      updateProjectV2Field(input: {fieldId: \$fieldId, singleSelectOptions: $OPTS_GQL}) {
        projectV2Field {
          ... on ProjectV2SingleSelectField {
            options { id name }
          }
        }
      }
    }
"
```

The mutation returns all options with fresh IDs — re-cache them. Items
currently using the field will need their values re-applied; this is fine
on a freshly-created project but be careful on a populated one.

## Label provisioning — case-insensitive collisions

GitHub label names are case-insensitive. Default repos ship with `bug`,
`enhancement`, etc. Trying to `gh label create BUG` on a repo that already
has `bug` will fail with "label already exists; use `--force` to update".

The robust pattern:
```
existing=$(gh label list --repo <r> --json name --jq '.[].name' | grep -ix "^${name}$" || true)
if [ -n "$existing" ]; then
    # --force allows renaming case + updating color/description in one call
    gh label create "$name" --repo <r> --color <hex> --description "<desc>" --force
else
    gh label create "$name" --repo <r> --color <hex> --description "<desc>"
fi
```

## Reading project items — JSON shape quirks

`gh project item-list <n> --owner <o> --format json` returns each item with
top-level keys: `id`, `content`, `labels`, `repository`, `status`, `title`,
plus one key per custom field. **Key names are derived from the field name
with the first word lowercased and spaces preserved.** Examples:

| Field name in UI    | jq path on each item       |
|---------------------|----------------------------|
| `Status`            | `.status`                  |
| `Parallel Group`    | `."parallel Group"`        |
| `Sprint`            | `.sprint`                  |
| `Story Points`      | `."story Points"`          |

Don't assume the JSON key matches the field name verbatim or matches a
slugified form. Either probe with `keys` once and cache, or always derive
from `.content.number` (issue/PR number) which is reliable.

The `.content` shape for an issue: `{number, title, body, ...}`. State for
the underlying issue (OPEN/CLOSED) is **not** populated here; use
`gh issue view <#> --json state` to read it.

## Locking pattern (bash, board-man writes)

```
exec 9>.workflow/temp/.board-man.lock
flock -x -w 30 9 || { echo '{"error":"lock timeout","exit_code":5}'; exit 5; }
# ... write op ...
# fd 9 closes on exit, releasing the lock
```

## ID quick-reference

- Project ID format: `PVT_kwDO...`
- Single-select field ID: `PVTSSF_lADO...`
- Number/text field ID: `PVTF_lADO...`
- Project item ID: `PVTI_lADO...`
- Single-select option ID: 8-char hex (`47fc9ee4`)
- Issue/PR GraphQL node ID: `I_kwDO...` or `PR_kwDO...`
- Issue integer number: `<N>` (used in URLs and `gh issue` commands)

`addSubIssue` requires the GraphQL node ID, NOT the integer number.
`gh issue view <N> --json id --jq .id` returns the node ID.
