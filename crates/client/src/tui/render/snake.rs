use gc_shared::game::snake::{ARENA_H, ARENA_W, SnakeState};
use gc_shared::types::{GameOutcome, PlayerId};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::{App, ClientGameState};

// Cells are 2 chars wide × 1 char tall — keeps the 32×18 arena roughly
// square on standard terminals (64 cols × 18 rows).
const CELL_W: u16 = 2;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let arena_width = ARENA_W * CELL_W + 2;
    let arena_height = ARENA_H + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // header / score
            Constraint::Min(arena_height), // arena
            Constraint::Length(2),         // footer
        ])
        .split(frame.area());

    let Some(ClientGameState::Snake(state)) = app.game_state.as_ref() else {
        return;
    };

    // Header — score line.
    let (you_score, opp_score) = score_tuple(state, app.my_player_id);
    let header_text = if let Some(ref outcome) = app.game_over {
        match outcome {
            GameOutcome::Win(winner) => {
                if Some(*winner) == app.my_player_id {
                    t.get("game.you_win").to_string()
                } else {
                    t.get("game.you_lose").to_string()
                }
            }
            GameOutcome::Draw => t.get("game.draw").to_string(),
        }
    } else {
        format!("You: {you_score}  |  Opp: {opp_score}")
    };
    let header_color = if app.game_over.is_some() {
        Color::Yellow
    } else {
        Color::Green
    };
    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // Arena — fixed size, centered within available space.
    let arena_area = fixed_arena_rect(chunks[1], arena_width, arena_height);
    render_arena(frame, state, app, arena_area);

    // Footer.
    let footer_text = if app.rematch_pending {
        format!("Waiting for opponent... | Esc: {}", t.get("game.leave"))
    } else if app.rematch_incoming {
        format!(
            "Y: Accept rematch | N: Decline | Esc: {}",
            t.get("game.leave")
        )
    } else if app.game_over.is_some() {
        format!(
            "R: {} | Esc: {} | I: Help",
            t.get("game.rematch"),
            t.get("game.leave")
        )
    } else {
        t.get("snake.controls").to_string()
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);

    if app.game_over.is_some() {
        render_game_over_banner(frame, t.get("snake.game_over"), chunks[1]);
    }

    if app.show_help {
        render_help_modal(frame);
    }

    if app.rematch_pending || app.rematch_incoming {
        super::render_rematch_overlay(frame, app);
    }
}

fn score_tuple(state: &SnakeState, me: Option<PlayerId>) -> (u32, u32) {
    let mut you = 0u32;
    let mut opp = 0u32;
    for snake in &state.snakes {
        if Some(snake.player_id) == me {
            you = snake.score;
        } else {
            opp = snake.score;
        }
    }
    (you, opp)
}

fn fixed_arena_rect(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn render_arena(frame: &mut Frame, state: &SnakeState, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Snake ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build an index: position → (player-color-kind, is_head).
    let mut heads: std::collections::HashMap<(u16, u16), Color> = std::collections::HashMap::new();
    let mut bodies: std::collections::HashMap<(u16, u16), Color> = std::collections::HashMap::new();
    for snake in &state.snakes {
        let color = if Some(snake.player_id) == app.my_player_id {
            Color::Green
        } else {
            Color::Cyan
        };
        for (i, pos) in snake.body.iter().enumerate() {
            if i == 0 {
                heads.insert((pos.x, pos.y), color);
            } else {
                bodies.insert((pos.x, pos.y), color);
            }
        }
    }
    let food: std::collections::HashSet<(u16, u16)> =
        state.food.iter().map(|p| (p.x, p.y)).collect();

    let mut lines: Vec<Line> = Vec::with_capacity(ARENA_H as usize);
    for y in 0..ARENA_H {
        let mut spans: Vec<Span> = Vec::with_capacity(ARENA_W as usize);
        for x in 0..ARENA_W {
            let key = (x, y);
            if let Some(color) = heads.get(&key) {
                spans.push(Span::styled(
                    "██",
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ));
            } else if let Some(color) = bodies.get(&key) {
                spans.push(Span::styled("▓▓", Style::default().fg(*color)));
            } else if food.contains(&key) {
                spans.push(Span::styled("●●", Style::default().fg(Color::Red)));
            } else {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }

    let arena = Paragraph::new(lines);
    frame.render_widget(arena, inner);
}

fn render_game_over_banner(frame: &mut Frame, label: &str, arena_area: Rect) {
    let text = format!("{label} — Press R to rematch, Esc to leave");
    let width = (text.chars().count() as u16 + 4).min(arena_area.width);
    let x = arena_area.x + arena_area.width.saturating_sub(width) / 2;
    let y = arena_area.y + arena_area.height / 2;
    let rect = Rect::new(x, y, width, 3);
    frame.render_widget(Clear, rect);
    let banner = Paragraph::new(text).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    frame.render_widget(banner, rect);
}

fn render_help_modal(frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        "Goal: Eat food to grow. Last snake alive wins.",
        "Don't hit walls or any snake body (including your own).",
        "",
        "Controls:",
        "- Arrow keys or W/A/S/D: change direction",
        "- I: Close help",
        "- Esc: Leave game",
    ];
    let help_text: Vec<Line> = lines.into_iter().map(Line::from).collect();

    let block = Block::default()
        .title("Snake Rules")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ClientDatabase;
    use crate::tui::app::App;
    use gc_shared::game::snake::SnakeEngine;
    use gc_shared::game::traits::RealtimeGameEngine;
    use gc_shared::i18n::Language;
    use gc_shared::types::{GameSettings, PlayerId};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn snake_renderer_does_not_panic_empty_state() {
        let db = ClientDatabase::open_in_memory().unwrap();
        let mut app = App::new(Language::English, db, "wss://localhost:8443".to_string());
        let p0 = PlayerId::new();
        let p1 = PlayerId::new();
        app.my_player_id = Some(p0);
        let state = SnakeEngine::initial_state(&[p0, p1], &GameSettings::default());
        app.game_state = Some(ClientGameState::Snake(state));

        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }
}
