/*
Add undo: Build stack of moves
*/

use coffee::{
    graphics::{Color, Frame, Mesh, Point, Rectangle, Shape, Window, WindowSettings},
    input::{keyboard, keyboard::KeyCode, mouse, ButtonState, Event, Input, KeyboardAndMouse},
    load::Task,
    Game, Result, Timer,
};
use itertools::put_back;
use std::collections::HashSet;

const YELLOW: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 0.0,
    a: 1.0,
};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

const BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

const GREEN: Color = Color {
    r: 0.0,
    g: 1.0,
    b: 0.0,
    a: 1.0,
};

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

fn color(block: &Block) -> Color {
    match block.r#type {
        BlockType::Player => RED,
        BlockType::Wall => Color::WHITE,
        BlockType::Exit => YELLOW,
        BlockType::Other(_) => match block.dir {
            BlockDir::LeftRight => BLUE,
            BlockDir::UpDown => GREEN,
            _ => panic!("No Static + Other blocks exist"),
        },
    }
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
            width: 500,
            height: 500,
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

    fn update(&mut self, window: &Window) {
        self.width = window.width() as usize;
        self.height = window.height() as usize;
        if self.drag_origin.is_some() {
            // Convert mouse pos to block pos, subtract from original pos to get delta pos.
            let (mx, my) = self.mouse_pos;
            self.drag_to(mx, my);
        }
    }

    fn interact(&mut self, input: &mut UnblockInput, _window: &mut Window) {
        if input.is_mouse_pressed {
            let (mx, my) = self.mouse_pos;
            let (gx, gy) = self.sxy_to_xy(
                input.cursor_position().coords.x as usize,
                input.cursor_position().coords.y as usize,
            );
            println!("mouse: {} {}; grid: {} {}", mx, my, gx, gy);
            if self.drag_target.is_none() {
                let (mx, my) = self.mouse_pos;
                println!("mouse down: {} {}", mx, my);
                self.begin_drag(mx, my);
            }
        }
        let mouse_pos = input.cursor_position();
        //mouse_pos.coords.y = 500 - mouse_pos.coords.y;
        //println!("mouse pos: {} {}", mouse_pos.0, mouse_pos.1);
        // TODO: Stop using usize to for mouse_pos...
        let margin_x = (500 - TILE_WIDTH * TILES_WIDE) / 2;
        let margin_y = (500 - TILE_HEIGHT * TILES_HIGH) / 2;
        if mouse_pos.coords.x > margin_x as f32 && mouse_pos.coords.y > margin_y as f32 {
            self.mouse_pos = (mouse_pos.coords.x as usize, mouse_pos.coords.y as usize);
        }
        if input.was_key_released(KeyCode::U) {
            let move_to_undo = self.moves.pop();
            if move_to_undo.is_some() {
                let undo = move_to_undo.unwrap();
                self.blocks[undo.block].target_x = undo.x;
                self.blocks[undo.block].target_y = undo.y;
                self.blocks[undo.block].drag = true;
                self.end_drag();
            }
        }

        if !input.is_mouse_pressed && self.drag_target.is_some() {
            println!("mouse up");
            self.end_drag();
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, _timer: &Timer) {
        let mut mesh = Mesh::new();
        for block in { self.blocks.iter_mut().rev() } {
            let (mut x, mut y) = (block.x1, block.y1);
            if block.drag && block.target_x != 0 && block.target_y != 0 {
                x = block.target_x;
                y = block.target_y;
            }
            let (sx, sy) = xy_to_sxy(self.width, self.height, x, y);
            let width = (1 + block.x2 - block.x1) * TILE_WIDTH;
            let height = (1 + block.y2 - block.y1) * TILE_HEIGHT;
            mesh.fill(
                Shape::Rectangle(Rectangle {
                    x: sx as f32,
                    y: sy as f32,
                    width: width as f32,
                    height: height as f32,
                }),
                color(block),
            );
            mesh.stroke(
                Shape::Rectangle(Rectangle {
                    x: sx as f32,
                    y: sy as f32,
                    width: width as f32,
                    height: height as f32,
                }),
                Color::BLACK,
                1,
            );
        }
        mesh.draw(&mut frame.as_target());
    }
}

// Copy of KeyboardAndMouse in order to get access to mouse_pressed
struct UnblockInput {
    cursor_position: Point,
    is_cursor_taken: bool,
    is_mouse_pressed: bool,
    left_clicks: Vec<Point>,
    pressed_keys: HashSet<keyboard::KeyCode>,
    released_keys: HashSet<keyboard::KeyCode>,
}

