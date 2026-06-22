# MindCanary Development Guide

Date: 2026-06-19

## Current state

Implemented:

- Cargo and pnpm workspaces
- Rust-owned versioned collector protocol
- generated TypeScript protocol types
- synthetic browser fixture generator
- local Unix-socket daemon
- SQLCipher-backed aggregate storage
- typed local check-in storage
- OS-keyring database-key provider
- Chrome native-message framing and forwarding
- Manifest V3 extension build
- extension popup with native-host, enabled-signal, queue, and delivery status
- privacy-preserving 15-minute tab-rhythm reducer
- CI checks for Rust, TypeScript, generated files, and fixtures
- deterministic local analytics prototype
- typed daemon read RPC for daily rhythm insights
- per-dimension baseline readiness and explicit abstention reasons
- typed daemon read RPC for a bounded daily timeline with explicit gaps
- reusable Rust daemon client with socket round-trip tests
- `mindcanaryctl` local control CLI for daemon health, settings, signal
  enable/disable, and record summaries
- desktop dashboard view models for insight cards, timeline charts, exact daily
  rows, gaps, and empty states
- OS aggregate signal IDs, consent settings, daily storage projection, timeline
  lane, export file, and deterministic insight read-model support
- Linux GNOME/X11 OS active/idle duration adapter using GNOME Mutter idle time,
  with consent-gated 15-minute `os.active_seconds` and `os.idle_seconds`
  aggregates
- consent-gated lock/unlock and suspend/resume count adapters using GNOME
  ScreenSaver and logind event streams
- two-step canonical local-record clearing with expiring confirmation tokens
- default-off, timestamped per-signal browser collection settings
- historical consent enforcement that blocks paused-period retry backfill
- two-step deletion of one aggregate signal's stored metric history without
  removing unrelated signals or check-ins
- fail-closed extension filtering using read-only daemon settings
- Tauri 2 and React desktop shell with a narrow typed daemon command bridge
- rendered optional check-in, daily history, insight, platform capability,
  collection, and record-clearing flows
- desktop getting-started checklist that distinguishes daemon readiness,
  browser signal consent, and first local record delivery
- first-run desktop onboarding that presents the local-first boundary and lets
  users start with check-ins or optional sources before showing the full app
- skippable onboarding choices for Chrome context and computer active/idle
  context, using starter sets rather than fine-grained signal configuration
- tabbed desktop sections for Today, History, Sources, and Data so setup and
  destructive controls do not crowd the daily check-in surface
- non-overlapping local dashboard refresh, with a manual control and
  one-minute polling only while the desktop window is visible
- confirmed local export to a human-readable report plus daily aggregate CSVs
- user-owned day and time-window annotations in encrypted storage, history,
  and export, deliberately excluded from automated analysis
- immediate latest-record context and explicit baseline-building progress
- user-initiated versioned SQLCipher backup, one-time recovery secret,
  verification, and empty-profile restoration
- offline local profile destruction for the encrypted database and keyring key
- user-level Linux systemd service installation and removal for `mindcanaryd`
- user-level Linux Chrome/Chromium native-host manifest installation and
  removal
- deterministic Chrome development identity plus shared development/release
  identity configuration for extension and native-host builds
- Firefox development build and native-host identity/manifest support, using a
  Firefox-compatible background script while preserving the same protocol
- desktop Chrome connector status and packaged native-host manifest repair flow
  that does not accept pasted extension IDs or arbitrary runtime paths
- count-only tab-retention carryover observer that survives extension restarts
  better than a single midnight bucket
- surfaced extension retry-queue overflow through a local dropped-batch count
  in the popup status
- descriptive insight hardening for pooled per-signal baselines, two-day
  sustained-change descriptions, isolated-spike abstention, exact prior-date
  evidence, and a
  visible "waiting for sustained change" readiness state
- descriptive insight abstention when a comparable baseline is too variable
- versioned local-v1 alpha insight thresholds with a zero-false-nudge synthetic
  fixture budget
- separately consented continuous-scrolling duration on the fixed `x.com` and
  `twitter.com` allowlist, with optional host permission and aggregate-only
  timeline/export output

Not yet implemented:

- signed ordinary-user Linux release artifacts
- unlisted Chrome Web Store beta/release identity and store installation flow
- disabled or removed extension detection in desktop connection status
- real-user/design-participant threshold validation and longer-horizon
  false-nudge evaluation
