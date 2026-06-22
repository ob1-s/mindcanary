# Privacy Policy

Status: early local-alpha policy. This document describes the current product,
not hypothetical hosted services.

## Short Version

MindCanary stores check-ins, annotations, and optional aggregate connector data
on your device. It has no account, telemetry, cloud sync, hosted dashboard, or
AI service. MindCanary's operator does not receive your rhythm record.

MindCanary is not a diagnostic, prediction, emergency, or treatment product.

## Local Records

The encrypted local database may contain:

- check-ins you submit, such as sleep, mood, energy, and context tags;
- annotations you write;
- optional browser aggregate counts and durations;
- optional OS aggregate durations and lifecycle counts;
- collection settings and consent transitions; and
- deterministic daily summaries and descriptive insight evidence.

Browser and OS connectors are optional and default off.

## Content MindCanary Does Not Collect

MindCanary must not store, log, export, or transmit URLs, page titles, page
text, search terms, message contents, screenshots, keystrokes, clipboard
contents, raw browsing history, application names, window titles, document
names, or filenames.

## Telemetry And Accounts

The current application sends no product telemetry and has no account system.
The support-information preview stays local until you intentionally copy it.
It excludes record values, timestamps, filenames, database paths, and
health-adjacent content.

## Export And Backup

Readable export writes plaintext files to a folder you choose. Moving,
uploading, emailing, or synchronizing those files takes them outside the
local-only boundary.

Encrypted backup writes a portable encrypted file and displays a separate
recovery secret. MindCanary does not retain the secret and cannot recover a
backup without it.

## Deletion And Removal

The product distinguishes pausing future collection, deleting one signal's
history, clearing canonical records, clearing the extension's undelivered
queue, destroying the encrypted local profile, and completing app-owned local
removal.

Browser-owned extension storage and user-created exports or backups are not
silently removed by desktop cleanup.

## Security Limit

Local encryption protects copied database files and powered-off devices better
than plaintext storage. It cannot protect against malware running as the same
unlocked operating-system user.

## Support

Do not post personal records in GitHub Issues. Use GitHub Private Vulnerability
Reporting for security concerns and follow [SECURITY.md](../SECURITY.md).
