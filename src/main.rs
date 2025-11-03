use macroquad::prelude::*;
use macroquad::audio::{self, Sound, PlaySoundParams, load_sound_from_bytes};
use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// Game constants
const SCREEN_WIDTH: i32 = 320;
const SCREEN_HEIGHT: i32 = 240;
const TILE_SIZE: i32 = 10;
const GRID_WIDTH: i32 = SCREEN_WIDTH / TILE_SIZE;
const GRID_HEIGHT: i32 = SCREEN_HEIGHT / TILE_SIZE;
const DEFAULT_MOVE_INTERVAL: f32 = 0.12; // default snake speed (seconds)

// Matrix-style palette
const MATRIX_HEAD: Color = Color::new(0.64, 1.0, 0.64, 1.0); // bright green
const MATRIX_BODY: Color = Color::new(0.25, 0.9, 0.25, 1.0); // medium green
const MATRIX_WALL: Color = Color::new(0.08, 0.4, 0.08, 1.0); // dark green
const MATRIX_FOOD: Color = Color::new(0.9, 1.0, 0.9, 1.0); // pale bright

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct Cell {
    x: i32,
    y: i32,
}

impl Cell {
    fn to_rect(self) -> Rect {
        Rect::new(
            (self.x * TILE_SIZE) as f32,
            (self.y * TILE_SIZE) as f32,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        )
    }
}

// Matrix glyph helpers
const MATRIX_GLYPHS: &[u8] = b"01<>[]{}()/\\|-=+*;:.,^~ABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn random_matrix_char() -> char {
    let idx = macroquad::rand::gen_range(0, MATRIX_GLYPHS.len());
    MATRIX_GLYPHS[idx] as char
}

fn matrix_char_for_cell(c: Cell) -> char {
    let hx = (c.x as i64).wrapping_mul(73_856_093);
    let hy = (c.y as i64).wrapping_mul(19_349_663);
    let h = (hx ^ hy).unsigned_abs() as usize;
    MATRIX_GLYPHS[h % MATRIX_GLYPHS.len()] as char
}

fn draw_glyph_at_cell(ch: char, cell: Cell, color: Color) {
    let x = (cell.x * TILE_SIZE) as f32 + 1.0;
    let y = (cell.y * TILE_SIZE) as f32 + TILE_SIZE as f32 - 1.0; // baseline
    let params = TextParams {
        font_size: TILE_SIZE as u16,
        font_scale: 1.0,
        font_scale_aspect: 1.0,
        color,
        ..Default::default()
    };
    draw_text_ex(&ch.to_string(), x, y, params);
}

fn draw_glyph_at_cell_scaled(
    ch: char,
    cell: Cell,
    color: Color,
    tile_w: f32,
    tile_h: f32,
    off_x: f32,
    off_y: f32,
){
    let x = off_x + (cell.x as f32) * tile_w + 1.0;
    let y = off_y + ((cell.y as f32 + 1.0) * tile_h) - 1.0; // baseline
    let size = tile_w.min(tile_h).max(6.0);
    let params = TextParams { font_size: size as u16, font_scale: 1.0, font_scale_aspect: 1.0, color, ..Default::default() };
    draw_text_ex(&ch.to_string(), x, y, params);
}

// Simple WAV (PCM16 mono) generator for tones
fn generate_wav_sine(frequency_hz: f32, duration_seconds: f32, volume: f32) -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let num_samples: u32 = (duration_seconds * sample_rate as f32) as u32;
    let mut data: Vec<u8> = Vec::with_capacity((num_samples as usize) * 2 + 44);

    let block_align: u16 = 2; // mono 16-bit
    let byte_rate: u32 = sample_rate * block_align as u32;
    let data_size: u32 = num_samples * 2;
    let chunk_size: u32 = 36 + data_size;

    // RIFF header
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&chunk_size.to_le_bytes());
    data.extend_from_slice(b"WAVE");
    // fmt chunk
    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    data.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    data.extend_from_slice(&1u16.to_le_bytes()); // channels
    data.extend_from_slice(&sample_rate.to_le_bytes());
    data.extend_from_slice(&byte_rate.to_le_bytes());
    data.extend_from_slice(&block_align.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    data.extend_from_slice(b"data");
    data.extend_from_slice(&data_size.to_le_bytes());

    let two_pi = std::f32::consts::TAU;
    let amplitude: f32 = (volume.clamp(0.0, 1.0)) * 0.7;
    for n in 0..num_samples {
        let t = n as f32 / sample_rate as f32;
        let sample = (amplitude * (two_pi * frequency_hz * t).sin() * i16::MAX as f32) as i16;
        data.extend_from_slice(&sample.to_le_bytes());
    }
    data
}

