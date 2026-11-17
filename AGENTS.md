# Rules for agents working in this repository

This repository, `gkuga/zenoh-examples`, reproduces problems in Eclipse Zenoh (`eclipse-zenoh/zenoh`,
"upstream") and shows workarounds and fixes. Read these rules before running any `git` or `gh` command,
and follow them in every session.

## 1. Nothing reaches upstream

Nothing done here may reach upstream or the people who work on it. No issue, PR, comment, mention,
reaction or notification, of any kind.

- **Do not act on upstream.** Do not open, comment on, review, react to, label, close or edit anything in
  `eclipse-zenoh/*`. Reading upstream (`gh api`, web pages, crates.io sources) is allowed.
- **Name the repository.** Every `gh` command that writes takes `--repo gkuga/zenoh-examples`. In a
  clone of a fork, a plain `gh pr create` opens the PR **on the parent repository**.
- **No cross-references.** GitHub links, and notifies, any reference it finds in issue and PR titles and
  bodies, comments, review comments, commit messages, tag messages and release notes. In all of these:
  - do not write `eclipse-zenoh/zenoh#123`, `eclipse-zenoh/zenoh@abc1234`, or `owner/repo#N` /
    `owner/repo@sha` for any repository other than the one the text is posted in;
  - do not paste URLs of upstream issues, PRs, discussions or commits
    (`https://github.com/eclipse-zenoh/...`);
  - do not write `@name` for a person or team. If a name is needed, put it in backticks.

  Refer to upstream in plain words instead: "upstream PR 2096", "upstream issue 2825". A bare `#N`
  means the current repository's own issue or PR `N`; use it only for those.
- **Repository files.** Markdown and code in this repository do not create cross-references, so
  `docs/` may link to upstream issues and PRs. Do not copy those links into commit messages, PR bodies,
  issues or comments.
- **Before posting**, search what you are about to post for `eclipse-zenoh`, `github.com/`, `#` followed
  by digits, and `@`. Each hit must be the current repository's own `#N`, or text inside backticks.

## 2. Public repository

This repository is public. In commits, files, issues and comments, do not mention private repositories
or issues, internal project names, host names, or local paths. Program output pasted into READMEs uses
`$CARGO_HOME` instead of local cargo paths.

## 3. Language

Write commits, code comments, files, issues and PR text in English.

## 4. When unsure

If an action might reach upstream, or you are not sure whether it would, do not do it. Stop and ask.
