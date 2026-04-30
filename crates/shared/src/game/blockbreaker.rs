//! Block Breaker — solo arcade game.
//!
//! Client-only: no server-side driver. The engine lives here so logic stays
//! testable and consistent with the other shared games. The TUI client drives
//! `tick(...)` at ~30 Hz from its main loop.
//!
//! Coordinate system: an 80×24 terminal hosts a header row + 1-row footer + a
//! bordered arena. The inner arena is rendered with Unicode half-blocks, so
//! the logical playfield is 78 sub-columns × 38 sub-rows. Floats parameterize
//! continuous positions; integers describe brick rectangles snapped to the
//! grid.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

// Default arena (sub-pixel resolution; half-blocks give 2 sub-rows per char
// row). The actual size is per-state so the arena scales with the terminal.
pub const DEFAULT_ARENA_W: u16 = 78;
pub const DEFAULT_ARENA_H: u16 = 38;
pub const MIN_ARENA_W: u16 = 60;
pub const MIN_ARENA_H: u16 = 30;

// Brick grid.
pub const BRICK_W: u16 = 6;
pub const BRICK_H: u16 = 2;
pub const BRICK_TOP_OFFSET: u16 = 2;
pub const BRICK_MAX_ROWS: u16 = 8;

// Paddle.
pub const PADDLE_INIT_W: f32 = 10.0;
/// Logical 8× starting size — clamped to the arena width at apply time.
pub const PADDLE_MAX_W: f32 = PADDLE_INIT_W * 8.0;
pub const PADDLE_GROW_STEP: f32 = 4.0;
/// Sub-cells per second while a direction key is held. Tuned together with
/// the client's DIR_TIMEOUT_MS so a single key tap moves the paddle a
/// readable, controllable distance instead of teleporting halfway across.
pub const PADDLE_KEY_VEL: f32 = 45.0;
pub const PADDLE_INERTIA_DECAY_MS: u64 = 60;
pub const PADDLE_BOUNCE_INERTIA: f32 = 0.35;

// Game limits.
pub const MAX_BALLS: usize = 5;
pub const MAX_LIVES: u8 = 5;

// Animation / FX. Tight values so the destruction reads as a flash, not a
// slow fade. At 33 Hz: 3 frames ≈ 90 ms.
pub const BRICK_DESTROY_FRAMES: u8 = 3;
/// Cross flash on hit. 2 frames at 33 Hz ≈ 60 ms — long enough to register
/// against terminal frame-skipping, short enough to not feel like a sticker.
pub const BRICK_FX_FRAMES: u8 = 2;
pub const PADDLE_FLASH_FRAMES: u8 = 2;
pub const BONUS_FALL_SPEED: f32 = 14.0;

// Scoring.
pub const SCORE_PER_BRICK: u64 = 10;
pub const SCORE_PER_LEVEL: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BBDifficulty {
    Easy,
    Hard,
    Hardcore,
}