#[derive(Clone)]
struct Map {
    walls: HashSet<Cell>,
    seed: u64,
    wall_density: f32,
}

impl Map {
    fn is_wall(&self, c: Cell) -> bool { self.walls.contains(&c) }

    fn generate(seed: u64, wall_density: f32) -> Self {
        // Use global RNG seeded for reproducibility
        macroquad::rand::srand(seed);

        let mut walls: HashSet<Cell> = HashSet::new();

        // Border walls
        for x in 0..GRID_WIDTH {
            walls.insert(Cell { x, y: 0 });
            walls.insert(Cell { x, y: GRID_HEIGHT - 1 });
        }
        for y in 0..GRID_HEIGHT {
            walls.insert(Cell { x: 0, y });
            walls.insert(Cell { x: GRID_WIDTH - 1, y });
        }

        // Safe spawn area (3x3 around center)
        let spawn = Cell { x: GRID_WIDTH / 2, y: GRID_HEIGHT / 2 };
        let is_spawn_safe = |c: &Cell| (c.x - spawn.x).abs() <= 2 && (c.y - spawn.y).abs() <= 2;

        // Random interior walls
        for y in 1..(GRID_HEIGHT - 1) {
            for x in 1..(GRID_WIDTH - 1) {
                let c = Cell { x, y };
                if is_spawn_safe(&c) { continue; }
                let r: f32 = macroquad::rand::gen_range(0.0, 1.0);
                if r < wall_density { walls.insert(c); }
            }
        }

        Self { walls, seed, wall_density }
    }
}

struct SnakeGame {
    snake: Vec<Cell>,
    body_chars: Vec<char>,
    direction: Direction,
    next_direction: Direction,
    food: Cell,
    food_char: char,
    last_move_at: f32,
    grow: bool,
    score: u32,
    alive: bool,
    map: Map,
    move_interval: f32,
    eat_sound: Sound,
    die_sound: Sound,
    volume: f32,
}

impl SnakeGame {
    fn clone_for_game_over(&self) -> Self {
        Self {
            snake: self.snake.clone(),
            body_chars: self.body_chars.clone(),
            direction: self.direction,
            next_direction: self.next_direction,
            food: self.food,
            food_char: self.food_char,
            last_move_at: self.last_move_at,
            grow: self.grow,
            score: self.score,
            alive: self.alive,
            map: self.map.clone(),
            move_interval: self.move_interval,
            eat_sound: self.eat_sound.clone(),
            die_sound: self.die_sound.clone(),
            volume: self.volume,
        }
    }
    fn new(map: Map, move_interval: f32, eat_sound: Sound, die_sound: Sound, volume: f32) -> Self {
        let start = Cell { x: GRID_WIDTH / 2, y: GRID_HEIGHT / 2 };
        let initial_snake = vec![
            start,
            Cell { x: start.x - 1, y: start.y },
            Cell { x: start.x - 2, y: start.y },
        ];
        let initial_chars = vec![random_matrix_char(), random_matrix_char(), random_matrix_char()];
        let food = Self::spawn_food(&initial_snake, &map);
        let food_char = random_matrix_char();
        Self {
            snake: initial_snake,
            body_chars: initial_chars,
            direction: Direction::Right,
            next_direction: Direction::Right,
            food,
            food_char,
            last_move_at: 0.0,
            grow: false,
            score: 0,
            alive: true,
            map,
            move_interval,
            eat_sound,
            die_sound,
            volume: volume.clamp(0.0, 1.0),
        }
    }