- week-long collector soak and overhead testing
- complete uninstall cleanup of extension queue, application configuration, and
  user-created exports
- Firefox temporary-install/native-messaging runtime validation
- foreground category and application-switch adapters [post-v1]
- broader website interaction adapters [post-v1 and user-research gated]
- local AI adapter
- synchronization

The daemon now writes aggregate batches and typed check-ins to a SQLCipher
database. The production bootstrap path stores the database key through the
operating-system keyring and fails closed if an existing encrypted database no
longer has its key.

## Toolchain

Pinned:

- Rust 1.86
- Node.js 22 or later
- pnpm 10.8.1

Install JavaScript dependencies:

```bash
pnpm install --frozen-lockfile
```

Run the complete check:

```bash
pnpm check
```

Individual checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p mindcanary-storage --test os_keyring -- --ignored --nocapture
pnpm protocol:check
pnpm fixtures:check
pnpm format:check
pnpm lint
pnpm test
pnpm build
```

## Run the local daemon

```bash
cargo run -p mindcanaryd
```

The default socket is:

```text
$XDG_RUNTIME_DIR/mindcanary/mindcanaryd.sock
```

The default database is:

```text
$XDG_DATA_HOME/mindcanary/mindcanary.db
```

If `XDG_DATA_HOME` is unavailable, the daemon follows the usual user data
fallback under `$HOME/.local/share/mindcanary`.

If `XDG_RUNTIME_DIR` is unavailable, the daemon uses the operating-system
temporary directory and creates its own mode-`0700` subdirectory. The socket is
mode `0600`.

## Protect dogfood data during development

For the short command-only runbook, see
[`development-profiles.md`](development-profiles.md).

The default XDG profile is live dogfood data once a developer is using
MindCanary personally. Do not use the default profile for clean-state
onboarding tests, destructive checks, or migration experiments.

Create a human-readable local export before sensitive work:

```bash
pnpm backup:local
```

By default this writes a timestamped export under
`$HOME/Documents/mindcanary-backups/`. Set `MINDCANARY_BACKUP_DIR` to choose a
different parent directory. The export contains readable daily aggregates and
check-ins, so keep it private.

Run clean-state development with an isolated profile:

```bash
scripts/with-dev-profile.sh onboarding -- cargo run -p mindcanaryd
```

In another terminal, use the same profile name for the desktop shell:

```bash
scripts/with-dev-profile.sh onboarding -- pnpm --filter @mindcanary/desktop tauri dev
```

The wrapper sets isolated `XDG_RUNTIME_DIR`, `XDG_DATA_HOME`, and
`XDG_CONFIG_HOME` directories under `target/dev-profiles/<profile>/`. Commands
that use the default socket will then talk to that profile:

```bash
scripts/with-dev-profile.sh onboarding -- \
  cargo run -q -p mindcanary-client --bin mindcanaryctl -- summary
```

This profile isolation is for daemon, desktop, check-in, and onboarding work.
The existing Chrome extension and native-host registration should be treated as
the dogfood connector path unless Chrome is also launched with the same
isolated config environment and the native-host manifest is reinstalled there.

Override only the daemon socket location for narrower development:

```bash
cargo run -p mindcanaryd -- --socket /tmp/mindcanary-dev/mindcanaryd.sock
```

Install the user-level Linux `systemd` service after building the daemon:

```bash
cargo build -p mindcanaryd
cargo run -p mindcanaryd -- \
  --install-user-service \
  --daemon-path "$PWD/target/debug/mindcanaryd"
```

The installer writes `mindcanaryd.service` under
`$XDG_CONFIG_HOME/systemd/user` when set and otherwise under
`$HOME/.config/systemd/user`. It does not enable or start the service unless
you pass `--enable-now`:

```bash
cargo run -p mindcanaryd -- \
  --install-user-service \
  --daemon-path "$PWD/target/debug/mindcanaryd" \
  --enable-now
```

Remove the installed user service:

```bash
cargo run -p mindcanaryd -- --uninstall-user-service
```

Pass `--disable-now` to stop and disable it with `systemctl --user` before
removing the unit file.

Destroy the local encrypted database profile after stopping the daemon:

```bash
systemctl --user stop mindcanaryd.service
cargo run -p mindcanaryd -- \
  --destroy-local-profile \
  --confirm-destroy-local-profile
