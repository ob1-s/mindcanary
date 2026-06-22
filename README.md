<p align="center">
  <img src="apps/desktop/public/mindcanary-mark.svg" width="96" alt="MindCanary canary mark">
</p>

# MindCanary

MindCanary is a private rhythms journal that combines optional check-ins with
low-detail digital context. It helps people review how their chosen routines
change over time without turning those changes into diagnoses, predictions, or
productivity scores.

The current release is an early Linux alpha. It works with check-ins alone;
browser and operating-system connectors are optional.

## Current Features

- short repeatable check-ins and user-owned day or time-window annotations;
- daily history with explicit missing data;
- descriptive comparisons against sustained windows of personal history;
- optional aggregate browser signals such as tab switching and open-tab counts;
- optional GNOME/X11 active and idle time;
- encrypted local storage with the database key held by the OS keyring;
- readable export, encrypted backup and restore, per-signal deletion, and
  complete app-owned local removal;
- no account, telemetry, cloud service, subscription, or AI dependency.

## Privacy Boundary

MindCanary must not store or transmit URLs, page titles, page text, search
terms, message contents, screenshots, keystrokes, clipboard contents, raw
browsing history, window titles, document names, or application names.

All current product records remain on the host unless the user explicitly
creates and moves an export or backup. See the [privacy policy](docs/privacy-policy.md),
[data boundary](docs/data-and-privacy.md), and [threat model](docs/threat-model.md).

## Alpha Installation

The packaged alpha currently targets Pop!_OS, Ubuntu, and similar Debian-based
Linux systems. Download the `.deb` and its checksum from the latest GitHub
prerelease, then follow the [Linux alpha install guide](docs/alpha-install.md).

The Chrome extension is optional and its ordinary-user store installation is
not part of this alpha.

## Development

The workspace uses Rust, pnpm, React, and Tauri 2. Start with the
[development guide](docs/development.md) and use an isolated
[development profile](docs/development-profiles.md) for onboarding or
destructive tests.

```bash
pnpm install
pnpm check
pnpm --filter @mindcanary/desktop tauri dev
```

## Project Documents

- [Architecture](docs/architecture.md)
- [Data and privacy](docs/data-and-privacy.md)
- [Threat model](docs/threat-model.md)
- [Known limitations](docs/known-limitations.md)
- [Insight evaluation](docs/insight-evaluation.md)
- [Public roadmap](docs/roadmap.md)
- [Security policy](SECURITY.md)

## Safety Boundary

MindCanary is not a medical, diagnostic, prediction, emergency, or treatment
product. It describes local records and leaves their meaning to the user.

## License

MindCanary is licensed under the GNU Affero General Public License v3.0 or
later. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
