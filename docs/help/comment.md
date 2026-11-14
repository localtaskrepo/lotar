# lotar comment

Add or list comments for a task. Supports positional text, message flag, file input, or stdin. If no text is provided, the command lists existing comments.

## Usage

```bash
lotar comment <TASK_ID> [TEXT]
lotar comment <TASK_ID> -m|--message <TEXT>
lotar comment <TASK_ID> -F|--file <PATH>
lotar comment <TASK_ID> <TEXT> [--commit "<SHA>[: WHAT]"]... [--check "<CMD>=<RESULT>[: REASON]"]... [--next <STEP>]

# Alias
lotar c <TASK_ID> [TEXT]

# Alternate form
lotar task comment <TASK_ID> [TEXT]

# Ask a question with options; answer it
lotar comment <TASK_ID> --question <TEXT> --option "<key>: <label>"... [--set "<key>:<field>=<value>"...] [--recommend <key>] [--multiple] [--no-other]
lotar comment <TASK_ID> --answer [<keys>] [--to <DATE>] [NOTE]

# Report a review's verdict
lotar comment <TASK_ID> --review pass|block [--commit <SHA>] [--finding "<finding>"...] [--blocker "<finding>"...] [SUMMARY]
```

- `-F -` reads the comment from stdin. A file that can't be read is an error (exit 1), never a silent listing.
- If neither TEXT, -m, nor -F are provided, the command reads from stdin when piped and data arrives within a moment; otherwise (a terminal, or a pipe that stays silent, as in many agent and CI harnesses) it lists existing comments for the task right away.
- TEXT is optional when using -m/--message or -F/--file.

## Examples

```bash
# Positional text
lotar comment 1 "Investigated and will fix tomorrow"

# Message flag (shell-safe)
lotar comment 1 -m "Re-tested with latest build; still failing"

# From file
lotar comment 1 -F notes/update.txt

# From stdin (multiline)
printf "Line 1\nLine 2\n" | lotar comment 1

# JSON output
lotar --format json comment 1 -m "First note"

# Hand-off report: the text is the summary
lotar comment DEV-12 "Report comments work in CLI, MCP and REST" \
  --commit "5b7385dc: DEV-12 Report comments" \
  --check "npm test=passed" \
  --check "npm run smoke=not_run: no browser in the sandbox" \
  --next "review agent checks the UI"

# List existing comments (no text provided)
lotar comment 1
lotar --format json comment 1
lotar task comment 1
```

Example JSON response:
```json
{
  "status": "success",
  "action": "task.comment",
  "task_id": "AUTH-1",
  "comments": 3,
  "added_comment": {
    "date": "2025-08-19T10:30:42Z",
    "author": "alice",
    "text": "First note"
  }
}
```

Example JSON list response (when no text is provided):
```json
{
  "status": "ok",
  "action": "task.comment.list",
  "task_id": "AUTH-1",
  "comments": 0,
  "items": []
}
```

## Questions and answers

A question comment carries options the reader answers by choosing one, in the web UI by clicking (the Comments tab, board cards and the Tasks list show open questions) or with `--answer`. Use it instead of asking "A, B or C?" in prose.

```bash
# Ask; choosing "fix" assigns the ticket to the @fix agent and reopens it
lotar comment AUTH-1 --question "Rework or close?" \
  --option "fix: Rework with @fix" --set "fix:assignee=@fix" --set "fix:status=Todo" \
  --option "close: Close as WontDo" --set "close:status=WontDo" \
  --recommend fix

# Answer the last open question by its key, with a note
lotar comment AUTH-1 --answer fix "Keep the old endpoint"

# Answer in your own words instead of an option
lotar comment AUTH-1 --answer -- "Ask me again next week"

# Tasks with an open question
lotar list --has question
```

- `--option "<key>: <label>"` adds an option; keys are letters, digits, `-` and `_`. `--recommend <key>` marks one, `--multiple` lets the reader choose several (`--answer a,b`), `--no-other` allows only the options.
- `--set "<key>:<field>=<value>"` sets a field when that option is chosen, as an automation `set:` does: `status`, `assignee`, `reporter`, `priority`, `type`, `due_date`, `effort` or `tags` (comma-separated; replaces the tags). Options only set fields; they never run commands or start anything else. Unknown fields and values the project does not allow are refused when you ask.
- `--answer <keys>` answers the newest open question, or the one asked at `--to <DATE>` (its comment date, or a unique start of it). Without keys, the comment text is a free-text answer. A question is answered once.
- The answer applies the chosen options' fields as an update by the answerer, recorded in history under their name; `on.updated`/`on.assigned` rules fire as for any change, then `on.commented` rules see the answer (`comment: { answer: fix }`, `${{comment.choice}}`; see [Automation](./automation.md#comment-conditions)).
- The answer and its option fields are saved together. A rejected field update leaves the question open so you can retry.
- The comment's `text` keeps the question with its options, or the answer with its labels, in prose, so `lotar show`, older lotar versions and agents reading the YAML see everything. `lotar comment <ID>` and `lotar show` mark a question no answer points to as `[open question]`.
- The stored comment adds `question: { options: [{key, label, set?}], recommended?, multiple?, other? }` or `answer: { to, choice?, note? }`; REST (`POST /api/tasks/comment`) and MCP (`task_comment_add`) take the same `question` and `answer` fields.
## Hand-off reports

