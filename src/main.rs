/*
Add undo: Build stack of moves
Add reset
*/
use clap::{App, Arg};
use failure::Error;
use itertools::put_back;
use quicksilver::{
    geom::{Circle, Line, Rectangle, Transform, Triangle, Vector},
    graphics::{Background::Col, Color},
    input::{ButtonState, Key, MouseButton},
    lifecycle::{run_with, Settings, State, Window},
    Result,
};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

const TILES_WIDE: usize = 8;
const TILES_HIGH: usize = 8;
const TILE_WIDTH: usize = 50;
const TILE_HEIGHT: usize = 50;

const FLOOR: u8 = b'*';
const WALL: u8 = b'&';
const LEFTRIGHT1: u8 = b'-';
const LEFTRIGHT2: u8 = b'_';
const UPDOWN1: u8 = b'|';
const UPDOWN2: u8 = b'(';
const PLAYER: u8 = b'=';
const EXIT: u8 = b'^';

#[derive(Debug, PartialEq)]
enum BlockDir {
    LeftRight,
    UpDown,
    Static,
}

#[derive(Debug, PartialEq)]
enum BlockType {
    Player,
    Other(u8),
    Wall,
    Exit,
}

#[derive(Debug)]
struct Block {
    dir: BlockDir,
    r#type: BlockType,
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    drag: bool,
    target_x: usize,
    target_y: usize,
}

impl Block {
    fn new(r#type: BlockType, dir: BlockDir, x1: usize, y1: usize, x2: usize, y2: usize) -> Block {
        Block {
            r#type,
            dir,
            x1,
            y1,
            x2,
            y2,
            ..Default::default()
        }
    }
}

impl Default for Block {
    fn default() -> Self {
        Block {
            r#type: BlockType::Wall,
            dir: BlockDir::Static,
            x1: 0,
            y1: 0,
            x2: 0,
            y2: 0,
            drag: false,
            target_x: 0,
            target_y: 0,
        }
    }
}

fn pos_to_xy(pos: usize) -> (usize, usize) {
    let x = pos % TILES_WIDE;
    let y = pos / TILES_WIDE;
    (x, y)
}

fn xy_to_pos(x: usize, y: usize) -> usize {
    x + y * 8
}

fn xy_to_sxy(x: usize, y: usize) -> (usize, usize) {
    let margin_x = (512 - TILE_WIDTH * TILES_WIDE) / 2;
    let margin_y = (512 - TILE_HEIGHT * TILES_HIGH) / 2;
    (x * TILE_WIDTH + margin_x, y * TILE_HEIGHT + margin_y)
}

fn sxy_to_xy(sx: usize, sy: usize) -> (usize, usize) {
    let margin_x = (512 - TILE_WIDTH * TILES_WIDE) / 2;
    let margin_y = (512 - TILE_HEIGHT * TILES_HIGH) / 2;
    ((sx - margin_x) / TILE_WIDTH, (sy - margin_y) / TILE_HEIGHT)
}

fn color(block: &Block) -> Color {
    match block.r#type {
        BlockType::Player => Color::RED,
        BlockType::Wall => Color::WHITE,
        BlockType::Exit => Color::YELLOW,
        BlockType::Other(_) => match block.dir {
            BlockDir::LeftRight => Color::BLUE,
            BlockDir::UpDown => Color::GREEN,
            _ => panic!("No Static + Other blocks exist"),
        },
    }
}

struct LevelSet {
    levels: Vec<Level>,
    current: usize,
}

