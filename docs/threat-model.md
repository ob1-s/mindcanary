# Threat Model

Status: current local Linux alpha.

## Protected Assets

- check-ins, annotations, and context tags;
- browser and OS aggregate records;
- consent history and insight evidence;
- the SQLCipher database key and backup recovery secrets; and
- the integrity of deletion, export, backup, and connector controls.

## Trust Boundaries

- React webview to fixed Tauri commands;
- Tauri shell to the local daemon socket;
- browser extension to the native-messaging host;
- native host to the daemon protocol;
- daemon to SQLCipher and the OS keyring; and
- user-created exports and backups leaving app-owned storage.

## Relevant Attackers And Failures

- a malicious or compromised browser extension update;
- another local process running as the same user;
- malformed or replayed native messages;
- protocol downgrade or source impersonation;
- accidental logging of private values;
- copied database, export, or backup files;
- misleading deletion or support wording; and
- dependency or release-artifact compromise.

## Current Mitigations

- no browsing-history permission or broad host access;
- strict protocol schemas and metric allowlists;
- native-host origin restrictions and administrative-request rejection;
- daemon-side consent checks over the complete aggregate period;
- idempotent batch and check-in identifiers;
- user-only local IPC and fixed packaged helper paths;
- SQLCipher encryption with an OS-keyring key;
- short-lived confirmations for destructive operations;
- separate extension queue reset, record deletion, and complete removal;
- preview-first diagnostics with no automatic upload; and
- privacy scans and isolated lifecycle tests in the release workflow.

## Residual Risks

- Malware running as the same unlocked user can observe local data or UI.
- Browser permissions still grant observation capability to extension code even
  when MindCanary deliberately stores less.
- Plaintext exports inherit the security of the folder chosen by the user.
- An encrypted backup is unrecoverable without its recovery secret.
- Unsigned early-alpha Linux packages rely on GitHub transport and published
  checksums rather than a mature signing identity.
- Bugs can still violate documented boundaries; this is why security reports
  and reproducible source review matter.

## Security Invariants

1. No URL, title, page text, search term, message content, screenshot,
   keystroke, or raw browsing history crosses a protocol boundary.
2. A connector cannot enable its own canonical storage permission.
3. Delayed delivery cannot backfill a period that was paused.
4. Replaying the same batch does not duplicate records.
5. The desktop cannot execute arbitrary daemon commands or paths.
6. Product removal never claims to delete browser-owned storage or user-owned
   exports and backups.
7. Deterministic insight evidence remains inspectable and non-diagnostic.
