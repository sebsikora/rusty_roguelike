extern crate tcod;

use tcod::console::*;
use tcod::colors::{self, Color};

// Define some constants.
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const LIMIT_FPS: i32 = 20;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 50, b: 150 };

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


fn make_map() -> Map {
    // Make an empty map from empty tiles.
    let mut map = vec![vec![Tile::empty(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    // Add a couple of test pillars.
    map[30][22] = Tile::wall();
    map[50][22] = Tile::wall();
    
    map
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
    let mut con = Offscreen::new(SCREEN_WIDTH, SCREEN_HEIGHT);
    
    tcod::system::set_fps(LIMIT_FPS);
    
    let mut player_x = SCREEN_WIDTH / 2;
    let mut player_y = SCREEN_HEIGHT / 2;
    
    // Instantiate a map.
    let map = make_map();
    
    // Instantiate 'player' and 'npc' objects and put them in the objects list.
    let player = Object::new(player_x, player_y, '@', colors::WHITE);
    let npc = Object::new(player_x - 5, player_y, '@', colors::YELLOW);
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