impl LevelSet {
    fn load() -> Result<LevelSet> {
        let data = include_bytes!("../levels.dat");
        //fs::File::open(path.join("levels.dat"))?.read_to_end(&mut data)?;
        let mut levels = Vec::new();
        let mut data = put_back(data.into_iter().map(|b| *b));
        'outer: loop {
            let mut b = match data.next() {
                Some(byte) => byte,
                None => break,
            };
            // Allow comment lines before levels.
            if b == b'#' {
                while b != b'\n' {
                    b = match data.next() {
                        Some(byte) => byte,
                        None => break 'outer,
                    };
                }
                continue;
            }
            // Skip lines with just whitespace.
            if b == b' ' || b == b'\r' || b == b'\n' {
                while b == b' ' || b == b'\r' || b == b'\n' {
                    b = match data.next() {
                        Some(byte) => byte,
                        None => break 'outer,
                    };
                }
                data.put_back(b);
                continue;
            }
            data.put_back(b);
            let (lower, _upper) = data.size_hint();
            if lower < 64 {
                break;
            }
            // Load level data.
            levels.push(Level::from(&mut data));
        }
        Ok(LevelSet { levels, current: 0 })
    }

    fn current(&mut self) -> &mut Level {
        &mut self.levels[self.current]
    }

    fn next(&mut self) {
        if self.current + 1 < self.levels.len() {
            self.current += 1;
        }
    }

    fn previous(&mut self) {
        if self.current > 0 {
            self.current -= 1;
        }
    }
}

struct Level {
    template: [u8; TILES_WIDE * TILES_HIGH],
    data: [u8; TILES_WIDE * TILES_HIGH],
    blocks: Vec<Block>,
    // UI state
    mouse_pos: (usize, usize),
    drag_origin: Option<(usize, usize)>,
    drag_target: Option<usize>,
    solved: bool,
}

impl Level {
    fn from<I: Iterator<Item = u8> + Sized>(data: &mut I) -> Level {
        let mut level = Level::new().unwrap();
        level.parse(data);
        level
    }

    fn reset(&mut self) {
        self.solved = false;
        self.blocks = Vec::new();
        self.parse(&mut self.template.clone().into_iter().map(|b| *b));
    }

    fn parse<'a, I: Iterator<Item = u8> + Sized>(&mut self, data: &'a mut I) -> &'a mut I {
        let mut pos = 0;
        loop {
            let b = match data.next() {
                Some(byte) => byte,
                None => panic!("Not enough level data"),
            };
            if b != b' ' && b != b'\r' && b != b'\n' {
                self.template[pos] = b;
                pos += 1;
            }
            if pos == 64 {
                break;
            }
        }
        self.data = self.template.clone();
        let mut id = 1;
        assert!(pos == 64, "Corrupt data passed to parse: {}", pos);
        assert!(self.data.len() == 64, "Too many chars: {}", self.data.len());
        for pos in 0..self.data.len() {
            let (x, y) = pos_to_xy(pos);
            match self.data[pos] {
                WALL => {
                    self.blocks
                        .push(Block::new(BlockType::Wall, BlockDir::Static, x, y, x, y));
                }
                ch @ LEFTRIGHT1 | ch @ LEFTRIGHT2 => {
                    let mut pos2 = pos.clone();
                    while self.data[pos2] == ch {
                        self.data[pos2] = id;
                        pos2 += 1;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 1);
                    self.blocks.push(Block::new(
                        BlockType::Other(ch),
                        BlockDir::LeftRight,
                        x,
                        y,
                        x2,
                        y2,
                    ));
                }
                EXIT => {
                    self.blocks
                        .push(Block::new(BlockType::Exit, BlockDir::Static, x, y, x, y));
                }
                PLAYER => {
                    let mut pos2 = pos;
                    while self.data[pos2] == PLAYER {
                        self.data[pos2] = id;
                        pos2 += 1;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 1);
                    self.blocks.push(Block::new(
                        BlockType::Player,
                        BlockDir::LeftRight,
                        x,
                        y,
                        x2,
                        y2,
                    ));
                }
                ch @ UPDOWN1 | ch @ UPDOWN2 => {
                    let mut pos2 = pos;
                    while self.data[pos2] == ch {
                        self.data[pos2] = id;
                        pos2 += TILES_WIDE;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 8);
                    self.blocks.push(Block::new(
                        BlockType::Other(ch),
                        BlockDir::UpDown,
                        x,
                        y,
                        x2,
                        y2,
                    ));
                }
                FLOOR => {}
                _ => {}
            };
        }
        data
    }

    fn serialize(&self) -> [u8; 64] {
        let mut level = [b'*'; 64];
        for block in &self.blocks {
            for x in block.x1..block.x2 + 1 {
                for y in block.y1..block.y2 + 1 {
                    level[xy_to_pos(x, y)] = match block.r#type {
                        BlockType::Other(ch) => ch,
                        BlockType::Exit => b'^',
                        BlockType::Player => b'=',
                        BlockType::Wall => b'&',
                    }
                }
            }
        }
        level
    }

    fn to_string(&self) -> String {
        let bytes = self.serialize();
        String::from_utf8(bytes.to_vec()).expect("Unable to convert")
    }

    fn to_string_pretty(&self) -> String {
        let bytes = self.serialize();
        let mut string = String::new();
        for pos in 0..64 {
            string = format!("{}{}", string, bytes[pos] as char);
            if pos % 8 == 7 {
                string = format!("{}\n", string);
            }
        }
        string
    }
}