    fn restart(&mut self) {
        let start = Cell { x: GRID_WIDTH / 2, y: GRID_HEIGHT / 2 };
        self.snake = vec![start, Cell { x: start.x - 1, y: start.y }, Cell { x: start.x - 2, y: start.y }];
        self.body_chars = vec![random_matrix_char(), random_matrix_char(), random_matrix_char()];
        self.direction = Direction::Right;
        self.next_direction = Direction::Right;
        self.food = Self::spawn_food(&self.snake, &self.map);
        self.food_char = random_matrix_char();
        self.last_move_at = 0.0;
        self.grow = false;
        self.score = 0;
        self.alive = true;
    }

    fn spawn_food(occupied: &[Cell], map: &Map) -> Cell {
        loop {
            let x = macroquad::rand::gen_range(1, GRID_WIDTH - 1);
            let y = macroquad::rand::gen_range(1, GRID_HEIGHT - 1);
            let cell = Cell { x, y };
            if !occupied.iter().any(|c| *c == cell) && !map.is_wall(cell) { return cell; }
        }
    }

    fn handle_input(&mut self) {
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            if self.direction != Direction::Down { self.next_direction = Direction::Up; }
        } else if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            if self.direction != Direction::Up { self.next_direction = Direction::Down; }
        } else if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            if self.direction != Direction::Right { self.next_direction = Direction::Left; }
        } else if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            if self.direction != Direction::Left { self.next_direction = Direction::Right; }
        }
    }

    fn step(&mut self) {
        if !self.alive { return; }
        if get_time() as f32 - self.last_move_at < self.move_interval { return; }
        self.last_move_at = get_time() as f32;

        self.direction = self.next_direction;
        let head = self.snake[0];
        let tentative = match self.direction {
            Direction::Up => Cell { x: head.x, y: head.y - 1 },
            Direction::Down => Cell { x: head.x, y: head.y + 1 },
            Direction::Left => Cell { x: head.x - 1, y: head.y },
            Direction::Right => Cell { x: head.x + 1, y: head.y },
        };

        // Bounds and wall collision (no wrap)
        if tentative.x < 0 || tentative.y < 0 || tentative.x >= GRID_WIDTH || tentative.y >= GRID_HEIGHT {
            self.alive = false;
            audio::play_sound(&self.die_sound, PlaySoundParams { looped: false, volume: 0.6 * self.volume });
            return;
        }
        if self.map.is_wall(tentative) {
            self.alive = false;
            audio::play_sound(&self.die_sound, PlaySoundParams { looped: false, volume: 0.6 * self.volume });
            return;
        }
        let new_head = tentative;

        // Self collision
        if self.snake.iter().any(|c| *c == new_head) {
            self.alive = false;
            audio::play_sound(&self.die_sound, PlaySoundParams { looped: false, volume: 0.6 * self.volume });
            return;
        }

        self.snake.insert(0, new_head);
        self.body_chars.insert(0, random_matrix_char());

        // Food collision
        if new_head == self.food {
            self.grow = true;
            self.score += 1;
            self.food = Self::spawn_food(&self.snake, &self.map);
            self.food_char = random_matrix_char();
            audio::play_sound(&self.eat_sound, PlaySoundParams { looped: false, volume: 0.35 * self.volume });
        }

        if !self.grow {
            self.snake.pop();
            self.body_chars.pop();
        } else {
            self.grow = false;
        }
    }

    fn draw(&self) {

        let sw = screen_width();
        let sh = screen_height();
        let tile_w = sw / GRID_WIDTH as f32;
        let tile_h = sh / GRID_HEIGHT as f32;
        let grid_w = tile_w * GRID_WIDTH as f32;
        let grid_h = tile_h * GRID_HEIGHT as f32;
        let off_x = (sw - grid_w) * 0.5;
        let off_y = (sh - grid_h) * 0.5;

        // Draw walls
        for c in &self.map.walls {
            let ch = matrix_char_for_cell(*c);
            draw_glyph_at_cell_scaled(ch, *c, MATRIX_WALL, tile_w, tile_h, off_x, off_y);
        }

        // Draw snake as Matrix glyphs
        for (i, (c, ch)) in self.snake.iter().zip(self.body_chars.iter()).enumerate() {
            let color = if i == 0 { MATRIX_HEAD } else { MATRIX_BODY };
            draw_glyph_at_cell_scaled(*ch, *c, color, tile_w, tile_h, off_x, off_y);
        }

        // Draw food glyph
        draw_glyph_at_cell_scaled(self.food_char, self.food, MATRIX_FOOD, tile_w, tile_h, off_x, off_y);

        // HUD
        let status = if self.alive { "Arrows/WASD to move" } else { "Game Over - R to restart, Enter to lobby" };
        draw_text(&format!("Score: {}", self.score), 8.0, 16.0, 24.0, MATRIX_BODY);
        draw_text(status, 8.0, 36.0, 18.0, MATRIX_WALL);
    }

    fn maybe_restart(&mut self) { /* handled by app screen */ }
}