A report belongs to a plain comment only. Combining it with a question, answer or review verdict is refused. With `--review`, `--commit <sha>` identifies the reviewed commit; without `--review`, repeat `--commit "<sha>: <what>"` to list report commits.

An agent ending its work adds the parts of its hand-off as a `report` on the comment, so they show as a checklist and a missing "not run" stands out:

- `--commit "<sha>: <what>"`: a commit and what it is (repeatable). The hash is 4 to 64 hex digits.
- `--check "<command>=<result>[: reason]"`: a check and its result, `passed`, `failed` or `not_run` (`not-run` works too), with why it failed or did not run (repeatable). The command may contain `=` itself (`FOO=1 npm test=passed`).
- `--next "<step>"`: the next step.

Every part is optional; the text stays required and is the prose summary, so tools that don't know reports lose nothing. The comment stores the report under its text:

```yaml
- date: 2026-11-13T21:20:32Z
  author: claude
  text: Report comments work in CLI, MCP and REST
  report:
    commits:
    - hash: 5b7385dc
      what: DEV-12 Report comments
    checks:
    - command: npm test
      result: passed
    - command: npm run smoke
      result: not_run
      reason: no browser in the sandbox
    next: review agent checks the UI
```

`lotar comment <ID>` and `lotar show` print the report under the text (`check npm run smoke: not run (no browser in the sandbox)`); JSON output includes it as `report`. The web UI shows the commits, linked to their page when the git remote is on GitHub, GitLab, Bitbucket or Codeberg, and the checks with pass, fail and not-run marks. It says "No check marked not run" when a report lists none. The Comments tab's Type filter has a "Hand-off" chip to show or hide report comments. MCP `task_comment_add` and `POST /api/tasks/comment` take the same `report` object.

## Reviews

A review comment records a reviewer's verdict, the commit reviewed and the findings. The web UI shows the verdict as a badge on the comment and the newest verdict next to the task's status; rules match it with `comment: { review: block }`. Use it instead of writing "Review: passed" in prose.

```bash
# The review passed
lotar comment AUTH-1 --review pass --commit 5b7385dc "Checked the migration and its tests."

# The review blocks the work, with one blocking and one other finding
lotar comment AUTH-1 --review block --commit 5b7385dc \
  --blocker "src/auth.rs:42: the token is logged" \
  --finding "docs/help/auth.md: the example is out of date" \
  "The change leaks the token."
```

- `--review pass` lets the work go on, `--review block` sends it back. The comment text is the reviewer's summary; it may be left out.
- `--commit <SHA>` names the commit the review looked at.
- `--finding` adds a finding, `--blocker` a finding that must be fixed before the work goes on. Both take `"[<file>[:<line>]: ]<summary>"`: the part before the first `": "` is the location when it has no spaces and contains a `/` or `.`. A `pass` review cannot have a blocker.
- The comment's `text` starts with "Review passed." or "Review blocked." (with " at <commit>"), then the summary, then the findings in prose, blocking ones first, so `lotar show`, older lotar versions and agents reading the YAML see everything. `lotar comment <ID>` and `lotar show` mark the comment `[review: pass]` or `[review: block]`.
- The stored comment adds `review: { verdict, commit?, findings?: [{summary, file?, line?, blocking?}] }`; REST (`POST /api/tasks/comment`) and MCP (`task_comment_add`) take the same `review` field, with `text` as the summary.

## Notes

- Each comment stores a UTC timestamp (RFC3339), its author and the text. The author is the identity the history records as actor (`default_reporter` such as `LOTAR_DEFAULT_REPORTER`, then git or system user; see [Identity & Users](./identity.md)). Comments written before lotar stored authors have no `author` and show without one.
- `lotar comment <ID>` and `lotar show` print the author before the text (`2025-08-19T10:30:42Z  alice: First note`).
- Project context is inferred from the numeric ID using your current repo (or configured `default_project`). Pass a fully-qualified ID (`AUTH-1`) or `--project` when multiple prefixes exist.