impl UnblockInput {
    /// Returns the current cursor position.
    pub fn cursor_position(&self) -> Point {
        self.cursor_position
    }

    /// Returns true if the cursor is currently not available.
    ///
    /// This mostly happens when the cursor is currently over a
    /// [`UserInterface`].
    ///
    /// [`UserInterface`]: ../ui/trait.UserInterface.html
    pub fn is_cursor_taken(&self) -> bool {
        self.is_cursor_taken
    }

    /// Returns the positions of the mouse clicks during the last interaction.
    ///
    /// Clicks performed while the mouse cursor is not available are
    /// automatically ignored.
    pub fn left_clicks(&self) -> &[Point] {
        &self.left_clicks
    }

    /// Returns true if the given key is currently pressed.
    pub fn is_key_pressed(&self, key_code: keyboard::KeyCode) -> bool {
        self.pressed_keys.contains(&key_code)
    }

    /// Returns true if the given key was released during the last interaction.
    pub fn was_key_released(&self, key_code: keyboard::KeyCode) -> bool {
        self.released_keys.contains(&key_code)
    }
}

impl Input for UnblockInput {
    fn new() -> UnblockInput {
        UnblockInput {
            cursor_position: Point::new(0.0, 0.0),
            is_cursor_taken: false,
            is_mouse_pressed: false,
            left_clicks: Vec::new(),
            pressed_keys: HashSet::new(),
            released_keys: HashSet::new(),
        }
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::CursorMoved { x, y } => {
                    self.cursor_position = Point::new(x, y);
                }
                mouse::Event::CursorTaken => {
                    self.is_cursor_taken = true;
                }
                mouse::Event::CursorReturned => {
                    self.is_cursor_taken = false;
                }
                mouse::Event::Input {
                    button: mouse::Button::Left,
                    state,
                } => match state {
                    ButtonState::Pressed => {
                        self.is_mouse_pressed = !self.is_cursor_taken;
                    }
                    ButtonState::Released => {
                        if !self.is_cursor_taken && self.is_mouse_pressed {
                            self.left_clicks.push(self.cursor_position);
                        }

                        self.is_mouse_pressed = false;
                    }
                },
                mouse::Event::Input { .. } => {
                    // TODO: Track other buttons!
                }
                mouse::Event::CursorEntered => {
                    // TODO: Track it!
                }
                mouse::Event::CursorLeft => {
                    // TODO: Track it!
                }
                mouse::Event::WheelScrolled { .. } => {
                    // TODO: Track it!
                }
            },
            Event::Keyboard(keyboard_event) => match keyboard_event {
                keyboard::Event::Input { key_code, state } => {
                    match state {
                        ButtonState::Pressed => {
                            let _ = self.pressed_keys.insert(key_code);
                        }
                        ButtonState::Released => {
                            let _ = self.pressed_keys.remove(&key_code);
                            let _ = self.released_keys.insert(key_code);
                        }
                    };
                }
                keyboard::Event::TextEntered { .. } => {}
            },
            Event::Gamepad { .. } => {
                // Ignore gamepad events...
            }
            Event::Window(_) => {
                // Ignore window events...
            }
        }
    }

    fn clear(&mut self) {
        self.left_clicks.clear();
        self.released_keys.clear();
    }
}

impl Game for LevelSet {
    type Input = UnblockInput;
    type LoadingScreen = ();
    const TICKS_PER_SECOND: u16 = 20;

    fn load(_window: &Window) -> Task<LevelSet> {
        Task::new(|| LevelSet::load())
    }

    fn draw(&mut self, frame: &mut Frame<'_>, timer: &Timer) {
        frame.clear(Color::BLACK);
        self.current().draw(frame, timer);
    }

    fn interact(&mut self, input: &mut Self::Input, _window: &mut Window) {
        if input.was_key_released(KeyCode::N) {
            self.next();
        }
        if input.was_key_released(KeyCode::P) {
            self.previous();
        }
        if input.was_key_released(KeyCode::R) {
            self.current().reset();
        }
        self.current().interact(input, _window);
    }

    fn update(&mut self, _window: &Window) {
        self.current().update(_window);
        if self.current().solved {
            self.current().reset();
            self.next();
        }
    }
}

fn main() -> Result<()> {
    LevelSet::run(WindowSettings {
        title: String::from("Unblock Me!"),
        size: (500, 500),
        resizable: false,
        fullscreen: false,
    })
}
