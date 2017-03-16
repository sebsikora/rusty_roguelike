extern crate tcod;
extern crate rand;
extern crate hsl;

use std::cmp;

use tcod::console::*;
use tcod::colors::*;
use tcod::map::{Map as FovMap, FovAlgorithm};

use rand::*;

// Define some constants.
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const LIMIT_FPS: i32 = 20;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const COLOR_WALL: (f64, f64, f64) = (0.509, 0.431, 0.196);
const COLOR_GROUND: (f64, f64, f64) = (0.0, 0.6, 0.0);

const COLOR_PLAYER: (f64, f64, f64) = (1.0, 1.0, 0.0);
const COLOR_CAT_BUDDY: (f64, f64, f64) = (1.0, 0.502, 0.0);

const ROOM_MAX_SIZE: i32 = 15;
const ROOM_MIN_SIZE: i32 = 10;
const MAX_ROOMS: i32 = 30;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 0;        // 0 = unlimited.
const IN_FOV_LIGHTNESS_MODIFIER: f64 = 0.2;
const AMBIENT_ILLUMINATION: (f64, f64, f64) = (0.008, 0.008, 0.008);
const ILLUMINATION_MODULATION: f64 = 0.5;

// Define a 'Map' datatype, in the form of a Vector of Vectors of Tiles.
type Map = Vec<Vec<Tile>>;
type LightField = Vec<Vec<(f64, f64, f64)>>;

// Define a 'Tile' object.
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    block_sight: bool,
    explored: bool,
    color: (f64, f64, f64),
}

// Define Tile object methods.
impl Tile {
    pub fn empty() -> Self {
        Tile{blocked: false, block_sight: false, explored: false, color: COLOR_GROUND}
    }
    
    pub fn wall() -> Self {
        Tile{blocked: true, block_sight: true, explored: false,  color: COLOR_WALL}
    }
}


// Define a 'Rect' rectangular room object.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

// Rect object methods.
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


// Define our 'Object' object, which will be used to represent all in-world objects.
#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: (f64, f64, f64),
    light_source: (bool, (f64, f64, f64)),
}

// Here we define the 'Object' object methods.
impl Object {
    pub fn new(x: i32, y: i32, char: char, color: (f64, f64, f64), light_source: (bool, (f64, f64, f64))) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
            light_source: light_source,
        }
    }
    
    // Move object by dx, dy.
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map) {
        if !map[(self.x + dx) as usize][(self.y + dy) as usize].blocked {
            self.x += dx;
            self.y += dy;
        }
    }
    
    // Draw object in chosen terminal.
    pub fn draw(&self, con: &mut Console, output_color: Color) {
        con.set_default_foreground(output_color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
    
    // Erase object in chosen terminal.
    pub fn clear(&self, con: &mut Console) {
        con.put_char(self.x, self.y, ' ', BackgroundFlag::None);
    }
}


// Room creation function.
fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize].block_sight = false;
            map[x as usize][y as usize].blocked = false;
            map[x as usize][y as usize].color = COLOR_GROUND;
        }
    }
}

// 'Horizontal' tunnel creation function.
fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize].block_sight = false;
        map[x as usize][y as usize].blocked = false;
        map[x as usize][y as usize].color = COLOR_GROUND;
    }
}

// 'Vertical' tunnel creation function.
fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize].block_sight = false;
        map[x as usize][y as usize].blocked = false;
        map[x as usize][y as usize].color = COLOR_GROUND;
    }
}

// Map creation function.
//
// Still to implement:
//     - Need to detect when though we have not yet generated MAX_ROOMS rooms,
//       there is insufficient empty space to create another room of at least 
//       ROOM_MIN_SIZE^2 dimensions.