struct LobbyState {
    seed: u64,
    wall_density: f32,
    move_interval: f32,
    selected: i32,
    preview_map: Map,
    preview_pos: Cell,
    preview_dir: Direction,
    preview_last_move: f32,
}

impl LobbyState {
    fn new() -> Self {
        let s = load_save();
        let time_seed = (get_time() as f64 * 1_000_000.0) as u64;
        let seed = if s.last_seed == 0 { time_seed } else { s.last_seed };
        let wall_density = if s.last_wall_density == 0.0 { 0.10 } else { s.last_wall_density };
        let move_interval = if s.last_move_interval == 0.0 {
            DEFAULT_MOVE_INTERVAL
        } else {
            s.last_move_interval
        };
        let preview_map = Map::generate(seed, wall_density);
        let preview_pos = Cell { x: GRID_WIDTH / 2, y: GRID_HEIGHT / 2 };
        let preview_dir = Direction::Right;
        Self {
            seed,
            wall_density,
            move_interval,
            selected: 0,
            preview_map,
            preview_pos,
            preview_dir,
            preview_last_move: 0.0,
        }
    }
}

struct SettingsState {
    sound_volume: f32,
}

enum Screen {
    Lobby(LobbyState),
    Settings(SettingsState),
    Playing(SnakeGame),
    GameOver(SnakeGame),
}

// Persistent storage
#[derive(Serialize, Deserialize, Default)]
struct SaveData {
    best_score: u32,
    last_seed: u64,
    last_wall_density: f32,
    last_move_interval: f32,
    sound_volume: f32,
}

fn save_path() -> String { "snake_save.json".to_string() }