```

This removes the default encrypted database, SQLite sidecar files, and the
OS-keyring database key. Pass `--database <path>` to target a development
database. It does not remove Chrome extension storage, native-host manifests,
application configuration, or exports saved elsewhere.

For a complete app-owned local removal path, use the wrapper script:

```bash
pnpm uninstall:local -- --confirm-local-removal
```

The wrapper removes the user service, Chrome and Chromium native-host
manifests, the default encrypted database profile, SQLite sidecars, the
OS-keyring database key, the package setup marker, and the runtime socket
directory. It still does not remove Chrome-owned extension storage or
user-created exports/backups. Preview the commands without removing anything:

```bash
pnpm uninstall:local -- --dry-run
```

The desktop Local Data panel exposes the same app-owned removal boundary behind
an exact confirmation phrase. Exercise the destructive path only in a
disposable test user or VM because the OS-keyring database key is shared by the
default local profile.

For a repeatable destructive audit without touching the dogfood profile, run
the private-session audit:

```bash
pnpm removal:audit
```

The audit creates a private `HOME`, XDG config/data/runtime directories, DBus
session, and GNOME keyring daemon under `target/removal-audit/`, seeds a tiny
profile, runs the real complete local-removal command, and checks that the
app-owned files and private keyring entry are gone.

## Generated artifacts

Rust is the source of truth for the wire protocol.

Regenerate TypeScript:

```bash
pnpm protocol:generate
```

Regenerate synthetic data:

```bash
pnpm fixtures:generate
```

CI fails if either committed artifact differs from its generator.

## Build the extension

```bash
pnpm --filter @mindcanary/extension build
```

The unpacked extension is written to:

```text
apps/extension/dist
```

Development builds have a deterministic Chrome identity:

```text
agokdhalkipifklmbipkgmfakdcaekbj
```

The development public key and expected ID live together in
`config/chrome-extension-identities.json`. The build verifies that the key
derives that ID before writing `dist/manifest.json`. Do not add a private key
to the repository.

Release builds use a separate explicit identity:

```bash
pnpm --filter @mindcanary/extension build:release
```

That command intentionally fails until the unlisted Chrome Web Store listing
provides a release ID in `config/chrome-extension-identities.json`. Release
builds never reuse the development manifest key.

The extension requests only:

- `alarms`
- `nativeMessaging`
- `storage`

The `idle` and `scripting` permissions are optional. The only optional host
permissions are `https://x.com/*` and `https://twitter.com/*`, used by the
separately enabled continuous-scrolling adapter. There is no history permission
or incognito access. The adapter is not part of the starter set and does not
read routes or page content.

Build the Firefox development package separately:

```bash
pnpm --filter @mindcanary/extension build:firefox
```

It is written to `apps/extension/dist-firefox` with development add-on ID
`development@mindcanary.local`. Firefox Manifest V3 uses a background script
rather than Chrome's service-worker manifest entry. Release builds fail closed
until a distinct Firefox release ID is configured in
`config/firefox-extension-identities.json`.

The native host name is currently:

```text
app.mindcanary.collector
```

To try the current collector locally:

1. Start the daemon in one terminal:

   ```bash
   cargo run -p mindcanaryd
   ```

2. Build the extension:

   ```bash
   pnpm --filter @mindcanary/extension build
   ```

3. Open `chrome://extensions`, enable Developer mode, choose "Load unpacked",
   and select `apps/extension/dist`.

4. Confirm that Chrome shows extension ID
   `agokdhalkipifklmbipkgmfakdcaekbj`. The popup also checks its runtime ID
   against the selected build identity and shows an explicit mismatch instead
   of offering a setup command for the wrong extension.

5. Install the native-host manifest using the command shown in the popup, or
   run the equivalent manually:

   ```bash
   cargo build -p mindcanary-native-host
   cargo run -p mindcanary-native-host -- \
     --install-manifest \
     --browser chrome \
     --channel development \
     --host-path "$PWD/target/debug/mindcanary-native-host"
   ```

6. Reload the unpacked extension in `chrome://extensions`.

7. Open the MindCanary extension popup and press "Refresh status". The popup
   should tell you whether the native host is connected, show the extension ID
   and native-host name, report whether any signals are enabled, and show
   whether aggregate batches are queued. It also reports whether Chrome's
   optional idle permission is granted. Once collection starts, it shows coarse
   progress toward the current fixed 15-minute bucket boundary. Refreshing also
   closes and delivers any bucket whose boundary has already passed.

