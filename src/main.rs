/*
6x6 board
10x10 tiles?
Read data file

Level 1:
&&&&&&&&
&---**|&
&**|**|&
&==|**|^
&|*|*--&
&|***|*&
&---*|*&
&&&&&&&&

Display board
Rounded rectangles
Brown w/ black border
Red w/ black border

Drag pieces
  left/right only
  up/down only

Detect if red piece reaches exit
Move on to the next level

Add undo: Build stack of moves
Add reset

Move block by block or fluid?
*/
use glutin_window::GlutinWindow;
use graphics::{self, Context, Graphics};
use opengl_graphics::{GlGraphics, OpenGL};
use piston::event_loop::{EventLoop, EventSettings, Events};
use piston::input::GenericEvent;
use piston::input::{
    keyboard::{Key, ModifierKey},
    mouse::MouseButton,
    Button, MouseCursorEvent, MouseScrollEvent, PressEvent, ReleaseEvent, RenderArgs, RenderEvent,
};
use piston::window::WindowSettings;
use std::convert::TryInto;

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

const TILES_WIDE: usize = 8;
const TILES_HIGH: usize = 8;
const TILE_WIDTH: usize = 50;
const TILE_HEIGHT: usize = 50;

const FLOOR: u8 = b'*';
const WALL: u8 = b'&';
const LEFTRIGHT: u8 = b'-';
const UPDOWN: u8 = b'|';
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
    Other,
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
    (x * TILE_WIDTH, y * TILE_HEIGHT)
}

fn sxy_to_xy(sx: usize, sy: usize) -> (usize, usize) {
    (sx / TILE_WIDTH, sy / TILE_HEIGHT)
}

fn color(block: &Block) -> [f32; 4] {
    match block.r#type {
        BlockType::Player => RED,
        BlockType::Wall => WHITE,
        BlockType::Exit => YELLOW,
        BlockType::Other => match block.dir {
            BlockDir::LeftRight => BLUE,
            BlockDir::UpDown => GREEN,
            _ => panic!("No Static + Other blocks exist"),
        },
    }
}

struct Level {
    data: [u8; TILES_WIDE * TILES_HIGH],
    blocks: Vec<Block>,
    // UI state
    mouse_down: bool,
    mouse_pos: (usize, usize),
    drag_origin: Option<(usize, usize)>,
    drag_target: Option<usize>,
}

impl Level {
    fn new() -> Level {
        let mut level = Level {
            data: [FLOOR; TILES_WIDE * TILES_HIGH],
            blocks: Vec::new(),
            mouse_down: false,
            mouse_pos: (0, 0),
            drag_origin: None,
            drag_target: None,
        };
        level.parse();
        level
    }

