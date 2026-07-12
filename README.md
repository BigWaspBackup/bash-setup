# B.A.S.H Setup

Windows installer package for Simba with **SRL-B**, **BashLib**, and the **B.A.S.H Launcher**  
(**BigAussie Script House**).

**Discord:** https://discord.gg/qsmKs5uKfR

### Fresh install (recommended)

1. Build or download `bash-setup.exe` (see below).
2. Run it and click through Next → Install → Finish.
3. Open Simba from the desktop shortcut and press play.

### Repair / force update

1. Run `tools\BASH_Force_Update_Tool.bat` **as Administrator**.
2. It backs up Simba + RuneLite, reinstalls both, then restores `credentials.simba`, `Configs\`, and `Includes\WaspLib\overrides.simba`.

## Maintainers — build the GUI installer

Requires [Rust](https://rustup.rs/) on Windows.

```cmd
cd rust
cargo build --release
```

Output:

```text
rust\target\release\bash-setup.exe
dist\bash-setup.exe 
```