8. Enable the default browser aggregate set. In the desktop app, open
   "Browser signals" and choose "Enable starter set". The same development
   action is available through the local control CLI:

   ```bash
   cargo run -p mindcanary-client --bin mindcanaryctl -- \
     enable-browser-defaults
   cargo run -p mindcanary-client --bin mindcanaryctl -- settings
   ```

   Until a signal is enabled, the extension intentionally reports "Collection
   is paused" and does not persist browser aggregates.

   The default browser set includes tab switching, maximum and average open
   tabs, browser-window maximum, active/idle duration, and tabs retained across
   a local-day boundary. If the popup shows that idle permission is needed,
   click "Allow idle permission" in the popup and approve Chrome's permission
   prompt.

The extension popup is operational status only. It does not show URLs, titles,
page text, search terms, or browsing history.

### Firefox development smoke

Build the Firefox package and native host:

```bash
pnpm --filter @mindcanary/extension build:firefox
cargo build -p mindcanary-native-host
cargo run -p mindcanary-native-host -- \
  --install-manifest \
  --browser firefox \
  --channel development \
  --host-path "$PWD/target/debug/mindcanary-native-host"
```

Open `about:debugging#/runtime/this-firefox`, choose "Load Temporary Add-on",
and select `apps/extension/dist-firefox/manifest.json`. Temporary installation
and the native-message round trip require a running Firefox session and remain
a manual validation. Use only one browser connector at a time for now: browser
aggregate records do not yet carry connector provenance, so overlapping Chrome
and Firefox collection would double-count shared time.

Useful local control commands while the daemon is running:

```bash
cargo run -p mindcanary-client --bin mindcanaryctl -- health
cargo run -p mindcanary-client --bin mindcanaryctl -- source-status
cargo run -p mindcanary-client --bin mindcanaryctl -- signals
cargo run -p mindcanary-client --bin mindcanaryctl -- settings
cargo run -p mindcanary-client --bin mindcanaryctl -- summary
cargo run -p mindcanary-client --bin mindcanaryctl -- \
  export --directory "$HOME/Documents/mindcanary-export"
cargo run -p mindcanary-client --bin mindcanaryctl -- enable-browser-defaults
cargo run -p mindcanary-client --bin mindcanaryctl -- \
  enable browser.open_tab_count_mean
cargo run -p mindcanary-client --bin mindcanaryctl -- disable-browser-defaults
```

Capture a stable Chrome connection smoke snapshot during manual testing:

```bash
pnpm smoke:chrome
pnpm smoke:chrome restart-daemon
pnpm smoke:chrome after-manual-step
```

The report records daemon health, source status, local summary counts, the
development native-host manifest status, and the user-level daemon service
state under `target/stable-chrome-smoke/`. Use it before and after manual
extension reload, disable, remove, or re-enable checks.

Install the user-level Linux Chrome manifest after building the native host:

```bash
cargo build -p mindcanary-native-host
cargo run -p mindcanary-native-host -- \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path "$PWD/target/debug/mindcanary-native-host"
```

Use `--browser chromium` for Chromium. The installer writes
`app.mindcanary.collector.json` under the browser's `NativeMessagingHosts`
directory, using `XDG_CONFIG_HOME` when set and otherwise `$HOME/.config`.
The manifest allowlists only the extension ID configured for the selected
channel. The installer refuses `--channel release` until a distinct Web Store
ID is configured. It does not accept arbitrary extension IDs. The daemon still
has to be running separately.

Use `--browser firefox` for Firefox. On Linux, that manifest is written under
`$HOME/.mozilla/native-messaging-hosts` and uses Firefox's configured add-on ID
rather than a Chrome extension origin.

Remove the user-level browser manifest:

```bash
cargo run -p mindcanary-native-host -- \
  --uninstall-manifest \
  --browser chrome
```

Use `--browser chromium` or `--browser firefox` to remove the corresponding
registration. This removes only the native-host manifest file, not the browser
extension or its local storage.

## Run the desktop shell

The React frontend can be checked independently:

```bash
pnpm --filter @mindcanary/desktop test
pnpm --filter @mindcanary/desktop build
```

After installing the Tauri prerequisites below, check and run the native shell:

```bash
pnpm --filter @mindcanary/desktop tauri:check
pnpm --filter @mindcanary/desktop tauri dev
```

