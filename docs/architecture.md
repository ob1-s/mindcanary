# Architecture

Status: implemented local Linux alpha.

MindCanary is a local-first desktop application. The desktop UI never opens the
database directly; a narrow Rust daemon owns storage, consent enforcement,
aggregation, and deterministic insight generation.

## Design Constraints

- The app remains useful with check-ins only.
- Every connector and higher-risk permission is optional.
- Browser and OS records are aggregates, not content logs.
- Rust protocol types are the source of truth for every process boundary.
- Insights describe sustained changes from personal history. They do not
  diagnose, predict, score, or prescribe action.
- Missing data remains missing and is never silently treated as zero.

## Components

### Desktop

The desktop application uses Tauri 2, React, and TypeScript. It provides:

- onboarding and check-ins;
- user-owned day and time-window annotations;
- daily history and descriptive baseline cards;
- connector consent, pause, and per-signal deletion controls;
- export, encrypted backup and restore, and complete local-removal flows; and
- a preview-only support report that excludes record values and paths.

Tauri commands expose fixed operations. They do not accept arbitrary commands,
executables, or runtime arguments from the webview.

### Local Daemon

`mindcanaryd` owns canonical state and communicates through a user-only Unix
domain socket. It validates protocol versions, source consent, period bounds,
metric allowlists, replay identifiers, and destructive confirmations.

The daemon can run independently of the desktop window so optional connectors
continue delivering aggregates while the UI is closed.

### Storage

Canonical records live in a SQLCipher database. A random database key is stored
through the operating-system keyring rather than beside the database.

Stored record families include:

- check-ins and selected context tags;
- user annotations;
- timestamped per-signal consent transitions;
- approved 15-minute aggregate batches;
- source status and replay identifiers; and
- deterministic daily projections and insight evidence.

### Browser Connector

The Manifest V3 extension observes browser lifecycle events and reduces them
into aligned 15-minute aggregates before native messaging. The native host can
forward only an allowlisted subset of protocol requests.

The extension does not request browsing-history permission or broad host
access. The optional continuous-scrolling adapter requires a separate
site-specific grant for `x.com` or `twitter.com`.

### OS Connector

The current OS adapter supports GNOME/X11 active and idle duration. Optional
lock, unlock, suspend, and resume count streams use local GNOME ScreenSaver and
logind events where available. No application, window, or document name is
collected.

## Data Flow

```text
check-in / annotation --------------------+
                                            |
browser events -> local aggregation -------+-> daemon validation
                                            |      |
OS events ------> local aggregation --------+      v
                                               encrypted storage
                                                      |
                                                      v
                                      daily history and deterministic
                                      sustained-window descriptions
```

Consent is enforced at both the source and daemon. A delayed batch cannot
backfill a period that was not continuously enabled.

## Insight Boundary

The Today view contains inputs and same-day facts only. Baseline comparisons
appear in History only after enough prior observations exist. Insight cards
compare multi-day windows, expose supporting dates and coverage, and abstain
when evidence is missing or unstable.

Annotations remain user-owned context and are excluded from automated scoring.

## Packaging

The Linux `.deb` installs the desktop binary, daemon, native host, and fixed
helpers. The desktop manages a `systemd --user` daemon service. Complete local
removal distinguishes app-owned records and registrations from browser-owned
extension storage and user-created exports or backups.

## Deliberate Exclusions

The current product has no account, telemetry, hosted sync, hosted dashboard,
payment system, medical prediction, emergency monitoring, or AI-controlled
analysis.
