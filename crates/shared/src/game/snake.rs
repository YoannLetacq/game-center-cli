//! Shared Snake engine (arena 32×18, deterministic LCG, up to N snakes).
//!
//! Server-authoritative: server drives ticks, validates inputs, broadcasts deltas.
//! Same engine runs client-side for solo-vs-bot matches.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::game::traits::RealtimeGameEngine;
use crate::types::{Difficulty, GameOutcome, GameSettings, PlayerId};

pub const ARENA_W: u16 = 32;
pub const ARENA_H: u16 = 18;

const INITIAL_FOOD: usize = 2;
const SPAWN_RETRIES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snake {
    pub player_id: PlayerId,
    pub body: VecDeque<Position>,
    pub direction: Direction,
    pub pending_direction: Option<Direction>,
    pub alive: bool,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnakeState {
    pub arena_w: u16,
    pub arena_h: u16,
    pub snakes: Vec<Snake>,
    pub food: Vec<Position>,
    pub tick: u64,
    pub rng_seed: u64,
    pub rng_state: u64,
    pub game_over: Option<GameOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnakeInput {
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnakeDelta {
    pub tick: u64,
    pub moves: Vec<(PlayerId, Position)>,
    pub grew: Vec<PlayerId>,
    pub deaths: Vec<PlayerId>,
    pub new_food: Vec<Position>,
    pub eaten_food: Vec<Position>,
    pub game_over: Option<GameOutcome>,
}

/// Zero-size engine marker. Named `SnakeEngine` to avoid colliding with the
/// `Snake` per-player data struct above. Re-exported as `Engine` for brevity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SnakeEngine;

pub use self::SnakeEngine as Engine;

// ---------- LCG ----------------------------------------------------------

/// Knuth MMIX LCG — cheap, deterministic, good enough for spawns/bots.
fn lcg_next(s: &mut u64) -> u64 {
    *s = s
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *s
}

fn seed_from_players(players: &[PlayerId]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in players {
        p.0.as_bytes().hash(&mut h);
    }
    // Non-zero fallback so an empty slice still yields a usable state.
    let v = h.finish();
    if v == 0 { 0x9E37_79B9_7F4A_7C15 } else { v }
}

// ---------- Helpers ------------------------------------------------------

fn step(pos: Position, dir: Direction, w: u16, h: u16) -> Option<Position> {
    match dir {
        Direction::Up => {
            if pos.y == 0 {
                None
            } else {
                Some(Position {
                    x: pos.x,
                    y: pos.y - 1,
                })
            }
        }
        Direction::Down => {
            if pos.y + 1 >= h {
                None
            } else {
                Some(Position {
                    x: pos.x,
                    y: pos.y + 1,
                })
            }
        }
        Direction::Left => {
            if pos.x == 0 {
                None
            } else {
                Some(Position {
                    x: pos.x - 1,
                    y: pos.y,
                })
            }
        }
        Direction::Right => {
            if pos.x + 1 >= w {
                None
            } else {
                Some(Position {
                    x: pos.x + 1,
                    y: pos.y,
                })
            }
        }
    }
}

fn occupied_cells(state: &SnakeState) -> HashSet<Position> {
    let mut set = HashSet::new();
    for snake in &state.snakes {
        if !snake.alive {
            continue;
        }
        for cell in &snake.body {
            set.insert(*cell);
        }
    }
    for f in &state.food {
        set.insert(*f);
    }
    set
}

fn spawn_food(state: &mut SnakeState) -> Option<Position> {
    let occupied = occupied_cells(state);
    let total = (state.arena_w as usize) * (state.arena_h as usize);
    if occupied.len() >= total {
        return None;
    }
    for _ in 0..SPAWN_RETRIES {
        let r = lcg_next(&mut state.rng_state);
        let x = ((r >> 16) as u16) % state.arena_w;
        let y = ((r >> 32) as u16) % state.arena_h;
        let p = Position { x, y };
        if !occupied.contains(&p) {
            state.food.push(p);
            return Some(p);
        }
    }
    None
}

// ---------- Engine -------------------------------------------------------

impl RealtimeGameEngine for SnakeEngine {
    type Input = SnakeInput;
    type State = SnakeState;
    type Delta = SnakeDelta;

    fn initial_state(players: &[PlayerId], settings: &GameSettings) -> Self::State {
        let seed = settings.seed.unwrap_or_else(|| seed_from_players(players));
        let w = ARENA_W;
        let h = ARENA_H;
        let mid = h / 2;

        let mut snakes: Vec<Snake> = Vec::with_capacity(players.len());
        for (i, pid) in players.iter().enumerate() {
            let (dir, body): (Direction, VecDeque<Position>) = if i % 2 == 0 {
                // Left-side snake, head at x=4, faces Right, body extends left.
                let mut body = VecDeque::with_capacity(3);
                body.push_back(Position { x: 4, y: mid });
                body.push_back(Position { x: 3, y: mid });
                body.push_back(Position { x: 2, y: mid });
                (Direction::Right, body)
            } else {
                // Right-side snake, head at x=W-5, faces Left, body extends right.
                let mut body = VecDeque::with_capacity(3);
                body.push_back(Position { x: w - 5, y: mid });
                body.push_back(Position { x: w - 4, y: mid });
                body.push_back(Position { x: w - 3, y: mid });
                (Direction::Left, body)
            };
            snakes.push(Snake {
                player_id: *pid,
                body,
                direction: dir,
                pending_direction: None,
                alive: true,
                score: 0,
            });
        }

        let mut state = SnakeState {
            arena_w: w,
            arena_h: h,
            snakes,
            food: Vec::new(),
            tick: 0,
            rng_seed: seed,
            rng_state: seed,
            game_over: None,
        };

        for _ in 0..INITIAL_FOOD {
            spawn_food(&mut state);
        }

        state
    }

    fn tick(state: &mut Self::State, inputs: &HashMap<PlayerId, Self::Input>) -> Self::Delta {
        let mut delta = SnakeDelta {
            tick: state.tick + 1,
            moves: Vec::new(),
            grew: Vec::new(),
            deaths: Vec::new(),
            new_food: Vec::new(),
            eaten_food: Vec::new(),
            game_over: None,
        };

        // 1) Buffer pending direction from inputs (reject 180° reversal vs. committed dir).
        for snake in state.snakes.iter_mut() {
            if !snake.alive {
                continue;
            }
            if let Some(input) = inputs.get(&snake.player_id)
                && input.direction != snake.direction.opposite()
            {
                snake.pending_direction = Some(input.direction);
            }
        }

        // 2) Commit pending dir (defense in depth: reject reversal at commit too).
        for snake in state.snakes.iter_mut() {
            if !snake.alive {
                continue;
            }
            if let Some(pending) = snake.pending_direction.take()
                && pending != snake.direction.opposite()
            {
                snake.direction = pending;
            }
        }

        // 3) Compute new heads + determine deaths.
        let snake_count = state.snakes.len();
        let mut new_heads: Vec<Option<Position>> = vec![None; snake_count];
        let mut will_die: Vec<bool> = vec![false; snake_count];

        for (i, snake) in state.snakes.iter().enumerate() {
            if !snake.alive {
                continue;
            }
            let head = *snake.body.front().expect("alive snake has no body");
            match step(head, snake.direction, state.arena_w, state.arena_h) {
                Some(p) => new_heads[i] = Some(p),
                None => {
                    // Wall collision.
                    will_die[i] = true;
                }
            }
        }

        // First compute growth (head on food) BEFORE body-collision decisions,
        // because a growing snake keeps its tail this tick.
        let mut grows: Vec<bool> = vec![false; snake_count];
        for (i, snake) in state.snakes.iter().enumerate() {
            if !snake.alive || will_die[i] {
                continue;
            }
            if let Some(head) = new_heads[i]
                && state.food.contains(&head)
            {
                grows[i] = true;
            }
        }

        // Head-on-head collisions.
        for i in 0..snake_count {
            if will_die[i] {
                continue;
            }
            let Some(hi) = new_heads[i] else { continue };
            for j in (i + 1)..snake_count {
                if will_die[j] {
                    continue;
                }
                let Some(hj) = new_heads[j] else { continue };
                if hi == hj {
                    will_die[i] = true;
                    will_die[j] = true;
                }
            }
        }

        // Body collisions. A snake that is NOT growing will vacate its tail cell.
        for i in 0..snake_count {
            if will_die[i] {
                continue;
            }
            let Some(head) = new_heads[i] else { continue };
            for (j, other) in state.snakes.iter().enumerate() {
                if !other.alive {
                    continue;
                }
                let tail_vacated = !grows[j] && !will_die[j];
                let len = other.body.len();
                for (k, cell) in other.body.iter().enumerate() {
                    // Skip vacating tail cell.
                    if tail_vacated && k + 1 == len {
                        continue;
                    }
                    // Own head cell gets replaced this tick (snake moves forward) —
                    // only the resulting new_head matters for self-collision.
                    if i == j && k == 0 {
                        continue;
                    }
                    if *cell == head {
                        will_die[i] = true;
                        break;
                    }
                }
                if will_die[i] {
                    break;
                }
            }
        }

        // 4) Apply results.
        for i in 0..snake_count {
            if !state.snakes[i].alive {
                continue;
            }
            if will_die[i] {
                state.snakes[i].alive = false;
                delta.deaths.push(state.snakes[i].player_id);
                continue;
            }
            let Some(head) = new_heads[i] else { continue };
            state.snakes[i].body.push_front(head);
            if grows[i] {
                state.snakes[i].score += 1;
                delta.grew.push(state.snakes[i].player_id);
                // The food is eaten — record and remove from state.food.
                if let Some(idx) = state.food.iter().position(|f| *f == head) {
                    let eaten = state.food.remove(idx);
                    delta.eaten_food.push(eaten);
                }
            } else {
                state.snakes[i].body.pop_back();
            }
            delta.moves.push((state.snakes[i].player_id, head));
        }

        // 5) Respawn a food for every eaten one.
        let to_spawn = delta.eaten_food.len();
        for _ in 0..to_spawn {
            if let Some(p) = spawn_food(state) {
                delta.new_food.push(p);
            }
        }

        state.tick += 1;

        // 6) Check terminal condition.
        if state.game_over.is_none() {
            if let Some(outcome) = compute_terminal(state) {
                state.game_over = Some(outcome.clone());
                delta.game_over = Some(outcome);
            }
        } else {
            delta.game_over = state.game_over.clone();
        }

        delta
    }

    fn is_terminal(state: &Self::State) -> Option<GameOutcome> {
        if let Some(o) = &state.game_over {
            return Some(o.clone());
        }
        compute_terminal(state)
    }

    fn snapshot(state: &Self::State) -> Self::State {
        state.clone()
    }
}

fn compute_terminal(state: &SnakeState) -> Option<GameOutcome> {
    // Single-snake games never end by attrition.
    if state.snakes.len() < 2 {
        return None;
    }
    let alive: Vec<&Snake> = state.snakes.iter().filter(|s| s.alive).collect();
    match alive.len() {
        0 => Some(GameOutcome::Draw),
        1 => Some(GameOutcome::Win(alive[0].player_id)),
        _ => None,
    }
}

// ---------- Bot ----------------------------------------------------------

/// Deterministic order for tie-breaking.
const DIR_ORDER: [Direction; 4] = [
    Direction::Up,
    Direction::Right,
    Direction::Down,
    Direction::Left,
];

pub fn bot_move(state: &SnakeState, my_pid: PlayerId, difficulty: Difficulty) -> SnakeInput {
    let me = match state.snakes.iter().find(|s| s.player_id == my_pid) {
        Some(s) if s.alive => s,
        _ => {
            return SnakeInput {
                direction: Direction::Right,
            };
        }
    };
    let head = *me.body.front().expect("alive snake has non-empty body");
    let current = me.direction;

    // Candidate directions: all except 180° reversal.
    let candidates: Vec<Direction> = DIR_ORDER
        .iter()
        .copied()
        .filter(|d| *d != current.opposite())
        .collect();

    // Build obstacle set: all snake bodies, but EXCLUDE tails of snakes not growing next tick.
    // We approximate "growing next tick" as "head adjacent to food" — close enough for bot.
    let mut obstacles: HashSet<Position> = HashSet::new();
    for s in &state.snakes {
        if !s.alive {
            continue;
        }
        let will_grow = s
            .body
            .front()
            .map(|h| state.food.iter().any(|f| is_adjacent(*h, *f)))
            .unwrap_or(false);
        let len = s.body.len();
        for (k, cell) in s.body.iter().enumerate() {
            if !will_grow && k + 1 == len {
                continue;
            }
            obstacles.insert(*cell);
        }
    }

    match difficulty {
        Difficulty::Easy => bot_easy(state, head, current, &candidates, &obstacles),
        Difficulty::Hard => bot_hard(state, head, current, &candidates, &obstacles),
    }
}

fn is_adjacent(a: Position, b: Position) -> bool {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    dx + dy == 1
}

fn bot_easy(
    state: &SnakeState,
    head: Position,
    current: Direction,
    candidates: &[Direction],
    obstacles: &HashSet<Position>,
) -> SnakeInput {
    // BFS from head to nearest food, treating obstacles as walls.
    if let Some(dir) = bfs_first_step(state, head, &state.food, obstacles)
        && candidates.contains(&dir)
    {
        return SnakeInput { direction: dir };
    }
    // Fallback: first safe non-reversal direction.
    for d in candidates {
        if let Some(p) = step(head, *d, state.arena_w, state.arena_h)
            && !obstacles.contains(&p)
        {
            return SnakeInput { direction: *d };
        }
    }
    SnakeInput { direction: current }
}

fn bot_hard(
    state: &SnakeState,
    head: Position,
    current: Direction,
    candidates: &[Direction],
    obstacles: &HashSet<Position>,
) -> SnakeInput {
    let mut best: Option<(i32, Direction)> = None;

    for &dir in candidates {
        let Some(new_head) = step(head, dir, state.arena_w, state.arena_h) else {
            // Wall.
            continue;
        };
        if obstacles.contains(&new_head) {
            continue;
        }

        let open = flood_fill(state, new_head, obstacles);
        let dist_food = nearest_food_distance(state, new_head, obstacles).unwrap_or(1000);
        let score = (open as i32) * 10 + (1000 - dist_food as i32);

        let replace = match best {
            None => true,
            Some((bs, bd)) => {
                if score > bs {
                    true
                } else if score == bs {
                    // Tie-break: current direction first, then deterministic order.
                    if dir == current && bd != current {
                        true
                    } else if bd == current {
                        false
                    } else {
                        rank(dir) < rank(bd)
                    }
                } else {
                    false
                }
            }
        };
        if replace {
            best = Some((score, dir));
        }
    }

    if let Some((_, d)) = best {
        return SnakeInput { direction: d };
    }
    SnakeInput { direction: current }
}

fn rank(d: Direction) -> usize {
    DIR_ORDER.iter().position(|x| *x == d).unwrap_or(usize::MAX)
}

fn bfs_first_step(
    state: &SnakeState,
    start: Position,
    targets: &[Position],
    obstacles: &HashSet<Position>,
) -> Option<Direction> {
    if targets.is_empty() {
        return None;
    }
    let target_set: HashSet<Position> = targets.iter().copied().collect();

    let mut visited: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<(Position, Option<Direction>)> = VecDeque::new();

    visited.insert(start);
    for &d in &DIR_ORDER {
        if let Some(p) = step(start, d, state.arena_w, state.arena_h) {
            if obstacles.contains(&p) || visited.contains(&p) {
                continue;
            }
            visited.insert(p);
            queue.push_back((p, Some(d)));
        }
    }

    while let Some((pos, first_dir)) = queue.pop_front() {
        if target_set.contains(&pos) {
            return first_dir;
        }
        for &d in &DIR_ORDER {
            if let Some(p) = step(pos, d, state.arena_w, state.arena_h) {
                if obstacles.contains(&p) || visited.contains(&p) {
                    continue;
                }
                visited.insert(p);
                queue.push_back((p, first_dir));
            }
        }
    }
    None
}

fn flood_fill(state: &SnakeState, start: Position, obstacles: &HashSet<Position>) -> usize {
    if obstacles.contains(&start) {
        return 0;
    }
    let mut visited: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(p) = queue.pop_front() {
        for &d in &DIR_ORDER {
            if let Some(n) = step(p, d, state.arena_w, state.arena_h) {
                if obstacles.contains(&n) || visited.contains(&n) {
                    continue;
                }
                visited.insert(n);
                queue.push_back(n);
            }
        }
    }
    visited.len()
}

fn nearest_food_distance(
    state: &SnakeState,
    start: Position,
    obstacles: &HashSet<Position>,
) -> Option<usize> {
    if state.food.is_empty() {
        return None;
    }
    let targets: HashSet<Position> = state.food.iter().copied().collect();
    let mut visited: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<(Position, usize)> = VecDeque::new();
    visited.insert(start);
    queue.push_back((start, 0));
    while let Some((p, dist)) = queue.pop_front() {
        if targets.contains(&p) {
            return Some(dist);
        }
        for &d in &DIR_ORDER {
            if let Some(n) = step(p, d, state.arena_w, state.arena_h) {
                if obstacles.contains(&n) || visited.contains(&n) {
                    continue;
                }
                visited.insert(n);
                queue.push_back((n, dist + 1));
            }
        }
    }
    None
}

// ========================================================================
//                                   TESTS
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> (Vec<PlayerId>, PlayerId, PlayerId) {
        let a = PlayerId::new();
        let b = PlayerId::new();
        (vec![a, b], a, b)
    }

    fn settings_with_seed(seed: u64) -> GameSettings {
        GameSettings {
            seed: Some(seed),
            ..GameSettings::default()
        }
    }

    #[test]
    fn initial_state_deterministic_with_seed() {
        let (players, _, _) = two_players();
        let s1 = SnakeEngine::initial_state(&players, &settings_with_seed(42));
        let s2 = SnakeEngine::initial_state(&players, &settings_with_seed(42));
        assert_eq!(s1.food, s2.food);
        assert_eq!(s1.rng_seed, s2.rng_seed);
        assert_eq!(s1.rng_state, s2.rng_state);
        assert_eq!(s1.snakes.len(), 2);
        assert_eq!(s1.food.len(), INITIAL_FOOD);
    }

    #[test]
    fn cannot_reverse_180_within_one_tick() {
        let (players, a, _) = two_players();
        let mut state = SnakeEngine::initial_state(&players, &settings_with_seed(1));
        // Snake 0 starts facing Right. Try to reverse to Left.
        let mut inputs = HashMap::new();
        inputs.insert(
            a,
            SnakeInput {
                direction: Direction::Left,
            },
        );
        SnakeEngine::tick(&mut state, &inputs);
        assert_eq!(state.snakes[0].direction, Direction::Right);
        assert!(state.snakes[0].alive);
    }

    #[test]
    fn wall_collision_kills() {
        let (players, a, _) = two_players();
        let mut state = SnakeEngine::initial_state(&players, &settings_with_seed(7));
        // Walk snake 0 up until it hits the top wall.
        let mut inputs = HashMap::new();
        inputs.insert(
            a,
            SnakeInput {
                direction: Direction::Up,
            },
        );
        for _ in 0..ARENA_H + 2 {
            SnakeEngine::tick(&mut state, &inputs);
            if !state.snakes[0].alive {
                break;
            }
            inputs.clear();
            inputs.insert(
                a,
                SnakeInput {
                    direction: Direction::Up,
                },
            );
        }
        assert!(!state.snakes[0].alive, "snake should have hit the wall");
    }

    #[test]
    fn self_collision_kills() {
        // Build a tight loop manually: length-5 snake that turns into its own body.
        let pid = PlayerId::new();
        let body: VecDeque<Position> = [
            Position { x: 5, y: 5 },
            Position { x: 5, y: 6 },
            Position { x: 6, y: 6 },
            Position { x: 6, y: 5 },
            Position { x: 7, y: 5 },
        ]
        .into_iter()
        .collect();
        let snake = Snake {
            player_id: pid,
            body,
            direction: Direction::Up,
            pending_direction: None,
            alive: true,
            score: 0,
        };
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![snake],
            food: Vec::new(),
            tick: 0,
            rng_seed: 1,
            rng_state: 1,
            game_over: None,
        };
        // Head (5,5) facing Up. Force Right -> new head (6,5) which is body cell.
        let mut inputs = HashMap::new();
        inputs.insert(
            pid,
            SnakeInput {
                direction: Direction::Right,
            },
        );
        SnakeEngine::tick(&mut state, &inputs);
        assert!(!state.snakes[0].alive);
    }

    #[test]
    fn head_on_collision_both_die() {
        let a = PlayerId::new();
        let b = PlayerId::new();
        // Two snakes one step apart, moving toward each other.
        let snake_a = Snake {
            player_id: a,
            body: [Position { x: 10, y: 10 }, Position { x: 9, y: 10 }]
                .into_iter()
                .collect(),
            direction: Direction::Right,
            pending_direction: None,
            alive: true,
            score: 0,
        };
        let snake_b = Snake {
            player_id: b,
            body: [Position { x: 12, y: 10 }, Position { x: 13, y: 10 }]
                .into_iter()
                .collect(),
            direction: Direction::Left,
            pending_direction: None,
            alive: true,
            score: 0,
        };
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![snake_a, snake_b],
            food: Vec::new(),
            tick: 0,
            rng_seed: 1,
            rng_state: 1,
            game_over: None,
        };
        SnakeEngine::tick(&mut state, &HashMap::new());
        assert!(!state.snakes[0].alive);
        assert!(!state.snakes[1].alive);
        assert!(matches!(state.game_over, Some(GameOutcome::Draw)));
    }

    #[test]
    fn food_growth_extends_and_respawns() {
        let pid = PlayerId::new();
        let snake = Snake {
            player_id: pid,
            body: [Position { x: 5, y: 5 }, Position { x: 4, y: 5 }]
                .into_iter()
                .collect(),
            direction: Direction::Right,
            pending_direction: None,
            alive: true,
            score: 0,
        };
        let food = Position { x: 6, y: 5 };
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![snake],
            food: vec![food],
            tick: 0,
            rng_seed: 123,
            rng_state: 123,
            game_over: None,
        };
        let delta = SnakeEngine::tick(&mut state, &HashMap::new());
        assert_eq!(state.snakes[0].body.len(), 3, "snake should have grown");
        assert_eq!(state.snakes[0].score, 1);
        assert!(delta.grew.contains(&pid));
        assert_eq!(delta.eaten_food, vec![food]);
        assert_eq!(delta.new_food.len(), 1);
        assert_eq!(state.food.len(), 1);
    }

    #[test]
    fn bot_never_returns_reversal() {
        let (players, a, _) = two_players();
        let state = SnakeEngine::initial_state(&players, &settings_with_seed(9));
        let current = state.snakes[0].direction;
        for diff in [Difficulty::Easy, Difficulty::Hard] {
            let mv = bot_move(&state, a, diff);
            assert_ne!(mv.direction, current.opposite(), "{:?}", diff);
        }
    }

    #[test]
    fn bot_avoids_immediate_wall() {
        // Put a lone snake one cell from the right wall, facing Right.
        let pid = PlayerId::new();
        let snake = Snake {
            player_id: pid,
            body: [
                Position {
                    x: ARENA_W - 1,
                    y: 5,
                },
                Position {
                    x: ARENA_W - 2,
                    y: 5,
                },
            ]
            .into_iter()
            .collect(),
            direction: Direction::Right,
            pending_direction: None,
            alive: true,
            score: 0,
        };
        let state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![snake],
            food: vec![Position { x: 0, y: 5 }],
            tick: 0,
            rng_seed: 1,
            rng_state: 1,
            game_over: None,
        };
        for diff in [Difficulty::Easy, Difficulty::Hard] {
            let mv = bot_move(&state, pid, diff);
            assert_ne!(
                mv.direction,
                Direction::Right,
                "bot walked into wall ({:?})",
                diff
            );
            assert_ne!(mv.direction, Direction::Left, "bot reversed ({:?})", diff);
        }
    }

    #[test]
    fn lcg_is_deterministic() {
        let mut a = 42u64;
        let mut b = 42u64;
        for _ in 0..10 {
            assert_eq!(lcg_next(&mut a), lcg_next(&mut b));
        }
    }
}
