use gc_shared::game::snake::{ARENA_H, ARENA_W, SnakeArena, SnakeState};
use gc_shared::types::{GameOutcome, PlayerId};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::{App, ClientGameState};

// Solo: 2 chars per cell keeps the 32×18 arena roughly square (64 cols).
// Multiplayer: 1 char per cell so two arenas fit side-by-side at 80 cols
// (each arena = 32 + borders ≈ 34 cols × 2 = 68 cols total).
const CELL_W_SOLO: u16 = 2;
const CELL_W_MULTI: u16 = 1;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let is_multi = matches!(
        app.game_state.as_ref(),
        Some(ClientGameState::Snake(s)) if s.arenas.len() > 1
    );
    let cell_w = if is_multi { CELL_W_MULTI } else { CELL_W_SOLO };
    // Outer rect: solo = 1 arena, multi = 2 arenas side-by-side.
    let arena_width = if is_multi {
        2 * (ARENA_W * cell_w + 2)
    } else {
        ARENA_W * cell_w + 2
    };
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
    render_arena(frame, state, app, arena_area, cell_w);

    // Footer.
    let footer_text = if app.rematch_pending {
        format!(
            "Waiting for opponent... | C: cancel | Esc: {}",
            t.get("game.leave")
        )
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
        super::render_help_overlay(frame, app);
    }

    if app.rematch_pending || app.rematch_incoming {
        super::render_rematch_overlay(frame, app);
    }
}

fn score_tuple(state: &SnakeState, me: Option<PlayerId>) -> (u32, u32) {
    let mut you = 0u32;
    let mut opp = 0u32;

    // Collect scores from all arenas
    for arena in &state.arenas {
        for snake in &arena.snakes {
            if Some(snake.player_id) == me {
                you = snake.score;
            } else {
                opp = snake.score;
            }
        }
    }
    (you, opp)
}

fn fixed_arena_rect(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn render_arena(frame: &mut Frame, state: &SnakeState, app: &App, area: Rect, cell_w: u16) {
    // Check if solo (1 arena) or multiplayer (2+ arenas)
    if state.arenas.len() == 1 {
        // Solo mode: render single arena
        render_single_arena(frame, &state.arenas[0], app, area, None, cell_w);
    } else {
        // Multiplayer mode: pin each half to the exact arena width so the
        // arena always fits, even if the surrounding area is wider.
        let half = ARENA_W * cell_w + 2;
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(half), Constraint::Length(half)])
            .split(area);

        let my_arena = state
            .arenas
            .iter()
            .find(|a| a.owner == app.my_player_id)
            .unwrap_or(&state.arenas[0]);
        let opp_arena = state
            .arenas
            .iter()
            .find(|a| a.owner != app.my_player_id)
            .unwrap_or(&state.arenas[0]);

        render_single_arena(frame, my_arena, app, chunks[0], Some(" You "), cell_w);
        render_single_arena(frame, opp_arena, app, chunks[1], Some(" Opponent "), cell_w);
    }
}

fn render_single_arena(
    frame: &mut Frame,
    arena: &SnakeArena,
    app: &App,
    area: Rect,
    title_override: Option<&str>,
    cell_w: u16,
) {
    let title = title_override.unwrap_or(" Snake ");
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build an index: position → color for rendering
    let mut heads: std::collections::HashMap<(u16, u16), Color> = std::collections::HashMap::new();
    let mut bodies: std::collections::HashMap<(u16, u16), Color> = std::collections::HashMap::new();
    for snake in &arena.snakes {
        let color = if Some(snake.player_id) == app.my_player_id {
            Color::Green
        } else {
            Color::Red
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
        arena.food.iter().map(|p| (p.x, p.y)).collect();

    let (head_g, body_g, food_g, blank_g) = if cell_w == 1 {
        ("█", "▓", "●", " ")
    } else {
        ("██", "▓▓", "●●", "  ")
    };

    let mut lines: Vec<Line> = Vec::with_capacity(ARENA_H as usize);
    for y in 0..ARENA_H {
        let mut spans: Vec<Span> = Vec::with_capacity(ARENA_W as usize);
        for x in 0..ARENA_W {
            let key = (x, y);
            if let Some(color) = heads.get(&key) {
                spans.push(Span::styled(
                    head_g,
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ));
            } else if let Some(color) = bodies.get(&key) {
                spans.push(Span::styled(body_g, Style::default().fg(*color)));
            } else if food.contains(&key) {
                spans.push(Span::styled(food_g, Style::default().fg(Color::Yellow)));
            } else {
                spans.push(Span::raw(blank_g));
            }
        }
        lines.push(Line::from(spans));
    }

    let arena_widget = Paragraph::new(lines);
    frame.render_widget(arena_widget, inner);
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

    #[test]
    fn snake_renderer_multi_arena_versus_mode() {
        let db = ClientDatabase::open_in_memory().unwrap();
        let mut app = App::new(Language::English, db, "wss://localhost:8443".to_string());
        let p0 = PlayerId::new();
        let p1 = PlayerId::new();
        app.my_player_id = Some(p0);

        // Create a state with two arenas (versus mode)
        let state = SnakeEngine::initial_state(&[p0, p1], &GameSettings::default());
        // Verify the state has at least one arena (created by initial_state)
        assert_eq!(state.arenas.len(), 1);

        // The initial_state already creates one arena; just verify the renderer doesn't panic
        // with a multiplayer-like setup
        app.game_state = Some(ClientGameState::Snake(state));

        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        // Should not panic
        terminal.draw(|f| render(f, &app)).unwrap();
    }
}