impl State for Level {
    fn new() -> Result<Level> {
        Ok(Level {
            template: [FLOOR; TILES_WIDE * TILES_HIGH],
            data: [FLOOR; TILES_WIDE * TILES_HIGH],
            blocks: Vec::new(),
            mouse_pos: (0, 0),
            drag_origin: None,
            drag_target: None,
            solved: false,
        })
    }

    fn update(&mut self, window: &mut Window) -> Result<()> {
        let mouse_pos = window.mouse().pos().into_point();
        // TODO: Stop using usize to for mouse_pos...
        if mouse_pos[0] > 0.0 && mouse_pos[1] > 0.0 {
            self.mouse_pos = (mouse_pos[0] as usize, mouse_pos[1] as usize);
        }
        if self.drag_origin.is_some() {
            match self.drag_target {
                Some(drag_target) => {
                    // Convert mouse pos to block pos, subtract from original pos to get delta pos.
                    let (mx, my) = self.mouse_pos;
                    let (bx, by) = sxy_to_xy(mx, my);
                    let (ox, oy) = self.drag_origin.unwrap();
                    let (dx, dy): (isize, isize) =
                        (bx as isize - ox as isize, by as isize - oy as isize);
                    let mut block = &mut self.blocks[drag_target];
                    block.target_x = block.x1;
                    block.target_y = block.y1;
                    let (x, y) = (block.x1, block.y1);
                    match block.dir {
                        BlockDir::LeftRight => {
                            let blocks_wide = block.x2 - block.x1;
                            // see if this is a valid move
                            let range: Vec<usize> = if dx > 0 {
                                (block.x1..block.x1 + dx as usize + 1).collect()
                            } else {
                                (block.x1 - dx.abs() as usize..block.x1).rev().collect()
                            };
                            for px in range {
                                if (self.data[xy_to_pos(px, y)] == FLOOR
                                    || self.data[xy_to_pos(px, y)] == EXIT
                                    || self.data[xy_to_pos(px, y)] == self.data[xy_to_pos(x, y)])
                                    && (self.data[xy_to_pos(px + blocks_wide, y)] == FLOOR
                                        || self.data[xy_to_pos(px + blocks_wide, y)] == EXIT
                                        || self.data[xy_to_pos(px + blocks_wide, y)]
                                            == self.data[xy_to_pos(x, y)])
                                {
                                    block.target_x = px;
                                } else {
                                    break;
                                }
                            }
                        }
                        BlockDir::UpDown => {
                            let blocks_high = block.y2 - block.y1;
                            // see if this is a valid move
                            let range: Vec<usize> = if dy > 0 {
                                (block.y1..block.y1 + dy as usize + 1).collect()
                            } else {
                                (block.y1 - dy.abs() as usize..block.y1).rev().collect()
                            };
                            for py in range {
                                if (self.data[xy_to_pos(x, py)] == FLOOR
                                    || self.data[xy_to_pos(x, py)] == self.data[xy_to_pos(x, y)])
                                    && (self.data[xy_to_pos(x, py + blocks_high)] == FLOOR
                                        || self.data[xy_to_pos(x, py + blocks_high)]
                                            == self.data[xy_to_pos(x, y)])
                                {
                                    block.target_y = py;
                                } else {
                                    break;
                                }
                            }
                        }
                        _ => panic!("Not a valid direction for a draggable block: {:#?}", block),
                    }
                }
                None => (),
            };
        }

        if window.mouse()[MouseButton::Left] == ButtonState::Pressed && self.drag_target.is_none() {
            let (mx, my) = self.mouse_pos;
            let (x, y) = sxy_to_xy(mx, my);
            self.drag_origin = Some((x, y));
            for (i, block) in self
                .blocks
                .iter_mut()
                .enumerate()
                .filter(|(_i, b)| b.dir != BlockDir::Static)
            {
                if (block.x1 <= x) && (x <= block.x2) && (block.y1 <= y) && (y <= block.y2) {
                    block.drag = true;
                    self.drag_target = Some(i);
                }
            }
        }
        if window.mouse()[MouseButton::Left] == ButtonState::Released && self.drag_target.is_some()
        {
            self.drag_target = None;
            self.drag_origin = None;
            for block in self.blocks.iter_mut() {
                if block.drag {
                    // Update block and data to reflect move.
                    let id = self.data[xy_to_pos(block.x1, block.y1)];
                    let width = block.x2 - block.x1;
                    let height = block.y2 - block.y1;
                    for x in block.x1..block.x2 + 1 {
                        for y in block.y1..block.y2 + 1 {
                            self.data[xy_to_pos(x, y)] = FLOOR;
                        }
                    }
                    block.x1 = block.target_x;
                    block.y1 = block.target_y;
                    block.target_x = 0;
                    block.target_y = 0;
                    block.x2 = block.x1 + width;
                    block.y2 = block.y1 + height;
                    for x in block.x1..block.x2 + 1 {
                        for y in block.y1..block.y2 + 1 {
                            if self.data[xy_to_pos(x, y)] == EXIT {
                                self.solved = true;
                            }
                            self.data[xy_to_pos(x, y)] = id;
                        }
                    }
                }
                block.drag = false;
            }
        }
        Ok(())
    }

