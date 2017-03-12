extern crate tcod;
extern crate rand;


use std::cmp;

use tcod::console::*;
use tcod::colors::{self, Color};

use rand::*;


// Define some constants.
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const LIMIT_FPS: i32 = 20;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 100, b: 0 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 150, b: 50 };

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;


type Map = Vec<Vec<Tile>>;


#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    block_sight: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile{blocked: false, block_sight: false}
    }
    
    pub fn wall() -> Self {
        Tile{blocked: true, block_sight: true}
    }
}


#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x1: x, y1: y, x2: x + w, y2: y + h }
    }
    
    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        
        (center_x, center_y)
    }
    
    pub fn intersects_with(&self, other: &Rect) -> bool {
        // Returns true if this rectangle intersects with target.
        (self.x1 <= other.x2) && (self.x2 >= other.x1) && (self.y1 <= self.y2) && (self.y2 >= other.y1)
    }
}


// Define our 'Object' structure, which will be used to represent all in-world objects.
#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
}

// Here we define the 'Object' methods.
impl Object {
    pub fn new(x: i32, y: i32, char: char, color: Color) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
        }
    }
    
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map) {
        if !map[(self.x + dx) as usize][(self.y + dy) as usize].blocked {
            self.x += dx;
            self.y += dy;
        }
    }
    
    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
    
    pub fn clear(&self, con: &mut Console) {
        con.put_char(self.x, self.y, ' ', BackgroundFlag::None);
    }
}


fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize].block_sight = false;
            map[x as usize][y as usize].blocked = false;
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize].block_sight = false;
        map[x as usize][y as usize].blocked = false;
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize].block_sight = false;
        map[x as usize][y as usize].blocked = false;
    }
}

fn make_map() -> (Map, (i32, i32)) {
    // Make an empty map from empty tiles.
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    let mut starting_position = (0, 0);
    
    // Add a couple of test pillars.
    //map[30][22] = Tile::wall();
    //map[50][22] = Tile::wall();
    
    // Create a couple of test rectangular rooms.
    //~let room1 = Rect::new(20, 15, 10, 15);
    //~let room2 = Rect::new(50, 15, 10, 15);
    //~create_room(room1, &mut map);
    //~create_room(room2, &mut map);
    //~create_h_tunnel(25, 55, 23, &mut map);
    
    let mut rooms = vec![];
    
    for _ in 0..MAX_ROOMS {
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);
        
        let new_room = Rect::new(x, y, w, h);
        
        let failed = rooms.iter().any(|other_room| new_room.intersects_with(other_room));
        
        if !failed {
            // In this case, there are no intersections between the proposed
            // new room and any existing rooms, so we create it.
            create_room(new_room, &mut map);
            
            // Get the centre co-ordinates of the room.
            let (new_x, new_y) = new_room.center();
            
            if rooms.is_empty() {
                // Then this is the first room, so we set the player start
                // co-ordinates appropriately.
                starting_position = (new_x, new_y);
            } else {
                // All other rooms after the first.
                // Connect it to the previous room with a tunnel.
                
                // Get center co-ordinates of previous room.
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();
                
                // First, pick a random boolean.
                if rand::random() {
                    // Tunnel horizontally, then vertically.
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }
            
            rooms.push(new_room);
        }
    }
    
    (map, starting_position)
}


// Keystroke handler.
fn handle_keys(root: &mut Root, player: &mut Object, map: &Map) -> bool {
    // Import necessary libraries for key handling.
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    
    // Catch any keystroke.
    let key = root.wait_for_keypress(true);
    
    // Filter keystroke.
    match key {
        Key { code: Enter, alt: true, .. } => {
            // Toggle full-screen mode.
            let fullscreen = root.is_fullscreen();
            root.set_fullscreen(!fullscreen);
        }
        Key { code: Escape, .. } => return true,    // Exit.
        
        // Movement keys.
        Key { code: Up, .. } => player.move_by(0, -1, map),
        Key { code: Down, .. } => player.move_by(0, 1, map),
        Key { code: Left, .. } => player.move_by(-1, 0, map),
        Key { code: Right, .. } => player.move_by(1, 0, map),
        
        _ => {},
        
    }
    false
}


fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &Map) {
    // Draw all world tiles.
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let wall = map[x as usize][y as usize].block_sight;
            if wall {
                con.set_char_background(x, y, COLOR_DARK_WALL, BackgroundFlag::Set);
            } else {
                con.set_char_background(x, y, COLOR_DARK_GROUND, BackgroundFlag::Set);
            }
        }
    }
    
    // Draw all world objects.
    for object in objects {
        object.draw(con);
    }
    
    // Blit the composition terminal contents into the root terminal.
    blit(con, (0,0), (SCREEN_WIDTH, SCREEN_HEIGHT), root, (0,0), 1.0, 1.0);
}


fn main() {
    // Create our 'root' terminal window, in which we will display completed frames.
    let mut root = Root::initializer()
        // Configuration.
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libcod tutorial")
        .init();
    
    // Create our 'composition' terminal, off-screen, in which we will compose each frame.
    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);
    
    tcod::system::set_fps(LIMIT_FPS);
    
    // Instantiate a map.
    let (map, (player_x, player_y)) = make_map();
    
    // Instantiate 'player' and 'npc' objects and put them in the objects list.
    let player = Object::new(player_x, player_y, '@', colors::WHITE);
    let npc = Object::new(SCREEN_WIDTH / 2 - 5, SCREEN_HEIGHT / 2, '@', colors::YELLOW);
    
    let mut objects = [player, npc];
    
    // Main world loop.
    while !root.window_closed() {
        // Draw all objects in objects list into composition terminal.
        render_all(&mut root, &mut con, &objects, &map);
        
        // Display the contents of the root terminal.
        root.flush();
        
        // Clear all objects from composition terminal.
        for object in &objects {
            object.clear(&mut con);
        }
        
        // Not sure why we need to do this, as we can pass the mutable objects[0] directly
        // within the handle_keys() function arguments (as shown in the commented version below).
        let player = &mut objects[0];
        //let exit = handle_keys(&mut root, &mut objects[0]);
        let exit = handle_keys(&mut root, player, &map);
        
        if exit {
            break
        }
    }
    
}
