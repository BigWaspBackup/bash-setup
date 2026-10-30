# BASH Launcher 2.0

Windows installer and desktop launcher for **Simba**, **SRL-B**, **BashLib**, and
B.A.S.H scripts (**BigAussie Script House**).

**Discord:** https://discord.gg/qsmKs5uKfR

## What gets installed

```text
%LOCALAPPDATA%\Simba\
  Simba64.exe               Simba (BigWaspBackup/bash-simba release, Torwent fallback)
  BASH.exe                  BASH Launcher 2.0 (desktop shortcut target)
  Scripts\bash-launcher.simba   legacy Simba launcher (kept for compatibility)
  Includes\SRL-B\           latest SRL-B release
  Includes\BashLib\         latest BashLib release
  Data\bash\state.json      installed / latest versions, written by BASH.exe
```

Desktop + Start menu shortcut **B.A.S.H** → `BASH.exe` (the egui launcher).
Users launch and maintain libs/scripts from that window — not the old Simba form.

### Fresh install

1. Download `bash-setup.exe` from the [latest release](https://github.com/BigWaspBackup/bash-setup/releases/latest).
2. Run it: Next → Install → Finish (leave "Launch BASH Launcher 2.0" ticked).
3. The launcher opens. Install the **BASH RuneLite Profile**, then pick a script.

Re-running the installer over an existing install updates it in place and keeps
`credentials.simba`, `Configs\`, your scripts and `Includes\*\overrides.simba`.

### Every start (BASH.exe)

The **B.A.S.H** shortcut runs `BASH.exe`, which:

1. Checks Simba, SRL-B, BashLib and itself against their latest releases.
2. If anything is out of date, asks **Update now / Skip this time** (close RuneLite
   and stop scripts first when libraries are locked).
3. Opens **BASH Launcher 2.0**: script catalog, account/settings, gear, farm/birdhouse,
   then runs scripts with `Simba64.exe --run` (no editor window by default).

Flags: `--update` (re-install libraries), `--no-launch`, `--silent`, `--simba-dir <path>`.
Log: `%TEMP%\bash-bootstrapper.log` (installer: `%TEMP%\bash-setup.log`).

### Repair / force update

1. Run `tools\BASH_Force_Update_Tool.bat` **as Administrator**.
2. It backs up Simba + RuneLite, reinstalls both via `bash-setup.exe --silent`,
   then restores `credentials.simba`, `Configs\`, and `Includes\BashLib\overrides.simba`.

## Maintainers - build

Requires [Rust](https://rustup.rs/) on Windows.

```cmd
cd rust
cargo build --release
```

Output:

```text
rust\target\release\bash-setup.exe   installer (GUI, or --silent)
rust\target\release\BASH.exe         BASH Launcher 2.0
```

Publish **both** files as assets on a `bash-setup` GitHub release tagged `v2.0.0`
(or later). The installer prefers a sibling `BASH.exe` when present, otherwise
copies itself (it runs as the launcher when named `BASH.exe`). Installed copies
self-update from the `BASH.exe` release asset.

Simba is fetched from the latest release of `BigWaspBackup/bash-simba` (private until
public; Torwent/Simba is the fallback), asset `Simba-Win64.exe`.