    fn draw(&mut self, window: &mut Window) -> Result<()> {
        for block in self.blocks.iter_mut().rev() {
            let (mut x, mut y) = (block.x1, block.y1);
            let (width, height) = (
                (1 + block.x2 - block.x1) * TILE_WIDTH,
                (1 + block.y2 - block.y1) * TILE_HEIGHT,
            );
            if block.drag && block.target_x != 0 && block.target_y != 0 {
                x = block.target_x;
                y = block.target_y;
            }
            let (sx, sy) = xy_to_sxy(x, y);
            window.draw(
                &Rectangle::new((sx as f32, sy as f32), (width as f32, height as f32)),
                Col(color(block)),
            );
            // Top
            window.draw(
                &Line::new((sx as f32, sy as f32), ((sx + width) as f32, sy as f32)),
                Col(Color::BLACK),
            );
            // Left
            window.draw(
                &Line::new((sx as f32, sy as f32), (sx as f32, (sy + height) as f32)),
                Col(Color::BLACK),
            );
            // Right
            window.draw(
                &Line::new(
                    ((sx + width) as f32, sy as f32),
                    ((sx + width) as f32, (sy + height) as f32),
                ),
                Col(Color::BLACK),
            );
            // Bottom
            window.draw(
                &Line::new(
                    (sx as f32, (sy + height) as f32),
                    ((sx + width) as f32, (sy + height) as f32),
                ),
                Col(Color::BLACK),
            );
        }
        Ok(())
    }
}

impl State for LevelSet {
    fn new() -> Result<LevelSet> {
        Ok(LevelSet {
            levels: Vec::new(),
            current: 0,
        })
    }

    fn update(&mut self, window: &mut Window) -> Result<()> {
        if window.keyboard()[Key::N] == ButtonState::Pressed {
            self.next();
        }
        if window.keyboard()[Key::P] == ButtonState::Pressed {
            self.previous();
        }
        self.current().update(window)?;
        if self.current().solved {
            self.current().reset();
            self.next();
        }
        self.current().update(window)?;
        Ok(())
    }

    fn draw(&mut self, window: &mut Window) -> Result<()> {
        window.clear(Color::BLACK)?;
        self.current().draw(window)?;
        Ok(())
    }
}

fn main() {
    let levels = LevelSet::load().unwrap_or_else(|err| {
        eprintln!("Error loading levels.dat: {}", err);
        std::process::exit(1);
    });
    let mut settings = Settings::default();
    settings.update_rate = 100.0;
    settings.draw_rate = 50.0;
    run_with(
        "Unblock Me!",
        Vector::new(512, 512),
        Settings::default(),
        || Ok(levels),
    );
}
