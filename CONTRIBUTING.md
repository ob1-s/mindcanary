# Contributing

MindCanary welcomes focused bug reports, documentation improvements, tests, and
code contributions that preserve its local-first and non-diagnostic boundary.

## Before Opening An Issue

Do not attach personal exports, databases, recovery secrets, browser-extension
storage, keyring material, or screenshots containing private records. Use
GitHub Private Vulnerability Reporting for security concerns.

## Development

Follow [docs/development.md](docs/development.md) and use an isolated profile for
onboarding, migration, backup, deletion, or removal work.

```bash
pnpm install
pnpm check
```

Rust protocol types are authoritative. After a protocol change, regenerate and
check TypeScript types:

```bash
pnpm protocol:generate
pnpm protocol:check
```

## Product Constraints

- No URLs, titles, page text, search terms, messages, screenshots, keystrokes,
  raw browsing history, application names, or window titles in records, logs,
  fixtures, support output, or protocol payloads.
- Connectors and higher-risk permissions remain optional.
- Insights describe sustained changes from personal history; they do not
  diagnose, predict, score, moralize, or prescribe action.
- Missing data remains missing.
- Export, deletion, backup, and removal wording must state their exact scope.

Keep changes narrow and add the smallest durable test that demonstrates the
behavior being changed.
