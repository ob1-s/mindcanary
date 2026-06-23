# Development Profiles

The default profile may contain live personal data. Do not use it for
onboarding, destructive tests, or migrations. Back it up before sensitive work:

```bash
pnpm backup:local
```

## Personal profile

Use the packaged app and its user service:

```bash
systemctl --user enable --now mindcanaryd.service
systemctl --user status mindcanaryd.service
```

Then open **MindCanary** from the app launcher. Development commands without a
profile name use this same default data.

## Isolated development profile

Choose a profile name and use it in both terminals:

```bash
# Terminal 1: daemon
scripts/with-dev-profile.sh onboarding -- cargo run -q -p mindcanaryd

# Terminal 2: desktop
scripts/with-dev-profile.sh onboarding -- \
  pnpm --filter @mindcanary/desktop tauri dev
```

The profile persists under `target/dev-profiles/<name>/`. Use a new name for a
fresh profile, or stop both processes and reset an existing one:

```bash
rm -rf -- target/dev-profiles/onboarding
```

Inspect a profile from another terminal:

```bash
scripts/with-dev-profile.sh onboarding -- \
  cargo run -q -p mindcanary-client --bin mindcanaryctl -- summary
```

Chrome remains connected to the personal profile unless Chrome and its native
host are also configured inside the isolated environment.
Login startup is managed only by the packaged Linux app; development profiles
show that setting as unavailable.

## Update the installed personal app

Close MindCanary first. Rebuild the `.deb` before reinstalling it; reinstalling
an existing artifact does not include newer source changes:

```bash
pnpm backup:local
pnpm package:linux

sudo apt install --reinstall \
  ./apps/desktop/src-tauri/target/release/bundle/deb/MindCanary_0.1.3_amd64.deb

systemctl --user daemon-reload
systemctl --user restart mindcanaryd.service
```

Reopen MindCanary only after the daemon restart. Reinstalling the package
preserves personal records; the backup is precautionary.
