# MindCanary Linux Alpha Install Guide

Status: early limited Linux alpha.

MindCanary is a private local journal for noticing how your routines change
over time. It is not a medical, diagnostic, emergency, prediction, or treatment
product.

The current alpha is for Pop!_OS, Ubuntu, and similar Debian-based Linux
systems. Chrome is optional. You can use MindCanary with check-ins only.

## Before You Install

MindCanary currently:

- stores data locally on your computer;
- does not require an account;
- does not require a subscription;
- does not send product telemetry;
- does not include hosted sync or AI;
- does not store URLs, page titles, page text, search terms, screenshots,
  keystrokes, or raw browsing history.

Please do not send personal exports, database files, or screenshots with
private values when asking for help.

## Install

1. Download the `.deb` file from the GitHub Release.
2. Open the folder where you downloaded it.
3. Double-click the `.deb` file and install it with your system software
   installer.
4. Open **MindCanary** from your app launcher.
5. Add one check-in.

If double-click install does not work, open a terminal in the download folder
and run:

```bash
sudo apt install ./MindCanary_0.1.2_amd64.deb
```

## Update An Existing Install

Close MindCanary, then install the newer `.deb` the same way as the first one.
Your local profile is kept across package updates. Creating an encrypted backup
first is recommended while the app is in alpha.

From a terminal in the download folder:

```bash
sudo apt install ./MindCanary_0.1.2_amd64.deb
```

Reopen MindCanary and confirm your prior History is present. Report an update
that does not preserve it before doing any cleanup or reinstall experiments.

## First Minute

After opening MindCanary:

- the local daemon should become **Running**;
- check-ins should be **Ready**;
- Chrome may say **Not connected** or **Disabled**. That is okay for this
  alpha.

The useful first test is simple:

1. Add a check-in.
2. Close MindCanary.
3. Open MindCanary again.
4. Confirm the check-in still appears in daily history.

## Optional Chrome Connector

The Chrome connector is optional and still has a rougher setup path because the
Chrome Web Store listing is not ready yet.

For this alpha, treat Chrome data as optional. Use the app with check-ins only
unless the release instructions explicitly include a browser build.

When the connector is enabled, it records low-detail aggregates such as tab
switches and open-tab counts in 15-minute periods. It does not store URLs,
titles, page text, search terms, or browsing history.

## Export Or Remove Your Data

Inside MindCanary, use the local data controls to:

- export local records to a folder you choose;
- clear local records;
- review app-owned local removal.

Exports are readable files. Keep them private.

Removing the Linux package does not automatically remove your local records.
If you want to remove local MindCanary data too, use the app's local removal
flow first, then uninstall the package.

To uninstall the package:

```bash
sudo apt remove mind-canary
```

## What Feedback Helps

Useful alpha feedback:

- Did installation work?
- Did the first check-in make sense?
- Did anything feel scary, clinical, guilt-inducing, or confusing?
- Did you understand that Chrome is optional?
- Did the app feel useful before it had several days of history?
- What operating system and version are you using?

Send non-sensitive feedback through this repository's GitHub Issues.

The bundled `FEEDBACK.md` has a short report template that avoids collecting
private MindCanary records. The Data page can also preview non-sensitive support
information; review it before choosing to copy and send it.

Do not send personal exports, databases, browser extension storage, or
screenshots containing private records unless you intentionally remove private
values first.

## Known Limitations

- Linux only for now.
- Chrome connector setup is not ready for ordinary alpha users.
- Pattern explanations need enough comparable local history.
- Missing days are shown as missing, not treated as zero.
- Local encryption cannot protect against malware running as the same unlocked
  OS user.
- Support is best-effort during the early alpha.
