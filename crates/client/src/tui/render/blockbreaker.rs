use gc_shared::game::blockbreaker::{
    BRICK_DESTROY_FRAMES, BRICK_FX_FRAMES, BlockBreakerState, BonusKind,
};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::{App, ClientGameState};

// 5-tone palette spread across the wheel so adjacent bricks read as
// distinct on a 16-color terminal. (Gemini review: dropped LightRed
// because it collapsed onto Red on most palettes; replaced with Blue.)
const BRICK_PALETTE: [Color; 5] = [
    Color::Red,
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
];
const BG: Color = Color::Reset;

pub fn render(frame: &mut Frame, app: &App) {
    let Some(ClientGameState::BlockBreaker(state)) = app.game_state.as_ref() else {
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),                  // header
            Constraint::Min(state.arena_h / 2 + 2), // arena (half-blocks halve height)
            Constraint::Length(1),                  // footer
        ])
        .split(frame.area());

    render_header(frame, chunks[0], state, app);
    render_arena(frame, chunks[1], state);
    render_footer(frame, chunks[2], state, app);

    if state.game_over {
        render_game_over_banner(frame, chunks[1], state);
    }
    if app.bb_paused && !state.game_over {
        render_pause_banner(frame, chunks[1]);
    }
    if app.show_help {
        render_help_modal(frame);
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &BlockBreakerState, app: &App) {
    let diff_label = match state.difficulty {
        gc_shared::game::blockbreaker::BBDifficulty::Easy => "Easy",
        gc_shared::game::blockbreaker::BBDifficulty::Hard => "Hard",
        gc_shared::game::blockbreaker::BBDifficulty::Hardcore => "Hardcore",
    };
    let line = Line::from(vec![
        Span::styled(
            format!("Score: {}  ", state.score),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Lives: {}  ", state.lives),
            Style::default().fg(Color::Red),
        ),
        Span::styled(
            format!("Level: {}  ", state.level),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            format!("[{diff_label}]  "),
            Style::default().fg(Color::Magenta),
        ),
        Span::styled(
            format!("Balls: {}", state.balls.len()),
            Style::default().fg(Color::Green),
        ),
    ]);
    let _ = app; // header doesn't need translator yet
    let p = Paragraph::new(line)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(p, area);
}

fn render_footer(frame: &mut Frame, area: Rect, state: &BlockBreakerState, app: &App) {
    let text = if state.game_over {
        "R: Restart  |  Esc: Leave"
    } else if app.bb_paused {
        "PAUSED  |  Space: Resume  |  Esc: Leave"
    } else if any_ball_stuck(state) {
        "Space: Launch  |  ←/→: Move paddle  |  Esc: Pause  |  I: Help"
    } else {
        "←/→: Move paddle  |  Esc: Pause  |  I: Help"
    };
    let _ = app;
    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(p, area);
}

fn any_ball_stuck(state: &BlockBreakerState) -> bool {
    state.balls.iter().any(|b| b.stuck)
}

