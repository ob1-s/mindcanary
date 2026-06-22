# Data And Privacy

Status: current local Linux alpha boundary.

MindCanary is designed so the operator does not possess the user's rhythm
record. The current application has no account, telemetry, hosted storage, or
remote analysis service.

## Stored Locally

Depending on the features a user enables, the encrypted local database may
contain:

- check-ins such as sleep, mood, energy, and selected context tags;
- user-authored day or time-window annotations;
- browser aggregate counts and durations in aligned 15-minute periods;
- OS active and idle duration and optional lifecycle counts;
- source state, consent transitions, and replay identifiers; and
- deterministic daily summaries and insight evidence.

Every browser and OS signal is optional. Disabling a signal stops future
collection; deleting its history is a separate confirmed operation.

## Never Stored Or Transmitted

MindCanary must not store, log, export, or transmit:

- URLs, page titles, page text, search terms, or message contents;
- browsing history, screenshots, keystrokes, or clipboard contents;
- application names, window titles, document names, or filenames;
- social-media account identity; or
- inferred diagnosis, clinical phase, dangerousness, or crisis state.

Tests, fixtures, logs, support reports, and protocol payloads follow the same
boundary.

## Local Protection

Canonical records use a SQLCipher database. The database key is generated
locally and stored through the operating-system keyring. The daemon owns all
database access and listens on a user-only local socket.

This protects copied database files and powered-off devices better than
plaintext storage. It cannot protect against malware or another process already
running with the same unlocked OS-user privileges.

## Browser Boundary

The browser extension reduces transient tab identifiers and lifecycle events
into approved aggregate values. Sensitive browser fields are not included in
storage or native-messaging payloads.

The extension has a bounded local retry queue. Clearing that queue removes only
undelivered aggregates; it does not remove records already accepted by the
daemon.

## Export And Backup

Readable export creates plaintext reports and CSV files in a folder selected by
the user. Once moved, emailed, or synchronized, those files are outside the
local-only boundary.

Encrypted backup creates one portable SQLCipher file and a separate recovery
secret. MindCanary does not retain the secret. Restore is allowed only into an
empty local record set so existing history is not silently replaced.

## Deletion And Removal

MindCanary distinguishes:

- pausing future collection;
- deleting one signal's stored history;
- clearing canonical local records;
- clearing the browser extension's undelivered queue;
- destroying the encrypted database profile and key; and
- complete app-owned local removal, including service and native-host
  registrations.

Browser-owned extension storage and user-created exports or backups are never
silently claimed as removed by desktop cleanup.

## Logs And Support Information

Production logs use event names and error classes, not record interpolation.
The desktop support preview contains versions, coarse platform information,
and source status labels. It excludes record values, timestamps, filenames,
database paths, and connector details, and is never sent automatically.
