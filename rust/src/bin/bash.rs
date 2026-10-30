#![windows_subsystem = "windows"]
//! Local launcher binary. The installer exe contains this same entry and
//! runs it when the file is named BASH.exe.

fn main() {
    bash_core::bootstrap::run();
}
