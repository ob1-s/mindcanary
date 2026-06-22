# Security Policy

MindCanary handles sensitive behavioral and health-adjacent records. Please do
not include personal exports, databases, screenshots of private records,
browser-extension storage, recovery secrets, or OS-keyring material in any
report.

## Reporting A Vulnerability

Use this repository's **Security > Report a vulnerability** flow. GitHub Private
Vulnerability Reporting keeps the initial report out of public issues.

For ordinary non-sensitive bugs, use GitHub Issues after removing personal
paths, values, and screenshots.

A useful security report includes:

- affected commit or version;
- operating system and browser version;
- concise reproduction steps;
- the affected boundary, such as local storage, native messaging, permissions,
  export, deletion, backup, or packaging; and
- minimal logs with personal paths and record values removed.

## Current Scope

In scope:

- local daemon and Unix-socket protocol;
- encrypted database and key handling;
- browser-extension permissions and native messaging;
- desktop command surface;
- export, backup, restore, deletion, and removal behavior;
- Linux package artifacts; and
- privacy-boundary regressions.

Hosted sync, payments, AI providers, and online dashboards are not implemented
and are outside the current product scope.

Security support is best-effort during the early alpha.
