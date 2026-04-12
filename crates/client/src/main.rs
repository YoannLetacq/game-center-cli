mod auth;
mod config;
mod database;
mod net;
mod tui;

use config::ClientConfig;
use database::ClientDatabase;
use gc_shared::i18n::Language;

fn main() {
    // Load config
    let config = ClientConfig::load(&ClientConfig::config_path());

    // Detect language
    let language = if config.language == "auto" {
        Language::detect()
    } else {
        Language::from_code(&config.language)
    };

    // Open local database
    let db_path = ClientConfig::config_dir().join("local.db");
    let db = match ClientDatabase::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            std::process::exit(1);
        }
    };

    // Create app and run TUI
    let app = tui::app::App::new(language, db);

    if let Err(e) = tui::run(app) {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}