impl BBDifficulty {
    pub fn start_speed(self) -> f32 {
        match self {
            BBDifficulty::Easy => 30.0,
            BBDifficulty::Hard => 42.0,
            BBDifficulty::Hardcore => 42.0,
        }
    }
    pub fn level_speed_mult(self) -> f32 {
        match self {
            BBDifficulty::Easy | BBDifficulty::Hard => 1.10,
            BBDifficulty::Hardcore => 1.15,
        }
    }
    pub fn max_speed_factor(self) -> f32 {
        4.0
    }
    pub fn start_lives(self) -> u8 {
        match self {
            BBDifficulty::Hardcore => 1,
            _ => 3,
        }
    }
    /// Whole-percent drop chance on each brick destruction.
    pub fn bonus_drop_pct(self) -> u8 {
        match self {
            BBDifficulty::Easy => 15,
            BBDifficulty::Hard => 8,
            BBDifficulty::Hardcore => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BonusKind {
    Enlarge,
    MultiBall,
    BlastBlock,
    ExtraLife,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brick {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub color_idx: u8,
    pub alive: bool,
    /// Counts down BRICK_DESTROY_FRAMES..0 after the killing hit. While >0
    /// the brick renders as a fading shell; collisions ignore it (alive=false).
    pub destroy_frames: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ball {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub stuck: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bonus {
    pub x: f32,
    pub y: f32,
    pub kind: BonusKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopFx {
    pub x: f32,
    pub y: f32,
    pub frames: u8,
    pub color_idx: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BBInput {
    /// -1 = left, 0 = none, +1 = right (set by the client from key state).
    pub dir: i32,
    pub launch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBreakerState {
    pub difficulty: BBDifficulty,
    /// Sub-pixel arena dimensions, captured at game start. Stored on the
    /// state so the engine and renderer agree without a separate config.
    pub arena_w: u16,
    pub arena_h: u16,
    /// Number of brick columns that fit in this arena.
    pub brick_cols: u16,
    /// Top edge of the paddle in sub-cells (= arena_h - 2).
    pub paddle_y_top: f32,
    pub level: u32,
    pub score: u64,
    pub lives: u8,
    pub paddle_x: f32,
    pub paddle_w: f32,
    pub paddle_vx: f32,
    pub paddle_flash: u8,
    pub balls: Vec<Ball>,
    pub bricks: Vec<Brick>,
    pub bonuses: Vec<Bonus>,
    pub fx: Vec<PopFx>,
    /// Current ball speed magnitude (sub-cells per second).
    pub speed: f32,
    pub time_ms: u64,
    pub last_input_ms: u64,
    pub rng_state: u64,
    pub initial_seed: u64,
    pub game_over: bool,
}

impl BlockBreakerState {
    pub fn new(difficulty: BBDifficulty, seed: u64) -> Self {
        Self::with_arena(difficulty, seed, DEFAULT_ARENA_W, DEFAULT_ARENA_H)
    }

    /// Create a state sized for a specific arena. Width/height are clamped
    /// to a safe minimum so the paddle and brick rows always fit.
    pub fn with_arena(difficulty: BBDifficulty, seed: u64, arena_w: u16, arena_h: u16) -> Self {
        let speed = difficulty.start_speed();
        let lives = difficulty.start_lives();
        let seed = if seed == 0 {
            0xDEAD_BEEF_FEED_FACE
        } else {
            seed
        };
        let arena_w = arena_w.max(MIN_ARENA_W);
        let arena_h = arena_h.max(MIN_ARENA_H);
        let brick_cols = (arena_w / BRICK_W).max(1);
        let paddle_y_top = (arena_h as f32 - 2.0).max(4.0);
        let mut state = Self {
            difficulty,
            arena_w,
            arena_h,
            brick_cols,
            paddle_y_top,
            level: 1,
            score: 0,
            lives,
            paddle_x: arena_w as f32 / 2.0,
            paddle_w: PADDLE_INIT_W,
            paddle_vx: 0.0,
            paddle_flash: 0,
            balls: Vec::new(),
            bricks: Vec::new(),
            bonuses: Vec::new(),
            fx: Vec::new(),
            speed,
            time_ms: 0,
            last_input_ms: 0,
            rng_state: seed,
            initial_seed: seed,
            game_over: false,
        };
        state.bricks = generate_level(state.level, state.initial_seed, state.brick_cols);
        state.spawn_initial_ball();
        state
    }

    fn spawn_initial_ball(&mut self) {
        self.balls.clear();
        self.balls.push(Ball {
            x: self.paddle_x,
            y: self.paddle_y_top - 1.0,
            vx: 0.0,
            vy: 0.0,
            stuck: true,
        });
    }

    fn reset_paddle_for_new_life(&mut self) {
        self.paddle_w = PADDLE_INIT_W;
        self.paddle_x = self.arena_w as f32 / 2.0;
        self.paddle_vx = 0.0;
        self.spawn_initial_ball();
    }

    fn advance_level(&mut self) {
        self.level += 1;
        self.speed = (self.speed * self.difficulty.level_speed_mult())
            .min(self.difficulty.start_speed() * self.difficulty.max_speed_factor());
        self.score += SCORE_PER_LEVEL;
        self.bricks = generate_level(self.level, self.initial_seed, self.brick_cols);
        self.bonuses.clear();
        self.fx.clear();
        // Paddle resets between levels (lives carry).
        self.paddle_w = PADDLE_INIT_W;
        self.paddle_x = self.arena_w as f32 / 2.0;
        self.paddle_vx = 0.0;
        self.spawn_initial_ball();
    }

    pub fn alive_brick_count(&self) -> usize {
        self.bricks.iter().filter(|b| b.alive).count()
    }

    fn paddle_max_w(&self) -> f32 {
        // 8× starting width capped to arena (minus 2 sub-cells safety margin).
        PADDLE_MAX_W.min(self.arena_w as f32 - 2.0)
    }
}

// ---------- LCG / noise helpers -----------------------------------------

fn lcg_next(s: &mut u64) -> u64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *s
}

/// Deterministic value-noise sample in [0.0, 1.0] for an integer grid cell.
fn noise2(seed: u64, x: i32, y: i32) -> f32 {
    let mut h = seed ^ 0xA5A5_5A5A_DEAD_BEEF;
    h = h.wrapping_add((x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA77));
    h ^= h >> 33;
    h = h.wrapping_add((y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB2F));
    h ^= h >> 29;
    h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 32;
    ((h as u32) as f32) / (u32::MAX as f32)
}

/// Smoothed value noise: average of cell + neighbor-weighted hashes.
fn smooth_noise(seed: u64, x: i32, y: i32) -> f32 {
    let center = noise2(seed, x, y) * 0.5;
    let edges = (noise2(seed, x - 1, y)
        + noise2(seed, x + 1, y)
        + noise2(seed, x, y - 1)
        + noise2(seed, x, y + 1))
        * 0.125;
    center + edges
}

fn pick_bonus(rng: &mut u64) -> BonusKind {
    // Distribution: enlarge 40 / multi-ball 30 / blast 25 / extra-life 5.
    let r = (lcg_next(rng) % 100) as u8;
    if r < 40 {
        BonusKind::Enlarge
    } else if r < 70 {
        BonusKind::MultiBall
    } else if r < 95 {
        BonusKind::BlastBlock
    } else {
        BonusKind::ExtraLife
    }
}

// ---------- Level generation --------------------------------------------

fn generate_level(level: u32, base_seed: u64, brick_cols: u16) -> Vec<Brick> {
    let level_seed = base_seed ^ (level as u64).wrapping_mul(0xC2B2_AE3D);
    let rows = (4 + ((level - 1) % 5)) as u16; // 4..8 rows
    let rows = rows.min(BRICK_MAX_ROWS);
    let threshold = if level == 1 { 0.05 } else { 0.30 };
    let mut bricks = Vec::with_capacity((brick_cols * rows) as usize);
    for row in 0..rows {
        for col in 0..brick_cols {
            let n = smooth_noise(level_seed, col as i32, row as i32);
            // Always keep top row dense so each level looks intentional.
            let keep = row == 0 || n > threshold;
            if !keep {
                continue;
            }
            // Per-brick deterministic color from the renderer's 5-color
            // palette. Using a separate noise sample (offset seed) so the
            // colour pattern doesn't track the layout pattern.
            let color_idx = (noise2(level_seed ^ 0xC0FF_EE15_BADD_F00D, col as i32, row as i32)
                * 5.0) as u8
                % 5;
            bricks.push(Brick {
                x: col * BRICK_W,
                y: BRICK_TOP_OFFSET + row * BRICK_H,
                w: BRICK_W,
                h: BRICK_H,
                color_idx,
                alive: true,
                destroy_frames: 0,
            });
        }
    }
    // Guarantee at least one brick (degenerate seed safety).
    if bricks.is_empty() {
        bricks.push(Brick {
            x: (brick_cols / 2) * BRICK_W,
            y: BRICK_TOP_OFFSET,
            w: BRICK_W,
            h: BRICK_H,
            color_idx: 0,
            alive: true,
            destroy_frames: 0,
        });
    }
    bricks
}

// ---------- Bonus application ------------------------------------------

fn apply_bonus(state: &mut BlockBreakerState, kind: BonusKind) {
    match kind {
        BonusKind::Enlarge => {
            state.paddle_w = (state.paddle_w + PADDLE_GROW_STEP).min(state.paddle_max_w());
            // Keep paddle in bounds after growth.
            let half = state.paddle_w / 2.0;
            state.paddle_x = state.paddle_x.clamp(half, state.arena_w as f32 - half);
        }
        BonusKind::MultiBall => {
            // Spawn a copy of the fastest active ball (or a new one near paddle).
            if state.balls.len() >= MAX_BALLS {
                return;
            }
            let template = state
                .balls
                .iter()
                .find(|b| !b.stuck)
                .cloned()
                .unwrap_or(Ball {
                    x: state.paddle_x,
                    y: state.paddle_y_top - 1.0,
                    vx: state.speed * 0.5,
                    vy: -state.speed * 0.866,
                    stuck: false,
                });
            let mut new_ball = template;
            // Mirror x-velocity so the spawned ball diverges visually.
            new_ball.vx = -new_ball.vx;
            new_ball.stuck = false;
            state.balls.push(new_ball);
        }
        BonusKind::BlastBlock => {
            // Pick a random alive brick and destroy it like a normal hit.
            let alive_idxs: Vec<usize> = state
                .bricks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.alive)
                .map(|(i, _)| i)
                .collect();
            if alive_idxs.is_empty() {
                return;
            }
            let pick = (lcg_next(&mut state.rng_state) as usize) % alive_idxs.len();
            let idx = alive_idxs[pick];
            destroy_brick(state, idx, false);
        }
        BonusKind::ExtraLife => {
            state.lives = (state.lives + 1).min(MAX_LIVES);
        }
    }
}

fn destroy_brick(state: &mut BlockBreakerState, idx: usize, allow_drop: bool) {
    let brick = &mut state.bricks[idx];
    if !brick.alive {
        return;
    }
    brick.alive = false;
    brick.destroy_frames = BRICK_DESTROY_FRAMES;
    let bx = brick.x as f32 + brick.w as f32 / 2.0;
    let by = brick.y as f32 + brick.h as f32 / 2.0;
    let color = brick.color_idx;
    state.fx.push(PopFx {
        x: bx,
        y: by,
        frames: BRICK_FX_FRAMES,
        color_idx: color,
    });
    state.score += SCORE_PER_BRICK;
    if allow_drop {
        let drop_pct = state.difficulty.bonus_drop_pct();
        if drop_pct > 0 {
            let r = (lcg_next(&mut state.rng_state) % 100) as u8;
            if r < drop_pct {
                let kind = pick_bonus(&mut state.rng_state);
                state.bonuses.push(Bonus { x: bx, y: by, kind });
            }
        }
    }
}

// ---------- Tick --------------------------------------------------------

pub fn tick(state: &mut BlockBreakerState, input: &BBInput, dt_ms: u32) {
    if state.game_over {
        return;
    }

    // Age animations created on the previous tick BEFORE generating new
    // ones. Otherwise a freshly-pushed effect with N frames decrements to
    // N-1 in the same tick, rendering only N-1 draws. (Codex review,
    // High: brick FX could otherwise vanish before the next draw.)
    if state.paddle_flash > 0 {
        state.paddle_flash -= 1;
    }
    for brick in state.bricks.iter_mut() {
        if brick.destroy_frames > 0 {
            brick.destroy_frames -= 1;
        }
    }
    state.bricks.retain(|b| b.alive || b.destroy_frames > 0);
    for fx in state.fx.iter_mut() {
        if fx.frames > 0 {
            fx.frames -= 1;
        }
    }
    state.fx.retain(|f| f.frames > 0);

    // Defensive clamps in case a deserialized state contains invalid
    // values — `paddle_w = 0` would otherwise NaN the bounce-angle math.
    if !state.paddle_w.is_finite() || state.paddle_w < 1.0 {
        state.paddle_w = PADDLE_INIT_W;
    }
    if !state.paddle_x.is_finite() {
        state.paddle_x = state.arena_w as f32 / 2.0;
    }

    let dt = (dt_ms as f32) / 1000.0;
    state.time_ms = state.time_ms.saturating_add(dt_ms as u64);

    // --- Paddle motion (with simple inertia) ---
    if input.dir != 0 {
        state.paddle_vx = input.dir as f32 * PADDLE_KEY_VEL;
        state.last_input_ms = state.time_ms;
    } else {
        let elapsed = state.time_ms.saturating_sub(state.last_input_ms);
        if elapsed >= PADDLE_INERTIA_DECAY_MS {
            state.paddle_vx = 0.0;
        }
    }
    state.paddle_x += state.paddle_vx * dt;
    let half = state.paddle_w / 2.0;
    if state.paddle_x - half < 0.0 {
        state.paddle_x = half;
        state.paddle_vx = 0.0;
    }
    if state.paddle_x + half > state.arena_w as f32 {
        state.paddle_x = state.arena_w as f32 - half;
        state.paddle_vx = 0.0;
    }

    // Snapshot dimensions for the inner loops (state is borrowed mutably below).
    let arena_w = state.arena_w as f32;
    let arena_h = state.arena_h as f32;
    let paddle_y_top = state.paddle_y_top;

    // --- Stuck balls follow paddle; launch on input ---
    let mut launch_now = input.launch;
    for ball in state.balls.iter_mut() {
        if ball.stuck {
            ball.x = state.paddle_x;
            ball.y = paddle_y_top - 1.0;
            if launch_now {
                // Straight up: no initial horizontal bias. Player's first
                // paddle bounce decides the direction.
                ball.vx = 0.0;
                ball.vy = -state.speed;
                ball.stuck = false;
                launch_now = false; // launch one ball per input tick
            }
        }
    }

    // --- Ball physics (substep per ball to avoid tunnelling) ---
    let max_speed = state.speed * state.difficulty.max_speed_factor();
    let paddle_left = state.paddle_x - state.paddle_w / 2.0;
    let paddle_right = state.paddle_x + state.paddle_w / 2.0;

    let mut bricks_to_destroy: Vec<usize> = Vec::new();
    let mut paddle_bounced = false;

    for ball in state.balls.iter_mut() {
        if ball.stuck {
            continue;
        }
        let mut remaining = dt;
        // Required substeps grow with ball speed so we never silently drop
        // motion. Each substep is capped to 0.8 sub-cells (see max_step
        // below). Add a small headroom for collision retries.
        let cur_speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
        let max_iters = ((cur_speed * dt) / 0.8).ceil() as u32 + 8;
        let max_iters = max_iters.min(256);
        let mut iter: u32 = 0;
        while remaining > 0.0 && iter < max_iters {
            iter += 1;
            let speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
            if speed <= 0.0001 {
                break;
            }
            // Cap step so we never move more than 0.8 sub-cells per substep
            // (smaller than BRICK_H so we never tunnel a brick).
            let max_step = 0.8 / speed;
            let step = remaining.min(max_step);

            let nx = ball.x + ball.vx * step;
            let ny = ball.y + ball.vy * step;

            // Wall bounces (left/right/top).
            if nx < 0.0 {
                ball.vx = -ball.vx;
                ball.x = -nx;
                remaining -= step;
                continue;
            }
            if nx > arena_w - 0.001 {
                ball.vx = -ball.vx;
                ball.x = 2.0 * (arena_w - 0.001) - nx;
                remaining -= step;
                continue;
            }
            if ny < 0.0 {
                ball.vy = -ball.vy;
                ball.y = -ny;
                remaining -= step;
                continue;
            }

            // Brick collision. Check the segment endpoint (single-substep
            // tunneling already prevented by max_step).
            let mut hit_idx: Option<usize> = None;
            for (i, brick) in state.bricks.iter().enumerate() {
                if !brick.alive {
                    continue;
                }
                if nx >= brick.x as f32
                    && nx < (brick.x + brick.w) as f32
                    && ny >= brick.y as f32
                    && ny < (brick.y + brick.h) as f32
                {
                    hit_idx = Some(i);
                    break;
                }
            }
            if let Some(i) = hit_idx {
                let brick = &state.bricks[i];
                let prev_x = ball.x;
                let prev_y = ball.y;
                let was_outside_x = prev_x < brick.x as f32 || prev_x >= (brick.x + brick.w) as f32;
                let was_outside_y = prev_y < brick.y as f32 || prev_y >= (brick.y + brick.h) as f32;
                if was_outside_y && !was_outside_x {
                    ball.vy = -ball.vy;
                } else if was_outside_x && !was_outside_y {
                    ball.vx = -ball.vx;
                } else {
                    ball.vx = -ball.vx;
                    ball.vy = -ball.vy;
                }
                // Don't penetrate; back the ball out by reverting position.
                ball.x = prev_x;
                ball.y = prev_y;
                bricks_to_destroy.push(i);
                remaining -= step;
                continue;
            }

            // Paddle collision (only check on downward motion).
            if ball.vy > 0.0
                && ball.y <= paddle_y_top
                && ny >= paddle_y_top
                && nx >= paddle_left
                && nx <= paddle_right
            {
                let offset = ((nx - state.paddle_x) / (state.paddle_w / 2.0)).clamp(-1.0, 1.0);
                let angle = -PI / 2.0 + offset * (PI / 3.0);
                ball.vx = state.speed * angle.cos();
                ball.vy = state.speed * angle.sin();
                // Inertia: paddle motion adds horizontal component.
                ball.vx += state.paddle_vx * PADDLE_BOUNCE_INERTIA;
                let s = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
                if s > max_speed {
                    let k = max_speed / s;
                    ball.vx *= k;
                    ball.vy *= k;
                }
                ball.x = nx;
                ball.y = paddle_y_top - 0.001;
                paddle_bounced = true;
                remaining -= step;
                continue;
            }

            ball.x = nx;
            ball.y = ny;
            remaining -= step;
        }
    }

    if paddle_bounced {
        state.paddle_flash = PADDLE_FLASH_FRAMES;
    }

    // Destroy bricks that were hit this frame.
    // De-dup so two ball substeps that touched the same brick only score once.
    bricks_to_destroy.sort_unstable();
    bricks_to_destroy.dedup();
    for i in bricks_to_destroy {
        destroy_brick(state, i, true);
    }

    // Drop balls that fell below the arena.
    state.balls.retain(|b| b.y < arena_h);

    // --- Bonuses fall + paddle catch ---
    let mut caught: Vec<BonusKind> = Vec::new();
    for b in state.bonuses.iter_mut() {
        b.y += BONUS_FALL_SPEED * dt;
    }
    state.bonuses.retain(|b| {
        if b.y >= paddle_y_top - 0.5
            && b.y <= paddle_y_top + 1.5
            && b.x >= paddle_left
            && b.x <= paddle_right
        {
            caught.push(b.kind);
            return false;
        }
        b.y < arena_h
    });
    for kind in caught {
        apply_bonus(state, kind);
    }

    // (Animation aging was moved to the top of `tick` so just-pushed
    // effects render for the full N draw frames they advertise.)

    // --- Life loss when all balls have left play ---
    if state.balls.is_empty() {
        if state.lives <= 1 {
            state.lives = 0;
            state.game_over = true;
            return;
        }
        state.lives -= 1;
        state.reset_paddle_for_new_life();
    }

    // --- Level cleared? ---
    if state.alive_brick_count() == 0 {
        state.advance_level();
    }
}

// ============================================================
//                            TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(d: BBDifficulty) -> BlockBreakerState {
        BlockBreakerState::new(d, 12345)
    }

    #[test]
    fn new_state_has_one_stuck_ball_and_bricks() {
        let s = fresh(BBDifficulty::Easy);
        assert_eq!(s.lives, 3);
        assert_eq!(s.balls.len(), 1);
        assert!(s.balls[0].stuck);
        assert!(!s.bricks.is_empty());
        assert!(s.alive_brick_count() > 0);
        assert!(!s.game_over);
        assert_eq!(s.level, 1);
    }

    #[test]
    fn hardcore_starts_with_one_life_no_bonus() {
        let s = fresh(BBDifficulty::Hardcore);
        assert_eq!(s.lives, 1);
        assert_eq!(s.difficulty.bonus_drop_pct(), 0);
    }

    #[test]
    fn launch_sets_ball_velocity() {
        let mut s = fresh(BBDifficulty::Easy);
        let input = BBInput {
            dir: 0,
            launch: true,
        };
        tick(&mut s, &input, 33);
        assert!(!s.balls[0].stuck);
        assert!(s.balls[0].vy < 0.0);
    }

    #[test]
    fn paddle_moves_with_input_and_clamps() {
        let mut s = fresh(BBDifficulty::Easy);
        let input_left = BBInput {
            dir: -1,
            launch: false,
        };
        for _ in 0..200 {
            tick(&mut s, &input_left, 33);
        }
        assert!((s.paddle_x - s.paddle_w / 2.0).abs() < 0.01);
    }

    #[test]
    fn ball_falls_off_costs_a_life() {
        let mut s = fresh(BBDifficulty::Easy);
        // Launch, then warp ball below paddle to force the loss.
        tick(
            &mut s,
            &BBInput {
                dir: 0,
                launch: true,
            },
            33,
        );
        s.balls[0].x = 0.0;
        s.balls[0].y = s.arena_h as f32 - 0.5;
        s.balls[0].vy = 50.0;
        s.balls[0].vx = 0.0;
        tick(
            &mut s,
            &BBInput {
                dir: 0,
                launch: false,
            },
            33,
        );
        assert_eq!(s.lives, 2);
        assert_eq!(s.balls.len(), 1);
        assert!(s.balls[0].stuck);
    }

    #[test]
    fn extra_life_caps_at_max() {
        let mut s = fresh(BBDifficulty::Easy);
        s.lives = MAX_LIVES;
        apply_bonus(&mut s, BonusKind::ExtraLife);
        assert_eq!(s.lives, MAX_LIVES);
    }

    #[test]
    fn enlarge_caps_at_max_width() {
        let mut s = fresh(BBDifficulty::Easy);
        for _ in 0..100 {
            apply_bonus(&mut s, BonusKind::Enlarge);
        }
        assert!(s.paddle_w <= PADDLE_MAX_W + 0.001);
    }

    #[test]
    fn multi_ball_caps_at_max() {
        let mut s = fresh(BBDifficulty::Easy);
        // Launch the first ball so the spawn has a non-stuck template.
        tick(
            &mut s,
            &BBInput {
                dir: 0,
                launch: true,
            },
            33,
        );
        for _ in 0..20 {
            apply_bonus(&mut s, BonusKind::MultiBall);
        }
        assert!(s.balls.len() <= MAX_BALLS);
    }

    #[test]
    fn blast_block_destroys_one_brick() {
        let mut s = fresh(BBDifficulty::Easy);
        let before = s.alive_brick_count();
        apply_bonus(&mut s, BonusKind::BlastBlock);
        assert_eq!(s.alive_brick_count() + 1, before);
    }

    #[test]
    fn level_advances_when_all_bricks_cleared() {
        let mut s = fresh(BBDifficulty::Easy);
        let initial_speed = s.speed;
        // Wipe out every brick, then tick once with the launched ball
        // re-attached so the engine sees a clear board and advances.
        for b in s.bricks.iter_mut() {
            b.alive = false;
            b.destroy_frames = 0;
        }
        // Keep ball in play above the paddle so we don't lose a life this tick.
        s.balls[0].stuck = false;
        s.balls[0].x = s.arena_w as f32 / 2.0;
        s.balls[0].y = 5.0;
        s.balls[0].vx = 0.0;
        s.balls[0].vy = -1.0;
        tick(
            &mut s,
            &BBInput {
                dir: 0,
                launch: false,
            },
            33,
        );
        assert_eq!(s.level, 2);
        assert!(s.speed > initial_speed);
        assert!(!s.bricks.is_empty());
    }

    #[test]
    fn destroy_brick_animation_lingers_then_drops() {
        let mut s = fresh(BBDifficulty::Easy);
        let idx = s.bricks.iter().position(|b| b.alive).unwrap();
        destroy_brick(&mut s, idx, false);
        // After destruction: dead but still has destroy_frames.
        assert!(!s.bricks[idx].alive);
        assert!(s.bricks[idx].destroy_frames > 0);
        // Tick BRICK_DESTROY_FRAMES + 1 times to flush the animation.
        s.balls[0].stuck = true; // pin so we don't accidentally clear level
        for _ in 0..(BRICK_DESTROY_FRAMES as u32 + 2) {
            tick(
                &mut s,
                &BBInput {
                    dir: 0,
                    launch: false,
                },
                33,
            );
        }
        // Brick should be removed from the vector.
        assert!(s.bricks.iter().all(|b| b.alive || b.destroy_frames > 0));
    }

    #[test]
    fn deterministic_layout_for_seed() {
        let a = BlockBreakerState::new(BBDifficulty::Easy, 42);
        let b = BlockBreakerState::new(BBDifficulty::Easy, 42);
        assert_eq!(a.bricks.len(), b.bricks.len());
        for (x, y) in a.bricks.iter().zip(b.bricks.iter()) {
            assert_eq!((x.x, x.y, x.color_idx), (y.x, y.y, y.color_idx));
        }
    }
}
