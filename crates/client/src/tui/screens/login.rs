use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{App, LoginField, LoginMode};

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(10),   // form
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    // Title
    let title = Paragraph::new(t.get("app.title"))
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Login form (centered)
    let form_area = centered_rect(50, 60, chunks[1]);
    render_form(frame, app, form_area);

    // Footer
    let mode_hint = match app.login_mode {
        LoginMode::Login => t.get("login.switch_to_register"),
        LoginMode::Register => t.get("login.switch_to_login"),
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::raw("Tab: switch field | F2: "),
        Span::styled(mode_hint, Style::default().fg(Color::Yellow)),
        Span::raw(" | Esc: "),
        Span::styled(t.get("app.quit"), Style::default().fg(Color::Red)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}

fn render_form(frame: &mut Frame, app: &App, area: Rect) {
    let t = &app.translator;

    let title = match app.login_mode {
        LoginMode::Login => t.get("login.title"),
        LoginMode::Register => t.get("login.register_button"),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacing
            Constraint::Length(3), // username
            Constraint::Length(1), // spacing
            Constraint::Length(3), // password
            Constraint::Length(1), // spacing
            Constraint::Length(1), // error
            Constraint::Length(1), // spacing
            Constraint::Length(1), // button
        ])
        .split(inner);

    // Username field
    let username_style = if app.active_field == LoginField::Username {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    let username_block = Block::default()
        .title(t.get("login.username"))
        .borders(Borders::ALL)
        .border_style(username_style);
    let username = Paragraph::new(app.username_input.as_str()).block(username_block);
    frame.render_widget(username, form_chunks[1]);

    // Password field (masked)
    let password_style = if app.active_field == LoginField::Password {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    let password_block = Block::default()
        .title(t.get("login.password"))
        .borders(Borders::ALL)
        .border_style(password_style);
    let masked: String = "*".repeat(app.password_input.len());
    let password = Paragraph::new(masked.as_str()).block(password_block);
    frame.render_widget(password, form_chunks[3]);

    // Error message
    if let Some(ref err) = app.login_error {
        let error = Paragraph::new(err.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(error, form_chunks[5]);
    }

    // Loading or submit hint
    if app.login_loading {
        let loading = Paragraph::new(t.get("login.connecting"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        frame.render_widget(loading, form_chunks[7]);
    } else {
        let button_text = match app.login_mode {
            LoginMode::Login => t.get("login.login_button"),
            LoginMode::Register => t.get("login.register_button"),
        };
        let button = Paragraph::new(Line::from(vec![
            Span::raw("Enter: "),
            Span::styled(
                button_text,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(button, form_chunks[7]);
    }

    // Show cursor in active field
    let (cursor_x, cursor_y) = match app.active_field {
        LoginField::Username => (
            form_chunks[1].x + app.username_input.len() as u16 + 1,
            form_chunks[1].y + 1,
        ),
        LoginField::Password => (
            form_chunks[3].x + app.password_input.len() as u16 + 1,
            form_chunks[3].y + 1,
        ),
    };
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Create a centered rect of given percentage width/height inside `r`.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