fn load_save() -> SaveData {
    let path = save_path();
    if Path::new(&path).exists() {
        if let Ok(text) = fs::read_to_string(&path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else { SaveData::default() }
    } else { SaveData::default() }
}

fn write_save(data: &SaveData) {
    let _ = fs::write(save_path(), serde_json::to_string_pretty(data).unwrap_or_default());
}

// Matrix rain background
#[derive(Clone, Copy)]
struct Drop {
    x: i32,
    y: i32,
    speed: f32,
}

fn draw_matrix_rain(drops: &mut Vec<Drop>, dt: f32) {
    let sw = screen_width();
    let sh = screen_height();
    let tile_w = sw / GRID_WIDTH as f32;
    let tile_h = sh / GRID_HEIGHT as f32;
    let grid_w = tile_w * GRID_WIDTH as f32;
    let grid_h = tile_h * GRID_HEIGHT as f32;
    let off_x = (sw - grid_w) * 0.5;
    let off_y = (sh - grid_h) * 0.5;

    for d in drops.iter_mut() {
        d.y = (d.y as f32 + d.speed * dt) as i32;
        if d.y >= GRID_HEIGHT { d.y = 0; }
        let cell = Cell { x: d.x.clamp(0, GRID_WIDTH - 1), y: d.y.clamp(0, GRID_HEIGHT - 1) };
        draw_glyph_at_cell_scaled(random_matrix_char(), cell, Color::new(0.2, 0.8, 0.2, 0.5), tile_w, tile_h, off_x, off_y);
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Snake - Macroquad".to_owned(),
        fullscreen: true,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {

    // Sounds (simple generated beeps)
    let eat_bytes = generate_wav_sine(880.0, 0.08, 0.6);
    let die_bytes = generate_wav_sine(110.0, 0.25, 0.7);
    let eat_sound = load_sound_from_bytes(&eat_bytes).await.unwrap();
    let die_sound = load_sound_from_bytes(&die_bytes).await.unwrap();

    let mut sound_volume = {
        let s = load_save();
        if s.sound_volume == 0.0 { 1.0 } else { s.sound_volume }
    };
    let mut screen = Screen::Lobby(LobbyState::new());
    let mut drops: Vec<Drop> = (0..(GRID_WIDTH / 2)).map(|i| Drop { x: (i * 2) % GRID_WIDTH, y: macroquad::rand::gen_range(0, GRID_HEIGHT), speed: macroquad::rand::gen_range(6.0, 18.0) }).collect();
    let mut last_time = get_time() as f32;

    loop {
        let now = get_time() as f32;
        let dt = (now - last_time).max(0.0);
        last_time = now;

        if is_key_pressed(KeyCode::Q) { break; }

        clear_background(BLACK);
        draw_matrix_rain(&mut drops, dt);
        let mut next_screen: Option<Screen> = None;
        match &mut screen {
            Screen::Lobby(lobby) => {
                let sw = screen_width();
                let sh = screen_height();

                let title = "SNAKE";
                let t = measure_text(title, None, 40, 1.0);
                let mut y = sh * 0.25;
                draw_text(title, (sw - t.width) * 0.5, y, 40.0, MATRIX_HEAD);
                y += 56.0;

                let items = [
                    "Enter: Start",
                    "R: Reseed",
                    "- / + : Wall density",
                    "[ / ] : Speed",
                    "Q: Quit",
                ];
                for (i, text) in items.iter().enumerate() {
                    let color = if lobby.selected == i as i32 { WHITE } else { GRAY };
                    let m = measure_text(text, None, 20, 1.0);
                    draw_text(text, (sw - m.width) * 0.5, y, 20.0, color);
                    y += 24.0;
                }

                let sline = "S: Settings";
                let ms = measure_text(sline, None, 20, 1.0);
                draw_text(sline, (sw - ms.width) * 0.5, y, 20.0, GRAY);
                y += 24.0;

                let best = load_save().best_score;
                let best_s = format!("Best: {}", best);
                let mb = measure_text(&best_s, None, 20, 1.0);
                draw_text(&best_s, (sw - mb.width) * 0.5, sh - 64.0, 20.0, MATRIX_BODY);

                let params = format!(
                    "Seed: {}  Density: {:.0}%  Speed: {:.0}ms",
                    lobby.seed,
                    lobby.wall_density * 100.0,
                    lobby.move_interval * 1000.0
                );
                let mp = measure_text(&params, None, 18, 1.0);
                draw_text(&params, (sw - mp.width) * 0.5, sh - 40.0, 18.0, LIGHTGRAY);

                // Preview panel that reacts to difficulty
                // Target 85% of screen, maintain grid aspect and center
                let target_w = sw * 0.85;
                let target_h = sh * 0.85;
                let scale = (target_w / GRID_WIDTH as f32)
                    .min(target_h / GRID_HEIGHT as f32);
                let tile_w = scale;
                let tile_h = scale;
                let pw = tile_w * GRID_WIDTH as f32;
                let ph = tile_h * GRID_HEIGHT as f32;
                let off_x = (sw - pw) * 0.5;
                let off_y = (sh - ph) * 0.5;

                // Draw preview map walls
                for c in &lobby.preview_map.walls {
                    let ch = matrix_char_for_cell(*c);
                    draw_glyph_at_cell_scaled(
                        ch,
                        *c,
                        Color::new(MATRIX_WALL.r, MATRIX_WALL.g, MATRIX_WALL.b, 0.8),
                        tile_w,
                        tile_h,
                        off_x,
                        off_y,
                    );
                }

                // Advance preview head based on selected speed
                let now = get_time() as f32;
                if now - lobby.preview_last_move >= lobby.move_interval.max(0.05) {
                    lobby.preview_last_move = now;
                    // Try to move; if blocked, rotate direction
                    let head = lobby.preview_pos;
                    let mut try_dir = lobby.preview_dir;
                    let mut moved = false;
                    for _ in 0..4 {
                        let tentative = match try_dir {
                            Direction::Up => Cell { x: head.x, y: head.y - 1 },
                            Direction::Down => Cell { x: head.x, y: head.y + 1 },
                            Direction::Left => Cell { x: head.x - 1, y: head.y },
                            Direction::Right => Cell { x: head.x + 1, y: head.y },
                        };
                        let in_bounds = tentative.x > 0
                            && tentative.y > 0
                            && tentative.x < GRID_WIDTH - 1
                            && tentative.y < GRID_HEIGHT - 1;
                        if in_bounds && !lobby.preview_map.is_wall(tentative) {
                            lobby.preview_pos = tentative;
                            lobby.preview_dir = try_dir;
                            moved = true;
                            break;
                        }
                        // rotate direction clockwise
                        try_dir = match try_dir {
                            Direction::Up => Direction::Right,
                            Direction::Right => Direction::Down,
                            Direction::Down => Direction::Left,
                            Direction::Left => Direction::Up,
                        };
                    }
                    if !moved {
                        // regenerate spot near center to avoid stalling
                        lobby.preview_pos = Cell { x: GRID_WIDTH / 2, y: GRID_HEIGHT / 2 };
                        lobby.preview_dir = Direction::Right;
                    }
                }

                // Draw preview head glyph; color shifts with speed
                let speed_factor = (DEFAULT_MOVE_INTERVAL / lobby.move_interval)
                    .clamp(0.5, 4.0);
                let head_color = Color::new(
                    (0.15 * speed_factor).min(1.0),
                    (0.9 * (1.0 / speed_factor)).min(1.0),
                    0.2,
                    1.0,
                );
                draw_glyph_at_cell_scaled(
                    random_matrix_char(),
                    lobby.preview_pos,
                    head_color,
                    tile_w,
                    tile_h,
                    off_x,
                    off_y,
                );

                if is_key_pressed(KeyCode::Up) {
                    lobby.selected = if lobby.selected <= 0 { 4 } else { lobby.selected - 1 };
                }
                if is_key_pressed(KeyCode::Down) {
                    lobby.selected = if lobby.selected >= 4 { 0 } else { lobby.selected + 1 };
                }

                if is_key_pressed(KeyCode::Left) {
                    match lobby.selected {
                        2 => {
                            lobby.wall_density = (lobby.wall_density - 0.02).max(0.0);
                            lobby.preview_map = Map::generate(lobby.seed, lobby.wall_density);
                        }
                        3 => { lobby.move_interval = (lobby.move_interval + 0.02).min(0.35); }
                        _ => {}
                    }
                }
                if is_key_pressed(KeyCode::Right) {
                    match lobby.selected {
                        2 => {
                            lobby.wall_density = (lobby.wall_density + 0.02).min(0.35);
                            lobby.preview_map = Map::generate(lobby.seed, lobby.wall_density);
                        }
                        3 => { lobby.move_interval = (lobby.move_interval - 0.02).max(0.05); }
                        _ => {}
                    }
                }

                if is_key_pressed(KeyCode::R) {
                    lobby.seed = lobby
                        .seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1);
                    lobby.preview_map = Map::generate(lobby.seed, lobby.wall_density);
                }
                if is_key_pressed(KeyCode::Minus) {
                    lobby.wall_density = (lobby.wall_density - 0.02).max(0.0);
                    lobby.preview_map = Map::generate(lobby.seed, lobby.wall_density);
                }
                if is_key_pressed(KeyCode::Equal) {
                    lobby.wall_density = (lobby.wall_density + 0.02).min(0.35);
                    lobby.preview_map = Map::generate(lobby.seed, lobby.wall_density);
                }
                if is_key_pressed(KeyCode::LeftBracket) {
                    lobby.move_interval = (lobby.move_interval + 0.02).min(0.35);
                }
                if is_key_pressed(KeyCode::RightBracket) {
                    lobby.move_interval = (lobby.move_interval - 0.02).max(0.05);
                }

                if is_key_pressed(KeyCode::S) {
                    next_screen = Some(Screen::Settings(SettingsState { sound_volume }));
                }

                if is_key_pressed(KeyCode::Enter) {
                    match lobby.selected {
                        0 => {
                            let map = Map::generate(lobby.seed, lobby.wall_density);
                            let game = SnakeGame::new(
                                map,
                                lobby.move_interval,
                                eat_sound.clone(),
                                die_sound.clone(),
                                sound_volume,
                            );
                            let mut s = load_save();
                            s.last_seed = lobby.seed;
                            s.last_wall_density = lobby.wall_density;
                            s.last_move_interval = lobby.move_interval;
                            write_save(&s);
                            next_screen = Some(Screen::Playing(game));
                        }
                        1 => {
                            lobby.seed = lobby.seed
                                .wrapping_mul(6364136223846793005)
                                .wrapping_add(1);
                        }
                        4 => {
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                }
            }

            Screen::Settings(settings) => {
                let sw = screen_width();
                let sh = screen_height();

                let title = "SETTINGS";
                let t = measure_text(title, None, 36, 1.0);
                let mut y = sh * 0.25;
                draw_text(title, (sw - t.width) * 0.5, y, 36.0, MATRIX_HEAD);
                y += 56.0;

                let vol_line = format!("Volume: {:>3}%", (settings.sound_volume * 100.0).round() as i32);
                let mv = measure_text(&vol_line, None, 22, 1.0);
                draw_text(&vol_line, (sw - mv.width) * 0.5, y, 22.0, WHITE);
                y += 28.0;

                let hint1 = "Left/Right or -/+ : Adjust volume   M: Mute/Unmute";
                let mh1 = measure_text(hint1, None, 18, 1.0);
                draw_text(hint1, (sw - mh1.width) * 0.5, y, 18.0, GRAY);
                y += 24.0;

                let hint2 = "Enter/Esc: Back";
                let mh2 = measure_text(hint2, None, 18, 1.0);
                draw_text(hint2, (sw - mh2.width) * 0.5, y, 18.0, GRAY);

                if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Minus) {
                    settings.sound_volume = (settings.sound_volume - 0.05).max(0.0);
                }
                if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::Equal) {
                    settings.sound_volume = (settings.sound_volume + 0.05).min(1.0);
                }
                if is_key_pressed(KeyCode::M) {
                    settings.sound_volume = if settings.sound_volume > 0.0 { 0.0 } else { 1.0 };
                }
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
                    sound_volume = settings.sound_volume;
                    let mut s = load_save();
                    s.sound_volume = sound_volume;
                    write_save(&s);
                    next_screen = Some(Screen::Lobby(LobbyState::new()));
                }
            }

            Screen::Playing(game) => {
                game.handle_input();
                game.step();
                game.draw();

                if !game.alive {
                    // Move into GameOver by cloning minimal state
                    next_screen = Some(Screen::GameOver(SnakeGame { map: game.map.clone(), ..game.clone_for_game_over() }));
                }
            }

            Screen::GameOver(game) => {
                game.draw();
                // Overlay
                draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, 0.4));
                let sw = screen_width();
                let sh = screen_height();
                let title = "GAME OVER";
                let tm = measure_text(title, None, 36, 1.0);
                draw_text(title, (sw - tm.width) * 0.5, sh * 0.4, 36.0, MATRIX_HEAD);
                let hint = "R: Restart  Enter: Lobby  Q: Quit";
                let hm = measure_text(hint, None, 22, 1.0);
                draw_text(hint, (sw - hm.width) * 0.5, sh * 0.4 + 36.0 + 20.0, 22.0, WHITE);
                // Save best
                let mut s = load_save();
                if game.score > s.best_score { s.best_score = game.score; write_save(&s); }

                if is_key_pressed(KeyCode::R) { game.restart(); let map = game.map.clone(); let speed = game.move_interval; next_screen = Some(Screen::Playing(SnakeGame::new(map, speed, game.eat_sound.clone(), game.die_sound.clone(), sound_volume))); }
                if is_key_pressed(KeyCode::Enter) { next_screen = Some(Screen::Lobby(LobbyState::new())); }
            }
        }

        if let Some(ns) = next_screen { screen = ns; }

        next_frame().await;
    }
}