    fn parse(&mut self) {
        let level1 = "\
                      &&&&&&&&\
                      &---**|&\
                      &**|**|&\
                      &==|**|^\
                      &|*|*--&\
                      &|***|*&\
                      &---*|*&\
                      &&&&&&&&\
                      ";
        let mut pos = 0;
        level1
            .replace(" ", "")
            .replace("\n", "")
            .bytes()
            .for_each(|b| {
                self.data[pos] = b;
                pos += 1;
            });
        let mut id = 1;
        assert!(self.data.len() == 64, "Too many chars: {}", self.data.len());
        for pos in 0..self.data.len() {
            let (x, y) = pos_to_xy(pos);
            match self.data[pos] {
                WALL => {
                    self.blocks
                        .push(Block::new(BlockType::Wall, BlockDir::Static, x, y, x, y));
                }
                LEFTRIGHT => {
                    let mut pos2 = pos.clone();
                    while self.data[pos2] == LEFTRIGHT {
                        self.data[pos2] = id;
                        pos2 += 1;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 1);
                    self.blocks.push(Block::new(
                        BlockType::Other,
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
                UPDOWN => {
                    let mut pos2 = pos;
                    while self.data[pos2] == UPDOWN {
                        self.data[pos2] = id;
                        pos2 += TILES_WIDE;
                    }
                    id += 1;
                    let (x2, y2) = pos_to_xy(pos2 - 8);
                    self.blocks
                        .push(Block::new(BlockType::Other, BlockDir::UpDown, x, y, x2, y2));
                }
                FLOOR => {}
                _ => {}
            };
        }
    }

    fn serialize(&self) -> [u8; 64] {
        let mut level = [b'*'; 64];
        for block in &self.blocks {
            for x in block.x1..block.x2 + 1 {
                for y in block.y1..block.y2 + 1 {
                    level[xy_to_pos(x, y)] = match block.r#type {
                        BlockType::Other => match block.dir {
                            BlockDir::LeftRight => b'-',
                            BlockDir::UpDown => b'|',
                            _ => panic!("Unexpected BlockDir during serialization!"),
                        },
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

    pub fn event<E: GenericEvent>(&mut self, e: &E) {
        if let Some(mouse_pos) = e.mouse_cursor_args() {
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
                        println!(
                            "delta: {} {}; origin: {} {}; block: {} {}",
                            dx, dy, ox, oy, bx, by
                        );
                        let mut block = &mut self.blocks[drag_target];
                        block.target_x = block.x1;
                        block.target_y = block.y1;
                        let (x, y) = (block.x1, block.y1);
                        match block.dir {
                            BlockDir::LeftRight => {
                                let blocks_wide = block.x2 - block.x1;
                                // see if this is a valid move
                                for px in block.x1..if dx > 0 {
                                    block.x1 + dx as usize
                                } else {
                                    block.x1 - dx.abs() as usize
                                } + 1
                                {
                                    if self.data[xy_to_pos(px, y)] == FLOOR
                                        || self.data[xy_to_pos(px, y)] == self.data[xy_to_pos(x, y)]
                                    {
                                        if self.data[xy_to_pos(px + blocks_wide, y)] == FLOOR
                                            || self.data[xy_to_pos(px + blocks_wide, y)]
                                                == self.data[xy_to_pos(x, y)]
                                        {
                                            println!("target x: {}", px);
                                            block.target_x = px;
                                        }
                                    }
                                }
                            }
                            BlockDir::UpDown => {
                                let blocks_high = block.y2 - block.y1;
                                // see if this is a valid move
                                for py in block.y1..if dy > 0 {
                                    block.y1 + dy as usize
                                } else {
                                    block.y1 - dy.abs() as usize
                                } + 1
                                {
                                    if self.data[xy_to_pos(x, py)] == FLOOR
                                        || self.data[xy_to_pos(x, py)] == self.data[xy_to_pos(x, y)]
                                    {
                                        if self.data[xy_to_pos(x, py + blocks_high)] == FLOOR
                                            || self.data[xy_to_pos(x, py + blocks_high)]
                                                == self.data[xy_to_pos(x, y)]
                                        {
                                            println!("target y: {}", py);
                                            block.target_y = py;
                                        }
                                    }
                                }
                            }
                            _ => {
                                panic!("Not a valid direction for a draggable block: {:#?}", block)
                            }
                        }
                    }
                    None => (),
                };
            }
        }
        if let Some(args) = e.press_args() {
            match args {
                Button::Mouse(_) => {
                    let (mx, my) = self.mouse_pos;
                    let (x, y) = sxy_to_xy(mx, my);
                    self.drag_origin = Some((x, y));
                    for (i, block) in self
                        .blocks
                        .iter_mut()
                        .enumerate()
                        .filter(|(_i, b)| b.dir != BlockDir::Static)
                    {
                        if (block.x1 <= x) && (x <= block.x2) && (block.y1 <= y) && (y <= block.y2)
                        {
                            println!(
                                "drag: {} {}; {} {} ({} {})",
                                block.x1, block.y1, block.x2, block.y2, x, y
                            );
                            block.drag = true;
                            self.drag_target = Some(i);
                        }
                    }
                }
                _ => {}
            };
        }
        if let Some(args) = e.release_args() {
            match args {
                Button::Mouse(_) => {
                    self.drag_target = None;
                    self.drag_origin = None;
                    for block in self.blocks.iter_mut() {
                        if block.drag {
                            // Updaate block and data to reflect move.
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
                            block.x2 = block.x1 + width;
                            block.y2 = block.y1 + height;
                            for x in block.x1..block.x2 + 1 {
                                for y in block.y1..block.y2 + 1 {
                                    self.data[xy_to_pos(x, y)] = id;
                                }
                            }
                        }
                        block.drag = false;
                    }
                }
                _ => {}
            };
        }
    }

    pub fn draw<G: Graphics>(&mut self, c: &Context, g: &mut G) {
        for block in self.blocks.iter_mut() {
            let (mut x, mut y) = (block.x1, block.y1);
            let (width, height) = (
                (1 + block.x2 - block.x1) * TILE_WIDTH,
                (1 + block.y2 - block.y1) * TILE_HEIGHT,
            );
            if block.drag {
                x = block.target_x;
                y = block.target_y;
            }
            let (sx, sy) = xy_to_sxy(x, y);
            graphics::Rectangle::new(color(block)).draw(
                [sx as f64, sy as f64, width as f64, height as f64],
                &c.draw_state,
                c.transform,
                g,
            );
        }
    }
}

fn main() {
    let mut level = Level::new();
    let opengl = OpenGL::V3_2;
    let settings = WindowSettings::new("Unblock Me!", [512; 2])
        .graphics_api(opengl)
        .exit_on_esc(true);
    let mut window: GlutinWindow = settings.build().expect("Could not create window");
    let mut events = Events::new(EventSettings::new().lazy(true).max_fps(20).ups(10));
    let mut gl = GlGraphics::new(opengl);
    while let Some(e) = events.next(&mut window) {
        level.event(&e);
        if let Some(args) = e.render_args() {
            gl.draw(args.viewport(), |c, g| {
                use graphics::clear;
                clear([0.0; 4], g);
                level.draw(&c, g);
            });
        }
    }
    println!("{}", level.to_string_pretty());
    println!(
        "{}",
        String::from_utf8(
            level
                .data
                .to_vec()
                .iter()
                .map(|b| {
                    if *b == 1 as u8 {
                        b'1'
                    } else if *b == 2 as u8 {
                        b'2'
                    } else if *b == 3 as u8 {
                        b'3'
                    } else if *b == 4 as u8 {
                        b'4'
                    } else if *b == 5 as u8 {
                        b'5'
                    } else if *b == 6 as u8 {
                        b'6'
                    } else if *b == 7 as u8 {
                        b'7'
                    } else if *b == 8 as u8 {
                        b'8'
                    } else if *b == 9 as u8 {
                        b'9'
                    } else if *b == 10 as u8 {
                        b'A'
                    } else {
                        *b
                    }
                })
                .collect()
        )
        .expect("Unable to convert")
    );
}