Build the first ordinary-user Linux package:

```bash
pnpm package:linux
```

The packaging command builds release versions of `mindcanaryd` and
`mindcanary-native-host`, includes them under `/usr/lib/mindcanary`, and
produces a Debian package under the Tauri bundle output directory. When the
packaged desktop cannot reach the daemon, it installs and enables the
user-level service and then retries. Development builds do not contain those
packaged helpers and continue to use the manually started development daemon.

The webview has a narrow command surface for daemon health, source status,
insights, daily history, collection settings, typed check-in submission, local
record counts, per-signal record deletion, and the two-step clear flow. It
cannot submit arbitrary protocol requests and has no SQL, shell, filesystem,
or public-network capability.

`source-status` reports only source type, operational health, and the most
recent local receipt timestamp. Browser and OS aggregate collectors become
stale after 45 minutes without a stored batch. Check-ins remain ready because
they have no required cadence.

The daily history read model returns the most recent 30 calendar days by
default, bounded to the first and last recorded local dates. Days without a
browser aggregate, OS aggregate, or check-in are returned explicitly rather
than represented as zero.

On Linux GNOME/X11, the daemon can collect OS active and idle duration if the
two OS activity signals are enabled. The desktop app exposes them in
"Computer activity signals". The same development action is available through
the local control CLI:

```bash
cargo run -p mindcanary-client --bin mindcanaryctl -- enable os.active_seconds
cargo run -p mindcanary-client --bin mindcanaryctl -- enable os.idle_seconds
cargo run -p mindcanary-client --bin mindcanaryctl -- settings
```

The adapter samples GNOME Mutter's idle monitor and stores only aligned
15-minute active/idle totals after daemon consent checks. It does not collect
window titles, document names, foreground applications, lock events, suspend
events, or resume events.

The baseline prototype compares the latest local day with all prior recorded
days containing that dimension. It does not infer workdays or weekends. It
requires three prior values before change language can appear. The insight
response also reports one deterministic readiness state per dimension:

- change described
- within the current baseline threshold
- no latest value
- insufficient prior comparable days
- prior comparable values centered on zero

The desktop keeps these reasons behind "How each dimension was handled." A
fresh profile therefore explains that it is building a baseline instead of
showing an unexplained empty insight panel. Emitted descriptions also include
the number and exact dates of prior baseline days in their evidence.
Cumulative signals are coverage-normalized for descriptions: tab switching is
measured per recorded hour, and browser/computer active time is measured as a
share of recorded periods. History continues to show raw daily totals.

## Storage notes

`rusqlite` is pinned to `0.39.x` because the newer `libsqlite3-sys 0.38.x`
build script requires a macro unavailable on the pinned Rust 1.86 toolchain.
The chosen release still provides `bundled-sqlcipher-vendored-openssl`, so the
project does not depend on a system SQLCipher installation.

The regular workspace test suite uses temporary fixed keys. The OS-keyring test
is ignored by default because it creates and deletes a real desktop keyring
entry:

```bash
cargo test -p mindcanary-storage --test os_keyring -- --ignored --nocapture
```

The wrong-key test may cause SQLCipher to write an HMAC failure line to stderr.
That is expected and confirms that a mismatched key cannot read page 1.

## Check-in protocol

The first check-in record supports bounded numeric fields and enum context tags:

- sleep minutes
- perceived sleep need
- mood
- energy
- irritability
- concentration
- impulsivity
- medication taken
- substance use
- context tags such as deadline, travel, illness, news cycle, or exercise

There is deliberately no free-text note, diagnosis, clinical phase, URL, title,
or generic metadata map in the wire protocol.

## Tauri prerequisites

The current Pop!_OS 22.04 development machine has the native Tauri development
libraries installed and has completed a development startup smoke test. A new
Ubuntu or Pop!_OS development machine should install:

```bash
sudo apt-get install -y \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Official reference: https://v2.tauri.app/start/prerequisites/

Do not replace the SQLCipher store with plaintext SQLite merely to make a UI
demo run.

## Privacy checks

Collector records are closed typed objects. Protocol tests reject unknown
fields and unknown signal IDs, including attempted `url` fields.

Before adding a signal:

1. add a typed Rust `SignalId`
2. define its validation rule
3. regenerate TypeScript
4. add synthetic data
5. prove the source adapter discards transient context
6. update consent and retention documentation

Never add a generic metadata map to the collector protocol.
