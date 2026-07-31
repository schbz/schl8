mod agent;
mod agent_brief;
mod agent_skills;
mod agent_toolkit;
mod app;
mod cli;
mod cli_install;
mod config;
mod crypto;
mod document;
mod hooks;
mod hotkey;
mod keybind;
mod login_item;
mod macos_default_app;
mod macos_open;
mod macos_power;
mod security;
mod tray;
mod ui;
mod update;

use anyhow::{Context, Result};
use clap::Parser;

fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = cli::Cli::parse();

    // 2. Lock down the process (disable core dumps, check mlock limits)
    security::memory::lock_down().context("failed to lock down process security")?;

    // 3. Install panic hook for clean shutdown
    security::cleanup::install_panic_hook();

    // Headless agent commands run and exit before any GUI setup — after
    // lock_down (core dumps stay disabled even for one-shot encrypts),
    // before Apple-event and theme work that only a window needs.
    if let Some(cmd) = cli.command {
        std::process::exit(agent::run(cmd));
    }

    // 3b. Register the Finder "open documents" Apple-event handler BEFORE
    // the event loop starts — the only point early enough to catch the file
    // that cold-launched the app from Finder ("Open With → Schl8").
    macos_open::install_early();

    // 3c. Initialize the theme from config so all UI colors resolve.
    ui::theme::init(&config::Config::load().appearance);

    // 4. Launch the GUI — file selection and decryption happen inside the app
    #[cfg(debug_assertions)]
    let app = {
        let mut app = if cli.sample_archive {
            app::App::new_sample_archive()
        } else if cli.sample {
            app::App::new_sample()
        } else {
            app::App::new(cli.file)
        };
        if cli.jot {
            app.open_jot_on_launch();
        }
        if cli.locked {
            app.start_locked_preview();
        }
        if cli.settings {
            app.open_settings_on_launch();
        }
        if cli.crawl {
            app.start_crawl_on_launch();
        }
        if cli.save_targets {
            app.open_save_targets_on_launch();
        }
        app
    };
    #[cfg(not(debug_assertions))]
    let app = app::App::new(cli.file);

    app.run()?;

    Ok(())
}
