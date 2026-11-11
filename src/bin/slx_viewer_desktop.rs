#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
#[path = "../desktop/mod.rs"]
mod desktop;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    if let Err(error) = desktop::run() {
        rfd::MessageDialog::new()
            .set_title("SLX Viewer")
            .set_level(rfd::MessageLevel::Error)
            .set_description(format!("Could not start SLX Viewer.\n\n{error}"))
            .show();
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("The SLX Viewer desktop app currently targets Windows.");
    std::process::ExitCode::FAILURE
}
