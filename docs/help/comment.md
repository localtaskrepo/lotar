# Comment

Quickly add a comment to a task.

## Usage

- Add a comment

  lotar comment <ID> "Your comment text"

- Shorthand alias:

  lotar c <ID> "Your comment text"

## Notes

- Author is resolved from your local identity (config default → git user.name/email → system username).
- Timestamp is set to the current time in UTC.
- JSON output includes the action and new comment count.

## Examples

- Text output

  lotar comment AUTH-1 "Investigated and will fix tomorrow"

- JSON output

  lotar --format json comment AUTH-1 "First note"

  {"status":"success","action":"task.comment","task_id":"AUTH-1","comments":3}
