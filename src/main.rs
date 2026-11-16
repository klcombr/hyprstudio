//! HyprStudio: a configuration studio for Hyprland.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> gtk4::glib::ExitCode {
    // GTK and Hyprland both use CLI arguments; HyprStudio only understands one.
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config-dir" | "-c" => {
                if let Some(dir) = args.next() {
                    // SAFETY: set before any other thread is started.
                    unsafe { std::env::set_var("HYPRSTUDIO_CONFIG_DIR", dir) };
                }
            }
            "--help" | "-h" => {
                println!(
                    "HyprStudio {}\n\nUsage: hyprstudio [--config-dir <path>]\n\n  --config-dir  Inspect and manage a different Hyprland config directory\n  --help        Show this message\n\nEnvironment:\n  HYPRSTUDIO_CONFIG_DIR       Same as --config-dir\n  HYPRSTUDIO_SKIP_HYPRLAND_VERIFY  Skip `hyprland --verify-config` (tests only)\n  RUST_LOG                    tracing filter, e.g. hyprstudio=debug",
                    env!("CARGO_PKG_VERSION")
                );
                return gtk4::glib::ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!("HyprStudio {}", env!("CARGO_PKG_VERSION"));
                return gtk4::glib::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("hyprstudio=info")),
        )
        .with_target(false)
        .init();

    hyprstudio::app::run()
}