fn render_arena(frame: &mut Frame, area: Rect, state: &BlockBreakerState) {
    let block = Block::default()
        .title(" Block Breaker ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Paint a sub-pixel grid then collapse pairs of rows into half-block chars.
    let w = state.arena_w as usize;
    let h = state.arena_h as usize;
    let mut grid: Vec<Color> = vec![BG; w * h];
    let put = |g: &mut [Color], x: i32, y: i32, c: Color| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return;
        }
        g[y as usize * w + x as usize] = c;
    };

    // Bricks (alive + fading).
    for brick in &state.bricks {
        let base = BRICK_PALETTE[(brick.color_idx as usize) % BRICK_PALETTE.len()];
        let color = if brick.alive {
            base
        } else {
            // Fade through Indexed shades by stepping toward DarkGray as
            // destroy_frames decreases.
            let f = brick.destroy_frames;
            if f >= BRICK_DESTROY_FRAMES * 2 / 3 {
                Color::White
            } else if f >= BRICK_DESTROY_FRAMES / 3 {
                Color::Gray
            } else {
                Color::DarkGray
            }
        };
        for dy in 0..brick.h {
            for dx in 0..brick.w {
                let x = brick.x as i32 + dx as i32;
                let y = brick.y as i32 + dy as i32;
                put(&mut grid, x, y, color);
            }
        }
    }

    // FX: brief 5-pixel cross on the brick that was just hit. Renders
    // for BRICK_FX_FRAMES (set in shared::blockbreaker) so the snap is
    // visible without lingering.
    for fx in &state.fx {
        if fx.frames == 0 {
            continue;
        }
        let col = Color::White;
        let cx = fx.x as i32;
        let cy = fx.y as i32;
        put(&mut grid, cx, cy, col);
        put(&mut grid, cx - 1, cy, col);
        put(&mut grid, cx + 1, cy, col);
        put(&mut grid, cx, cy - 1, col);
        put(&mut grid, cx, cy + 1, col);
    }
    let _ = BRICK_FX_FRAMES;

    // Bonuses — falling pickups, single sub-pixel.
    for bonus in &state.bonuses {
        let c = match bonus.kind {
            BonusKind::Enlarge => Color::LightBlue,
            BonusKind::MultiBall => Color::LightMagenta,
            BonusKind::BlastBlock => Color::LightRed,
            BonusKind::ExtraLife => Color::LightGreen,
        };
        let x = bonus.x as i32;
        let y = bonus.y as i32;
        put(&mut grid, x - 1, y, c);
        put(&mut grid, x, y, c);
        put(&mut grid, x + 1, y, c);
    }

    // Paddle: 1 sub-row tall at the state's paddle_y_top.
    let paddle_color = if state.paddle_flash > 0 {
        Color::White
    } else {
        Color::LightCyan
    };
    let half = state.paddle_w / 2.0;
    let p_left = (state.paddle_x - half).round() as i32;
    let p_right = (state.paddle_x + half).round() as i32;
    let p_y = state.paddle_y_top as i32;
    for x in p_left..p_right {
        put(&mut grid, x, p_y, paddle_color);
    }

    // Balls — single fixed high-contrast colour distinct from the brick
    // fade and FX (which use white) so the eye locks onto the ball.
    for ball in &state.balls {
        let x = ball.x as i32;
        let y = ball.y as i32;
        put(&mut grid, x, y, Color::LightYellow);
    }

    // Collapse to half-block chars (top sub-row, bottom sub-row) per char cell.
    let char_rows = h / 2;
    let render_w = inner.width.min(w as u16);
    let render_h = inner.height.min(char_rows as u16);
    let mut lines: Vec<Line> = Vec::with_capacity(render_h as usize);
    for cr in 0..render_h as usize {
        let mut spans: Vec<Span> = Vec::with_capacity(render_w as usize);
        for cc in 0..render_w as usize {
            let top = grid[(cr * 2) * w + cc];
            let bot = grid[(cr * 2 + 1) * w + cc];
            let span = match (top, bot) {
                (Color::Reset, Color::Reset) => Span::raw(" "),
                (t, Color::Reset) => Span::styled("▀", Style::default().fg(t)),
                (Color::Reset, b) => Span::styled("▄", Style::default().fg(b)),
                (t, b) if t == b => Span::styled("█", Style::default().fg(t)),
                (t, b) => Span::styled("▀", Style::default().fg(t).bg(b)),
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }
    let p = Paragraph::new(lines);
    frame.render_widget(p, inner);
}

fn render_game_over_banner(frame: &mut Frame, arena_area: Rect, state: &BlockBreakerState) {
    let text = format!(
        "GAME OVER  —  Score {}  Level {}  —  R: Restart  Esc: Leave",
        state.score, state.level
    );
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

fn render_pause_banner(frame: &mut Frame, arena_area: Rect) {
    let lines = [
        Line::from(Span::styled(
            "PAUSED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from("").alignment(Alignment::Center),
        Line::from("Space: Resume   Esc: Leave").alignment(Alignment::Center),
    ];
    let width = 36u16.min(arena_area.width);
    let height = 5u16.min(arena_area.height);
    let x = arena_area.x + arena_area.width.saturating_sub(width) / 2;
    let y = arena_area.y + arena_area.height.saturating_sub(height) / 2;
    let rect = Rect::new(x, y, width, height);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let p = Paragraph::new(lines.to_vec())
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(p, rect);
}

fn render_help_modal(frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);
    let body: Vec<Line> = [
        "Goal: break every brick to advance to the next level.",
        "Lose a life when the LAST ball falls below the paddle.",
        "",
        "Bonuses (catch with the paddle):",
        "  Blue   — Enlarge paddle",
        "  Pink   — Spawn an extra ball",
        "  Red    — Blast a random brick",
        "  Green  — +1 life (rare)",
        "",
        "Controls:",
        "  ←/→   Move paddle",
        "  Space Launch ball",
        "  I     Toggle this help",
        "  Esc   Pause (press Esc again to leave)",
    ]
    .into_iter()
    .map(Line::from)
    .collect();
    let block = Block::default()
        .title(" Block Breaker — Rules ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let p = Paragraph::new(body).block(block).alignment(Alignment::Left);
    frame.render_widget(p, area);
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
    use gc_shared::game::blockbreaker::{BBDifficulty, BlockBreakerState};
    use gc_shared::i18n::Language;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_without_panic_at_min_size() {
        let db = ClientDatabase::open_in_memory().unwrap();
        let mut app = App::new(Language::English, db, "wss://localhost:8443".to_string());
        app.game_state = Some(ClientGameState::BlockBreaker(BlockBreakerState::new(
            BBDifficulty::Easy,
            7,
        )));
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }
}
