pub mod app;
pub mod event;
pub mod screens;

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, LoginField, Screen};
use event::{Event, EventHandler};

/// Run the TUI application loop.
pub fn run(mut app: App) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let events = EventHandler::new(50); // 50ms tick rate

    // Main loop
    while app.running {
        // Draw
        terminal.draw(|frame| match app.screen {
            Screen::Login => screens::login::render(frame, &app),
            Screen::Lobby => screens::lobby::render(frame, &app),
        })?;

        // Handle events
        if let Ok(event) = events.recv() {
            match event {
                Event::Key(key) => handle_key(&mut app, key.code, key.modifiers),
                Event::Tick => {} // Could update animations, etc.
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    // Global quit
    if code == KeyCode::Esc || (code == KeyCode::Char('c') && modifiers == KeyModifiers::CONTROL) {
        app.quit();
        return;
    }

    match app.screen {
        Screen::Login => handle_login_key(app, code),
        Screen::Lobby => handle_lobby_key(app, code),
    }
}

fn handle_login_key(app: &mut App, code: KeyCode) {
    if app.login_loading {
        return; // Ignore input while loading
    }

    match code {
        KeyCode::Tab => app.toggle_field(),
        KeyCode::F(2) => app.toggle_login_mode(),
        KeyCode::Enter => {
            if let Some(err) = app.validate_login_form() {
                app.login_error = Some(err);
            } else {
                // In Phase 2, we do offline auth simulation.
                // Real networking will be wired in Phase 3+.
                app.login_loading = true;
                // For now, simulate successful auth after form validation
                app.on_auth_success("offline-token".to_string());
            }
        }
        KeyCode::Backspace => match app.active_field {
            LoginField::Username => {
                app.username_input.pop();
            }
            LoginField::Password => {
                app.password_input.pop();
            }
        },
        KeyCode::Char(c) => match app.active_field {
            LoginField::Username => app.username_input.push(c),
            LoginField::Password => app.password_input.push(c),
        },
        _ => {}
    }
}

fn handle_lobby_key(app: &mut App, code: KeyCode) {
    if let KeyCode::Char('q') = code {
        app.quit();
    }
    // Room creation and joining will be implemented in Phase 3
}
