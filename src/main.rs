/*
Add undo: Build stack of moves
*/

use gate::renderer::{Affine, Renderer};
use gate::{App, AppContext, AppInfo, KeyCode};
use itertools::put_back;

mod asset_id {
    include!(concat!(env!("OUT_DIR"), "/asset_id.rs"));
}
use crate::asset_id::{AssetId, SpriteId};

#[cfg(target_arch = "wasm32")]
gate::gate_header!();

const BLACK: (u8, u8, u8) = (0, 0, 0);

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
    sprite: SpriteId,
}

impl Block {
    fn new(
        r#type: BlockType,
        dir: BlockDir,
        x1: usize,
        y1: usize,
        x2: usize,
        y2: usize,
        sprite: SpriteId,
    ) -> Block {
        Block {
            r#type,
            dir,
            x1,
            y1,
            x2,
            y2,
            sprite,
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
            sprite: SpriteId::Wall,
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

struct LevelSet {
    levels: Vec<Level>,
    current: usize,
}

impl LevelSet {
    fn load() -> LevelSet {
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
        LevelSet { levels, current: 0 }
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

struct Move {
    block: usize,
    x: usize,
    y: usize,
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
    width: usize,
    height: usize,
    moves: Vec<Move>,
}

fn xy_to_sxy(width: usize, height: usize, x: usize, y: usize) -> (usize, usize) {
    let margin_x = (width - TILE_WIDTH * TILES_WIDE) / 2;
    let margin_y = (height - TILE_HEIGHT * TILES_HIGH) / 2;
    (x * TILE_WIDTH + margin_x, y * TILE_HEIGHT + margin_y)
}

impl Level {
    fn new() -> Level {
        Level {
            template: [FLOOR; TILES_WIDE * TILES_HIGH],
            data: [FLOOR; TILES_WIDE * TILES_HIGH],
            blocks: Vec::new(),
            mouse_pos: (0, 0),
            drag_origin: None,
            drag_target: None,
            solved: false,
            width: 800,
            height: 600,
            moves: Vec::new(),
        }
    }

    fn from<I: Iterator<Item = u8> + Sized>(data: &mut I) -> Level {
        let mut level = Level::new();
        level.parse(data);
        level
    }

    fn sxy_to_xy(&self, sx: usize, sy: usize) -> (usize, usize) {
        let margin_x = (self.width - TILE_WIDTH * TILES_WIDE) / 2;
        let margin_y = (self.height - TILE_HEIGHT * TILES_HIGH) / 2;
        ((sx - margin_x) / TILE_WIDTH, (sy - margin_y) / TILE_HEIGHT)
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
                    self.blocks.push(Block::new(
                        BlockType::Wall,
                        BlockDir::Static,
                        x,
                        y,
                        x,
                        y,
                        SpriteId::Wall,
                    ));
                }
                ch @ LEFTRIGHT1 | ch @ LEFTRIGHT2 => {
                    let mut pos2 = pos.clone();
                    while self.data[pos2] == ch {
                        self.data[pos2] = id;
                        pos2 += 1;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 1);
                    let sprite = match x2 - x {
                        1 => SpriteId::Horiz2,
                        2 => SpriteId::Horiz3,
                        _ => panic!("Unsupported horizontal block width: {}", 1 + x2 - x),
                    };
                    self.blocks.push(Block::new(
                        BlockType::Other(ch),
                        BlockDir::LeftRight,
                        x,
                        y,
                        x2,
                        y2,
                        sprite,
                    ));
                }
                EXIT => {
                    self.blocks.push(Block::new(
                        BlockType::Exit,
                        BlockDir::Static,
                        x,
                        y,
                        x,
                        y,
                        SpriteId::Exit,
                    ));
                }
                PLAYER => {
                    let mut pos2 = pos;
                    while self.data[pos2] == PLAYER {
                        self.data[pos2] = id;
                        pos2 += 1;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 1);
                    let sprite = match x2 - x {
                        1 => SpriteId::Player,
                        _ => panic!("Unsupported player block width: {}", 1 + x2 - x),
                    };
                    self.blocks.push(Block::new(
                        BlockType::Player,
                        BlockDir::LeftRight,
                        x,
                        y,
                        x2,
                        y2,
                        sprite,
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
                    let sprite = match y2 - y {
                        1 => SpriteId::Vert2,
                        2 => SpriteId::Vert3,
                        _ => panic!("Unsupported vertical block height: {}", 1 + y2 - y),
                    };
                    self.blocks.push(Block::new(
                        BlockType::Other(ch),
                        BlockDir::UpDown,
                        x,
                        y,
                        x2,
                        y2,
                        sprite,
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

    fn drag_to(&mut self, mx: usize, my: usize) {
        let drag_target = match self.drag_target {
            Some(dt) => dt,
            None => return,
        };
        let (bx, by) = self.sxy_to_xy(mx, my);
        let (ox, oy) = self.drag_origin.unwrap();
        let (dx, dy): (isize, isize) = (bx as isize - ox as isize, by as isize - oy as isize);
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
            _ => panic!(
                "Not a valid direction for a draggable block: {:#?}",
                block.r#type
            ),
        }
    }

    fn begin_drag(&mut self, mx: usize, my: usize) {
        let (x, y) = self.sxy_to_xy(mx, my);
        self.drag_origin = Some((x, y));
        let width = self.width;
        let height = self.height;
        for (i, block) in self
            .blocks
            .iter_mut()
            .enumerate()
            .filter(|(_i, b)| b.dir != BlockDir::Static)
        {
            if (block.x1 <= x) && (x <= block.x2) && (block.y1 <= y) && (y <= block.y2) {
                block.drag = true;
                self.drag_target = Some(i);
                return;
            }
        }

        // Look for less than perfect hits to attempt touch support
        for (i, block) in self
            .blocks
            .iter_mut()
            .enumerate()
            .filter(|(_i, b)| b.dir != BlockDir::Static)
        {
            let (sx1, sy1) = xy_to_sxy(width, height, block.x1, block.y1);
            let (sx2, sy2) = xy_to_sxy(width, height, block.x2 + 1, block.y2 + 1);
            if (sx1 - 10 <= mx) && (mx <= sx2 + 10) && (sy1 - 10 <= my) && (my <= sy2 + 10) {
                block.drag = true;
                self.drag_target = Some(i);
                return;
            }
        }
    }

    fn end_drag(&mut self) {
        for (i, block) in self.blocks.iter_mut().enumerate() {
            if block.drag {
                if self.drag_target.is_some() {
                    self.moves.push(Move {
                        block: i,
                        x: block.x1,
                        y: block.y1,
                    })
                }
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
        self.drag_target = None;
        self.drag_origin = None;
    }
}

impl App<AssetId> for Level {
    /// Invoked when the application is first started, default behavior is a no-op.
    fn start(&mut self, _ctx: &mut AppContext<AssetId>) {}

    /// Advances the app state by a given amount of `seconds` (usually a fraction of a second).
    fn advance(&mut self, _seconds: f64, ctx: &mut AppContext<AssetId>) {
        let dims = ctx.dims();
        self.width = dims.0 as usize;
        self.height = dims.1 as usize;
        let mut mouse_pos = ctx.cursor();
        mouse_pos.1 = 600. - mouse_pos.1;
        //println!("mouse pos: {} {}", mouse_pos.0, mouse_pos.1);
        // TODO: Stop using usize to for mouse_pos...
        let margin_x = (800 - TILE_WIDTH * TILES_WIDE) / 2;
        let margin_y = (600 - TILE_HEIGHT * TILES_HIGH) / 2;
        if mouse_pos.0 > margin_x as f64 && mouse_pos.1 > margin_y as f64 {
            self.mouse_pos = (mouse_pos.0 as usize, mouse_pos.1 as usize);
        }
        if self.drag_origin.is_some() {
            // Convert mouse pos to block pos, subtract from original pos to get delta pos.
            let (mx, my) = self.mouse_pos;
            self.drag_to(mx, my);
        }
    }

    /// Invoked when a key or mouse button is pressed down.
    fn key_down(&mut self, key: KeyCode, _ctx: &mut AppContext<AssetId>) {
        if key == KeyCode::MouseLeft {
            let (mx, my) = self.mouse_pos;
            let (gx, gy) = self.sxy_to_xy(mx, my);
            println!("mouse: {} {}; grid: {} {}", mx, my, gx, gy);
        }
        if key == KeyCode::MouseLeft && self.drag_target.is_none() {
            let (mx, my) = self.mouse_pos;
            println!("mouse down: {} {}", mx, my);
            self.begin_drag(mx, my);
        }
        if key == KeyCode::U {
            let move_to_undo = self.moves.pop();
            if move_to_undo.is_some() {
                let undo = move_to_undo.unwrap();
                self.blocks[undo.block].target_x = undo.x;
                self.blocks[undo.block].target_y = undo.y;
                self.blocks[undo.block].drag = true;
                self.end_drag();
            }
        }
    }

    /// Invoked when a key or mouse button is released, default behavior is a no-op.
    fn key_up(&mut self, key: KeyCode, _ctx: &mut AppContext<AssetId>) {
        if key == KeyCode::MouseLeft && self.drag_target.is_some() {
            println!("mouse up");
            self.end_drag();
        }
    }

    /// Render the app in its current state.
    fn render(&mut self, renderer: &mut Renderer<AssetId>, _ctx: &AppContext<AssetId>) {
        let mut renderer = renderer.sprite_mode();
        for block in { self.blocks.iter_mut().rev() } {
            let (mut x, mut y) = (block.x1, block.y1);
            if block.drag && block.target_x != 0 && block.target_y != 0 {
                x = block.target_x;
                y = block.target_y;
            }
            let (sx, sy) = xy_to_sxy(self.width, self.height, x, y);
            let width = (1 + block.x2 - block.x1) * TILE_WIDTH / 2;
            let height = (1 + block.y2 - block.y1) * TILE_HEIGHT / 2;
            renderer.draw(
                &Affine::translate((sx + width) as f64, 600. - (sy + height) as f64),
                block.sprite,
            );
        }
    }
}

impl App<AssetId> for LevelSet {
    fn key_down(&mut self, key: KeyCode, ctx: &mut AppContext<AssetId>) {
        if key == KeyCode::N {
            self.next();
        }
        if key == KeyCode::P {
            self.previous();
        }
        if key == KeyCode::R {
            self.current().reset();
        }
        self.current().key_down(key, ctx);
    }

    fn key_up(&mut self, key: KeyCode, ctx: &mut AppContext<AssetId>) {
        self.current().key_up(key, ctx);
    }

    fn advance(&mut self, seconds: f64, ctx: &mut AppContext<AssetId>) {
        self.current().advance(seconds, ctx);
        if self.current().solved {
            self.current().reset();
            self.next();
        }
    }

    fn render(&mut self, renderer: &mut Renderer<AssetId>, ctx: &AppContext<AssetId>) {
        renderer.clear(BLACK);
        self.current().render(renderer, ctx);
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    sdl2::hint::set("SDL_RENDER_DRIVER", "opengles2");
    let levels = LevelSet::load();
    let info = AppInfo::with_max_dims(800., 600.)
        .min_dims(500., 500.)
        .tile_width(50)
        .title("Unblock Me!");
    gate::run(info, levels);
}
