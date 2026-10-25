# Security policy

## Supported versions

LoTaR is maintained on a single release line. Security fixes go into the next release, so please upgrade to the [latest release](https://github.com/localtaskrepo/lotar/releases/latest) before reporting.

## Reporting a vulnerability

Please do not open a public issue for security problems.

Report it privately through a [GitHub security advisory](https://github.com/localtaskrepo/lotar/security/advisories/new). Include what you found, how to reproduce it, and the impact you expect. If you cannot use GitHub, email [mallox@pyxzl.net](mailto:mallox@pyxzl.net).

LoTaR is maintained by volunteers, so responses are best-effort. You can expect an acknowledgement within a week, and we will keep you updated in the advisory until a fix is released. We are happy to credit you in the advisory and release notes.

## Scope

Things we especially want to hear about:

- The web server (`lotar serve`) or REST API exposing data or accepting writes it should not, including from other sites in the browser
- Agent jobs or automation rules running commands, or writing files, outside what the configuration allows
- Sync leaking credentials, or writing to the wrong remote
- Paths that let task data, attachments or references read or write outside the repository

`lotar serve` is designed for local use. Exposing it to a network without your own authentication in front of it is outside the supported setup.
