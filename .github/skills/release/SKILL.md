---
name: release
description: Ship the next lotar release from its "Release vX.Y.Z" ticket, from the version bump to closing the ticket, which starts the checks of the tickets waiting for it.
---

Work from the open `Release vX.Y.Z` ticket (`lotar list 'title:"Release v" -status:done'`).
[docs/release-guide.md](../../../docs/release-guide.md) is the reference for every step;
this is the order and the gates. Comment each step's result on the release ticket.

1. **The fixes are on main.** `lotar task relationships <release> --kind blocks` lists the
   tickets waiting for it. For each, find its commit (its comments name it, or
   `git log main --grep <ID>`) and check `git branch --contains <commit>` lists `main`. A
   fix not on main: comment it on the release ticket and ask the owner whether to wait.
2. **Version bump**: a patch unless the owner asks for a minor, as the guide's
   [Pre-Release Checklist](../../../docs/release-guide.md#pre-release-checklist) says, with
   its local checks, committed on main.
3. **QA green** for that commit: the Quality Assurance run (`gh run list --workflow
   "Quality Assurance" --commit <sha>`) has succeeded, not just started.
4. **Signed tag, pushed, only with the owner's OK.** Pushing a tag publishes the release.
   Ask him first (a HelpNeeded question on the release ticket when you can't ask directly),
   then `git tag -s vX.Y.Z -m vX.Y.Z` and `git push origin vX.Y.Z`.
5. **Release workflow green**: every job of the Release run for the tag succeeded
   ([Cutting a Release](../../../docs/release-guide.md#cutting-a-release)). Re-run a failed
   job; report one that fails again.
6. **Installed here**: `brew upgrade lotar`, then `lotar --version` prints the new version.
7. **Close the release ticket**: `lotar status <release> Done`. That hands each Blocked
   ticket depending on it to @fix to run its check
   ([Tickets waiting for a release](../../../docs/release-guide.md#tickets-waiting-for-a-release)).
   Check that they were assigned.
8. **Open the next release ticket**, `Release vX.Y.(Z+1)`, as the guide says, so the next
   waiting ticket has one to depend on.

Done when the release is installed, its ticket is Done, the waiting tickets are with @fix,
and the next release ticket exists.
