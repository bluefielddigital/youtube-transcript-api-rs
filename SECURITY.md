# Security

## Reporting a vulnerability

Please report security problems privately, not in a public issue. Use
GitHub's private vulnerability reporting (the **Report a vulnerability**
button on the repository's **Security** tab), or
[open a draft advisory](https://github.com/bluefielddigital/youtube-transcript-api-rs/security/advisories/new)
directly.

Include what's affected, how to reproduce it, and what an attacker could do
with it. The maintainers aim to acknowledge a report within three working
days, keep you updated during the fix, and credit you in the advisory and the
release notes unless you'd rather stay anonymous.

## Supported versions

Fixes go into the newest release. Until 1.0, that's the newest `0.x`
version: upgrade to it to get a fix.

| Version | Supported |
| --- | --- |
| Newest 0.x release | Yes |
| Older releases | No |

## Scope

In scope: the library and the `youtube-transcript-api` binary, including the
release archives. They parse responses from YouTube and from proxies, so a
crafted response that makes them panic, hang, use unbounded memory or leak
proxy credentials is a vulnerability. YouTube blocking requests, or changing
its pages so that fetching fails, is a bug rather than a vulnerability: open
an issue.

`cargo deny` checks the dependencies for advisories on every change and
every week.