fn make_map() -> (Map, (i32, i32)) {
    // Make an empty map from empty tiles.
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    let mut starting_position = (0, 0);
        
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


fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &mut Map, fov_map: &mut FovMap, fov_recompute: bool, light_field: &mut LightField) {
    use tcod::colors::*;
    
    if fov_recompute {
        // Recompte FOV if needed (ie - player moves).
        let player = &objects[0];
        fov_map.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
        
        // Update illumination map.
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                light_field[x as usize][y as usize].0 = 0.0;
                light_field[x as usize][y as usize].1 = 0.0;
                light_field[x as usize][y as usize].2 = 0.0;
            }
        }
        for object in objects {
            if object.light_source.0 {
                // Here calculate the light field generated by the object light source.
                //
                // ...then, composit this into the illumination map, light_field.
                //
                // When we draw tiles below, we will get ther luminance value from the
                // illumination map.
                let olf = compute_lightfield(map, object);
                let object_light_field = olf.0;
                let map_start_offset = olf.1;
                let map_end_offset = olf.2;
                
                'y_loop: for y in (map_start_offset.1)..(map_end_offset.1) {
                            if (y < 0) || (y > (MAP_HEIGHT - 1)) {
                                continue 'y_loop;
                            }
                            'x_loop: for x in (map_start_offset.0)..(map_end_offset.0) {
                                        if (x < 0) || (x > (MAP_WIDTH - 1)) {
                                            continue 'x_loop;
                                        }
                                        
                                        light_field[x as usize][y as usize].0 = light_field[x as usize][y as usize].0 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].0;
                                        light_field[x as usize][y as usize].1 = light_field[x as usize][y as usize].1 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].1;
                                        light_field[x as usize][y as usize].2 = light_field[x as usize][y as usize].2 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].2;
                                     }
                            }
            }
        }
        
        // Draw all world tiles.
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                let visible = fov_map.is_in_fov(x, y);
                
                // If we borrow map as mutable first, then we cannot borrow it as unmutable
                // afterwards to create the mutable wall_color.
                let mut wall_color = map[x as usize][y as usize].color;
                let explored = &mut map[x as usize][y as usize].explored;
                
                let mut display_color: (i32, i32, i32) = (0, 0, 0);
                
                if visible {
                    let float_r_channel_output: f64 = wall_color.0 * ((light_field[x as usize][y as usize].0) + AMBIENT_ILLUMINATION.0);
                    let float_g_channel_output: f64 = wall_color.1 * ((light_field[x as usize][y as usize].1) + AMBIENT_ILLUMINATION.1);
                    let float_b_channel_output: f64 = wall_color.2 * ((light_field[x as usize][y as usize].2) + AMBIENT_ILLUMINATION.2);
                    
                    // -- Code to turn linearised total brightness into a log brightness --
                    let a = -1.01179495;
                    let b = -4.47099458;
                    let c = 1.01214152;
                    let mut corrected_r_channel_output: f64 = (((b * float_r_channel_output).exp()) * a) + c;
                    let mut corrected_g_channel_output: f64 = (((b * float_g_channel_output).exp()) * a) + c;
                    let mut corrected_b_channel_output: f64 = (((b * float_b_channel_output).exp()) * a) + c;
                    if corrected_r_channel_output > 1.0 {
                        corrected_r_channel_output = 1.0;
                    }
                    if corrected_g_channel_output > 1.0 {
                        corrected_g_channel_output = 1.0;
                    }
                    if corrected_b_channel_output > 1.0 {
                        corrected_b_channel_output = 1.0;
                    }
                    display_color.0 = (corrected_r_channel_output * 255.0) as i32;
                    display_color.1 = (corrected_g_channel_output * 255.0) as i32;
                    display_color.2 = (corrected_b_channel_output * 255.0) as i32;
                    // --------------------------------------------------------------------
                    
                    *explored = true;
                    
                } else {
                    
                    let float_r_channel_output: f64 = wall_color.0 * AMBIENT_ILLUMINATION.0;
                    let float_g_channel_output: f64 = wall_color.1 * AMBIENT_ILLUMINATION.1;
                    let float_b_channel_output: f64 = wall_color.2 * AMBIENT_ILLUMINATION.2;
                    
                    // -- Code to turn linearised total brightness into a log brightness --
                    let a = -1.01179495;
                    let b = -4.47099458;
                    let c = 1.01214152;
                    let mut corrected_r_channel_output: f64 = (((b * float_r_channel_output).exp()) * a) + c;
                    let mut corrected_g_channel_output: f64 = (((b * float_g_channel_output).exp()) * a) + c;
                    let mut corrected_b_channel_output: f64 = (((b * float_b_channel_output).exp()) * a) + c;
                    if corrected_r_channel_output > 1.0 {
                        corrected_r_channel_output = 1.0;
                    }
                    if corrected_g_channel_output > 1.0 {
                        corrected_g_channel_output = 1.0;
                    }
                    if corrected_b_channel_output > 1.0 {
                        corrected_b_channel_output = 1.0;
                    }
                    display_color.0 = (corrected_r_channel_output * 255.0) as i32;
                    display_color.1 = (corrected_g_channel_output * 255.0) as i32;
                    display_color.2 = (corrected_b_channel_output * 255.0) as i32;
                    // --------------------------------------------------------------------
                }
                
                if *explored {
                    con.set_char_background(x, y, Color { r: (display_color.0 as u8), g: (display_color.1 as u8), b: (display_color.2 as u8) }, BackgroundFlag::Set);
                }
            }
        }
    }
    
    // Draw all world objects.
    //
    // NOTE - Once objects gain the ability to move, we will need to recompute the FOV whenever an object that
    //        is currently in the FOV moves...
    //
    for object in objects {
        let visible = fov_map.is_in_fov(object.x, object.y);
        let mut display_color: (i32, i32, i32) = (0, 0, 0);
        
        if visible {
            let float_r_channel_output: f64 = ((object.color).0) * ((light_field[object.x as usize][object.y as usize].0) + AMBIENT_ILLUMINATION.0);
            let float_g_channel_output: f64 = ((object.color).1) * ((light_field[object.x as usize][object.y as usize].1) + AMBIENT_ILLUMINATION.1);
            let float_b_channel_output: f64 = ((object.color).2) * ((light_field[object.x as usize][object.y as usize].2) + AMBIENT_ILLUMINATION.2);
            
            // -- Code to turn linearised total brightness into a log brightness --
            let a = -1.01179495;
            let b = -4.47099458;
            let c = 1.01214152;
            let mut corrected_r_channel_output: f64 = (((b * float_r_channel_output).exp()) * a) + c;
            let mut corrected_g_channel_output: f64 = (((b * float_g_channel_output).exp()) * a) + c;
            let mut corrected_b_channel_output: f64 = (((b * float_b_channel_output).exp()) * a) + c;
            if corrected_r_channel_output > 1.0 {
                corrected_r_channel_output = 1.0;
            }
            if corrected_g_channel_output > 1.0 {
                corrected_g_channel_output = 1.0;
            }
            if corrected_b_channel_output > 1.0 {
                corrected_b_channel_output = 1.0;
            }
            display_color.0 = (corrected_r_channel_output * 255.0) as i32;
            display_color.1 = (corrected_g_channel_output * 255.0) as i32;
            display_color.2 = (corrected_b_channel_output * 255.0) as i32;
            // --------------------------------------------------------------------
            object.draw(con, Color { r: (display_color.0 as u8), g: (display_color.1 as u8), b: (display_color.2 as u8) });
        }
    }
    
    // Blit the composition terminal contents into the root terminal.
    blit(con, (0,0), (SCREEN_WIDTH, SCREEN_HEIGHT), root, (0,0), 1.0, 1.0);
}


