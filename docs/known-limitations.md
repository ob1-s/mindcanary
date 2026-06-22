# Known Limitations

Status: local-alpha draft.

## Product Limitations

- MindCanary is not a diagnostic, prediction, emergency, or treatment product.
- Insights are descriptive comparisons against local records, not clinical
  interpretations.
- The app may be useful before baseline history exists as a logbook, but
  pattern explanations require enough comparable data.
- Missing days remain missing. They are not treated as zero.
- Baseline descriptions currently pool all prior days containing each signal.
  User-defined work/non-work comparison calendars are not implemented. A
  sustained schedule-related change may therefore be described without
  MindCanary claiming what caused it; supporting dates remain visible.
- Switching and active-time descriptions normalize by recorded coverage. They
  describe activity during observed periods, not whole-day behavior when a
  connector was absent.
- Tracking can increase rumination for some people. Pause and deletion controls
  are part of the product, not edge cases.

## Platform Limitations

- The first supported packaged target is Linux on Pop!_OS/Ubuntu-like systems.
- GNOME/X11 active/idle duration is the first OS adapter.
- Wayland, KDE, macOS, Windows, mobile, and foreground-app categories require
  separate adapters and review.
- Lock/unlock and suspend/resume counts require the GNOME ScreenSaver and
  logind event streams. They remain unavailable when those local services do
  not connect.
- The browser extension is optional. The desktop app remains useful with
  check-ins only.

## Browser Connector Limitations

- The extension stores only aggregate batches and operational queue state, not
  URLs, page titles, page text, search terms, or browsing history.
- Browser aggregates are delivered in aligned 15-minute periods, so the desktop
  may not update immediately after installation.
- Disabling or removing the extension is controlled by the browser. MindCanary can
  show that browser data has not arrived recently, but it cannot manage
  browser-owned extension storage from the desktop app.
- The extension retry queue is bounded. If the daemon is unavailable for long
  enough, old unsent batches are dropped and counted rather than retained
  indefinitely.
- Firefox builds and native-host manifests are implemented, but temporary
  add-on installation and native-message delivery still need real-session
  validation. Chrome remains the first packaged connector.
- Use only one browser connector at a time. Records do not yet carry connector
  provenance, so overlapping Chrome and Firefox collection would double-count
  shared activity.
- Continuous-scrolling collection is an optional aggregate-only adapter for
  `x.com` and `twitter.com`; it is not a general website tracker.

## Data And Removal Limitations

- Clearing records inside the app is not the same as uninstalling MindCanary.
- Destroying the local database profile removes database files and the
  OS-keyring database key, but it does not remove browser extension storage or
  user-created exports/backups.
- Repeated same-day check-ins remain separate in encrypted storage, while
  History and the current CSV export show their daily average and count. A
  moment-by-moment check-in view and export are not implemented yet.
- Export files are plaintext readable files in a folder the user chooses.
- Losing local records before exporting or backing them up cannot be reversed
  by MindCanary.
- An encrypted backup cannot be restored without its generated recovery
  secret. MindCanary does not retain a recovery copy.

## Security Limitations

- Local encryption protects copied database files and powered-off devices better
  than plaintext storage, but it cannot protect against malware running as the
  same unlocked OS user.
- A malicious authorized extension update can observe whatever browser
  permissions allow. Permission minimization and review reduce this risk but do
  not remove it.
- Hosted sync, hosted dashboards, payments, telemetry, and AI are deferred until
  they have separate security and privacy designs.