fn compute_lightfield(map: &mut Map, object: &Object) -> (LightField, (i32, i32), (i32, i32)) {
    let mut total_ray_count = 0;
    
    let float_light_r_intensity: f64 = (object.light_source.1).0;
    let float_light_g_intensity: f64 = (object.light_source.1).1;
    let float_light_b_intensity: f64 = (object.light_source.1).2;
    
    let mut max_intensity: f64 = 0.0;
    if (float_light_r_intensity >= float_light_g_intensity) && (float_light_r_intensity >= float_light_b_intensity) {
        max_intensity = float_light_r_intensity;
    }
    if (float_light_g_intensity >= float_light_r_intensity) && (float_light_g_intensity >= float_light_b_intensity) {
        max_intensity = float_light_g_intensity;
    }
    if (float_light_b_intensity >= float_light_r_intensity) && (float_light_b_intensity >= float_light_g_intensity) {
        max_intensity = float_light_b_intensity;
    }
    
    let float_light_radius: f64 = (max_intensity / 0.0039215).sqrt();
    let int_light_radius: i32 = (float_light_radius.round() as i32);
    //println!("Int light radius {}", int_light_radius);
    
    let light_field_dimensions: (i32, i32) = ((2 * int_light_radius) + 1, (2 * int_light_radius) + 1);
    let mut light_field: LightField = vec![vec![(0.0, 0.0, 0.0); light_field_dimensions.0 as usize]; light_field_dimensions.1 as usize];
    
    let map_light_coords: (i32, i32) = (object.x, object.y);
    //println!("Light coords {} {}", object.x, object.y);
    
    let map_offset_start: (i32, i32) = ((map_light_coords.0 - int_light_radius), (map_light_coords.1 - int_light_radius));
    let map_offset_end: (i32, i32) = ((map_light_coords.0 + int_light_radius), (map_light_coords.1 + int_light_radius));
    //println!("Map start offsets {} {}", map_offset_start.0, map_offset_start.1);
    //println!("Map end offsets {} {}", map_offset_end.0, map_offset_end.1);
    
    let field_light_coords: (f64, f64) = (((map_light_coords.0 as f64) + 0.5) - (map_offset_start.0 as f64), ((map_light_coords.1 as f64) + 0.5) - (map_offset_start.1 as f64));
    //println!("Field light coords {} {}", field_light_coords.0, field_light_coords.1);
    
    'target_y: for map_target_y_coord in (map_offset_start.1)..(map_offset_end.1) {
        'target_x: for map_target_x_coord in (map_offset_start.0)..(map_offset_end.0) {
            total_ray_count = total_ray_count + 1;
            //println!("---------------------------------------------------------");
            
            let field_target_coords: (f64, f64) = (((map_target_x_coord as f64) + 0.5) - (map_offset_start.0 as f64), ((map_target_y_coord as f64) + 0.5) - (map_offset_start.1 as f64));
            //println!("Field target coords {} {}", field_target_coords.0, field_target_coords.1);
            
            let field_light_target_dist_comps: (f64, f64) = ((field_target_coords.0 - field_light_coords.0), (field_target_coords.1 - field_light_coords.1));
            let field_light_target_distance: f64 = ((field_light_target_dist_comps.0).powi(2) + (field_light_target_dist_comps.1).powi(2)).sqrt();
            //println!("Field light -> target distance {} {} -> {}", field_light_target_dist_comps.0, field_light_target_dist_comps.1, field_light_target_distance);
            
            let mut field_ray_coords: (f64, f64) = (field_light_coords.0, field_light_coords.1);
            let mut field_ray_brightness: (f64, f64, f64) = (float_light_r_intensity, float_light_g_intensity, float_light_b_intensity);
            
            let field_dist_step: f64 = 0.1;
            let field_dist_increments: f64 = field_light_target_distance / field_dist_step;
            
            let field_dist_step_comps: (f64, f64) = ((field_light_target_dist_comps.0 / field_dist_increments), (field_light_target_dist_comps.1 / field_dist_increments));
            //println!("-> Step {} --> {} {}", field_dist_step, field_dist_step_comps.0, field_dist_step_comps.1);
            //println!("-> Increments {}", field_dist_increments);
            
            let mut field_travelled_dist_this_target: (f64, (f64, f64)) = (0.0, (0.0, 0.0));
            
            'ray: for mut increment in 0..(field_dist_increments as i32) {
                
                let map_check_coords: (i32, i32) = (((field_ray_coords.0).trunc() as i32) + map_offset_start.0, ((field_ray_coords.1).trunc() as i32) + map_offset_start.1);
                if (map_check_coords.0 < 0) || (map_check_coords.0 > (MAP_WIDTH - 1)) || (map_check_coords.1 < 0) || (map_check_coords.1 > (MAP_HEIGHT - 1)) {
                    continue 'target_x;
                }
                
                let field_write_coords: (i32, i32) = ((field_ray_coords.0).trunc() as i32, (field_ray_coords.1).trunc() as i32);
                let ray_r_brightness: f64 = field_ray_brightness.0;
                let ray_g_brightness: f64 = field_ray_brightness.1;
                let ray_b_brightness: f64 = field_ray_brightness.2;
                
                if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 < ray_r_brightness {
                    light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 = ray_r_brightness;
                }
                if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 < ray_g_brightness {
                    light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 = ray_g_brightness;
                }
                if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 < ray_b_brightness {
                    light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 = ray_b_brightness;
                }
                
                if map[map_check_coords.0 as usize][map_check_coords.1 as usize].block_sight {
                    continue 'target_x;
                }
                
                field_ray_coords.0 += field_dist_step_comps.0;
                field_ray_coords.1 += field_dist_step_comps.1;
                
                field_travelled_dist_this_target.0 += field_dist_step;
                (field_travelled_dist_this_target.1).0 += field_dist_step_comps.0;
                (field_travelled_dist_this_target.1).1 += field_dist_step_comps.1;
                
                // Reduce light intensity here...
                let mut modulation_distance = (field_travelled_dist_this_target.0 * ILLUMINATION_MODULATION);
                if modulation_distance < 1.0 {
                    modulation_distance = 1.0;
                }
                
                field_ray_brightness.0 = (float_light_r_intensity / ((modulation_distance).powf(2.0)));
                if field_ray_brightness.0 > float_light_r_intensity {
                    field_ray_brightness.0 = float_light_r_intensity;
                }
                field_ray_brightness.1 = (float_light_g_intensity / ((modulation_distance).powf(2.0)));
                if field_ray_brightness.1 > float_light_g_intensity {
                    field_ray_brightness.1 = float_light_g_intensity;
                }
                field_ray_brightness.2 = (float_light_b_intensity / ((modulation_distance).powf(2.0)));
                if field_ray_brightness.2 > float_light_b_intensity {
                    field_ray_brightness.2 = float_light_b_intensity;
                }
                // 
            }
            //println!("---> Travelled {} --> {} {}", field_travelled_dist_this_target.0, (field_travelled_dist_this_target.1).0, (field_travelled_dist_this_target.1).1);
            //println!("--------> Ray final field coords {} {}", field_ray_coords.0, field_ray_coords.1);
        }
    }
    //println!("Total ray count {}", total_ray_count);
    
    (light_field, map_offset_start, map_offset_end)
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
    tcod::system::set_fps(LIMIT_FPS);
    
    // Create our 'composition' terminal, off-screen, in which we will compose each frame.
    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);
    
    // Instantiate a map.
    let (mut map, (player_x, player_y)) = make_map();
    
    // Instantiate 'player' and 'npc' objects and put them in the objects list.
    let player = Object::new(player_x, player_y, '@', COLOR_PLAYER, (true, (1.0, 0.0, 1.0)));
    let light_bulb = Object::new(player_x+3, player_y+3, '*', COLOR_PLAYER, (true, (0.1, 0.1, 0.1)));
    let light_bulb2 = Object::new(player_x-3, player_y-3, '*', COLOR_PLAYER, (true, (0.1, 0.1, 0.1)));
    let npc = Object::new(SCREEN_WIDTH / 2 - 5, SCREEN_HEIGHT / 2, '@', COLOR_CAT_BUDDY, (false, (0.0, 0.0, 0.0)));
    
    let mut objects = [player, npc, light_bulb, light_bulb2];
    
    // Setup field of view map.
    let mut fov_map = FovMap::new(MAP_WIDTH, MAP_HEIGHT);
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            fov_map.set(x, y, 
                        !map[x as usize][y as usize].block_sight,
                        !map[x as usize][y as usize].blocked);
        }
    }
    
    // Set a ficticious previous player position to make sure that fov is calculated
    // on first pass of game loop.
    let mut previous_player_position = (-1, -1);
    
    // Generate master illumination map.
    //
    // This is a vector field of i32 illumination values. These are zeroed at the start of each
    // FOV update, and then all light-sources (including the ambient illumination) are summed into
    // it. Tiles and Objects are drawn with their 'lightness' value scaled according to this value
    // at their position. The values are re-scaled from the native linear 0 -> 9999 to
    // log 0.0 -> 1.0.
    
    let mut light_field: LightField = vec![vec![(0.0, 0.0, 0.0); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    // Main world loop.
    while !root.window_closed() {
        // Set flag to recompute fov is player position has changed.
        let fov_recompute = previous_player_position != (objects[0].x, objects[0].y);
        // Draw all objects in objects list into composition terminal.
        render_all(&mut root, &mut con, &objects, &mut map, &mut fov_map, fov_recompute, &mut light_field);
        
        // Display the contents of the root terminal.
        root.flush();
        
        // Clear all objects from composition terminal.
        for object in &objects {
            object.clear(&mut con);
        }
        
        // Not sure why we need to do this, as we can pass the mutable objects[0] directly
        // within the handle_keys() function arguments (as shown in the commented version below).
        // Readability??
        let player = &mut objects[0];
        
        // Prior to handling keystrokes (where player position may be changed)
        // we grab the old player position.
        previous_player_position = (player.x, player.y);
        
        //let exit = handle_keys(&mut root, &mut objects[0]);
        let exit = handle_keys(&mut root, player, &map);
        
        if exit {
            break
        }
    }
    
}
