extern crate tcod;
extern crate rand;

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
//const COLOR_GROUND: (f64, f64, f64) = (0.247, 0.471, 0.0039);
const COLOR_GROUND: (f64, f64, f64) = (0.1, 0.1, 0.1);

const COLOR_PLAYER: (f64, f64, f64) = (1.0, 1.0, 0.0);
const COLOR_CAT_BUDDY: (f64, f64, f64) = (1.0, 0.502, 0.0);

const ROOM_MAX_SIZE: i32 = 30;
const ROOM_MIN_SIZE: i32 = 10;
const MAX_ROOMS: i32 = 30;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 0;        // 0 = unlimited.

const AMBIENT_ILLUMINATION: (f64, f64, f64) = (0.0, 0.0, 0.0);
const MIN_NOT_VISIBLE_ILLUMINATION: (f64, f64, f64) = (0.015, 0.015, 0.015);
const RAYCAST_DISTANCE_STEP: f64 = 0.05;
const RAYCAST_FINENESS: i32 = 1;
const REFLECTION_LEVEL: i32 = 8;
const REFLECTION_BRIGHTNESS_SCALING: f64 = 1.0;
const RAYCAST_INFO: bool = false;
const TRANSPARENT_OBJECTS_REFLECTIVE: bool = false;
const TRANSPARENT_OBJECTS_REFRACT: bool = false;

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
    reflectivity: f64,
    refractive_index: f64,
}

// Define Tile object methods.
impl Tile {
    pub fn empty() -> Self {
        Tile{blocked: false, block_sight: false, explored: false, color: COLOR_GROUND, reflectivity: 0.0, refractive_index: 1.000277}
    }
    
    pub fn wall() -> Self {
        Tile{blocked: true, block_sight: true, explored: false,  color: COLOR_WALL, reflectivity: 0.25, refractive_index: 0.0}
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


struct BrightnessTables {
    brightness_tables: Vec<Vec<f64>>,
    distance_tables: Vec<Vec<f64>>,
    tangent_tables: Vec<f64>,
}


impl BrightnessTables {
    pub fn new() -> BrightnessTables {
        // Create lookup table of ray brightness as a function of distance travelled and starting collimation.
        let mut brightness_tables = vec![];
        for double_angle in 0..181 {
            let mut angle: f64 = (double_angle as f64) / 2.0;
            if angle > 90.0 {
                angle = 90.0;
            }
            let mut table = vec![];
            let power_coeff: f64 = angle / 45.0;
            for increment in 0..4001 {
                let distance: f64 = 1.0 + ((increment as f64) * 0.05);
                let brightness_scaling: f64 = 1.0 / distance.powf(power_coeff);
                table.push(brightness_scaling);
            }
            brightness_tables.push(table);
        }
        
        // Create lookup table of ray distance as a function of current brightness and starting collimation.
        let mut distance_tables = vec![];
        for double_collimation in 0..181 {
            let mut collimation: f64 = (double_collimation as f64) / 2.0;
            if collimation > 90.0 {
                collimation = 90.0;
            }
            let mut table_2 = vec![];
            let power_coeff_2: f64 = collimation / 45.0;
            for increment_2 in 0..1001 {
                let brightness: f64 = (increment_2 as f64) * 0.001;
                let distance_2: f64 = if power_coeff_2 == 0.0 {
                    4000.0
                } else {
                    (brightness / 0.002).powf(1.0/power_coeff_2)
                };
                table_2.push(distance_2);
            }
            distance_tables.push(table_2);
        }
        
        // Create a lookup table of inverse tangents of ratios between -100.0 and 100.0.
        let mut tangent_tables = vec![];
        for ratio in 0..2000001 {
            let angle_radians: f64 = (((ratio - 1000000) as f64) * 0.0001).atan();
            let angle_degrees: f64 = angle_radians.to_degrees();
            tangent_tables.push(angle_degrees);
        }
        
        BrightnessTables {
            brightness_tables: brightness_tables,
            distance_tables: distance_tables,
            tangent_tables: tangent_tables,
        }
    }
    
    // Read brightness table.
    pub fn read_brightness_table(&self, distance: &f64, collimation: &f64) -> f64 {
        let mut coll: f64 = *collimation;
        if coll > 90.0 {
            coll = 90.0;
        }
        let mut dist: f64 = *distance;
        if dist > 4000.0 {
            dist = 4000.0;
        }
        let brightness_index: i32 = ((dist - 1.0) * 20.0) as i32;
        let power_index: i32 = (coll * 2.0) as i32; 
        let brightness_scaling: f64 = self.brightness_tables[power_index as usize][brightness_index as usize];
        brightness_scaling
    }
    
    // Read distance table.
    pub fn read_distance_table(&self, brightness: &f64, collimation: &f64) -> f64 {
        let mut coll: f64 = *collimation;
        if coll > 90.0 {
            coll = 90.0;
        }
        let mut bright: f64 = *brightness;
        if bright > 1000.0 {
            bright = 1000.0;
        }
        let distance_index: i32 = (bright * 1000.0) as i32;
        let power_index: i32 = (coll * 2.0) as i32; 
        let distance: f64 = self.distance_tables[power_index as usize][distance_index as usize];
        distance
    }
    
    // Read inverse-tangent table.
    pub fn read_tangent_table(&self, ratio: &f64) -> f64 {
        let index: i32 = ((*ratio / 0.0001) as i32) + 1000000;
        let angle_degrees: f64 = self.tangent_tables[index as usize];
        angle_degrees
    }
}


// Define a child reflection object.
struct ChildReflection {
    diffuse: bool,
    x: i32,
    y: i32,
    float_offset: (f64, f64),
    face: i32,
    direction: f64,
    intensity_profile: (f64, f64, f64),
    angular_sweep: f64,
    collimation: f64,
}

// Child reflection object methods.
impl ChildReflection {
    pub fn new(diffuse: bool, x: i32, y: i32, float_offset: (f64, f64), face: i32, direction: f64, intensity_profile: (f64, f64, f64), angular_sweep: f64, collimation: f64) -> ChildReflection {
        ChildReflection {
            diffuse: diffuse,
            x: x,
            y: y,
            float_offset: float_offset,
            face: face,
            direction: direction,
            intensity_profile: intensity_profile,
            angular_sweep: angular_sweep,
            collimation: collimation,
        }
    }
}


// Define our light field object.
#[derive(Debug)]
struct LightFieldObject {
    // LightFieldObject just stores a vector of LightFields and their associated
    // bounding corners in map space.
    light_field: LightField,
    map_offset_start: (i32, i32),
    map_offset_end: (i32, i32),
}

impl LightFieldObject {
    // But - the methods are what is important.
    pub fn new() -> LightFieldObject {
        let light_field: LightField = vec![vec![(0.0, 0.0, 0.0); 1 as usize]; 1 as usize];
        let map_offset_start: (i32, i32) = (0, 0);
        let map_offset_end: (i32, i32) = (0, 0);
        LightFieldObject {
            light_field: light_field,
            map_offset_start: map_offset_start,
            map_offset_end: map_offset_end,
        }
    }
    
    // Public function that can be called on an object LightFieldObject to reacalculate it's immediate
    // LightField, and if requested, iteratively calculate any resulting reflections.
    pub fn recalculate(&mut self, map: &Map, pos_x: &i32, pos_y: &i32, float_offset: &(f64, f64), intensity_profile: &(f64, f64, f64), direction: &f64, angular_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables) {
        
        //let alpha_angle_modifier: f64 = *direction;
        let map_light_coords: (i32, i32) = (*pos_x, *pos_y);
        
        // Get beam sweep angle - either side of the beam centre (alpha angle = 0 deg).
        let beam_sweep: f64 = *angular_sweep;
        
        // Determine light-field dimensions and create empty LightField. Also, the determine_light_field_dimensions() function will
        // create a zeroed-out index field of the same dimensions.
        let light_field_result: ((i32, i32), (i32, i32), LightField) = self.determine_light_field_dimensions(&map_light_coords, intensity_profile, brightness_tables, collimation);
        self.map_offset_start = light_field_result.0;
        self.map_offset_end = light_field_result.1;
        self.light_field = light_field_result.2;
        
        // Determine light-source floating-point position offset allowing for light-source direction.
        let pos_offset = self.rotate_float_offset(float_offset, direction);
        // Determine light-source position.
        let field_light_coords: (f64, f64) = ((map_light_coords.0 as f64) + 0.5 + pos_offset.0 - (self.map_offset_start.0 as f64), (map_light_coords.1 as f64) + 0.5 + pos_offset.1 - (self.map_offset_start.1 as f64));
        
        // Identify target tiles along periphery of LightField.
        let map_targets_list: Vec<(f64, f64)> = self.locate_perimeter_targets(&self.map_offset_start, &self.map_offset_end);
        
        // First pass raycasting begins! Cast rays, modify LightField accordingly, and store any resulting candidate child reflections.
        let mut candidate_reflections: Vec<ChildReflection> = self.raycast(map, &map_targets_list, &map_light_coords, &field_light_coords, direction, &beam_sweep, intensity_profile, collimation, brightness_tables);
        
        // Now, if reflections are enabled we begin iteratively filtering and then raycasting progressively deeper levels of child reflections.
        let mut reflection_level_index: i32 = 0;
        while reflection_level_index < REFLECTION_LEVEL {
            if (&candidate_reflections).len() > 0 {
                // If there are any resulting candidate child reflections, filter them.
                let final_reflections = self.filter_candidate_reflections(candidate_reflections);
                
                // If the flag to list some info about reflections is set then display it.
                if RAYCAST_INFO == true {
                    println!("---------------------------- Level {} --------------------------", reflection_level_index + 1);
                    println!("{} filtered reflections.", (&final_reflections).len());
                    for reflection in &final_reflections {
                        println!("{} {}   {} {}   {} {}", reflection.x, reflection.y, reflection.face, reflection.direction, (reflection.float_offset).0, (reflection.float_offset).1);
                    }
                }
                
                // Iterate through and raycast each child reflection. For each, store all of the resulting sub-child reflections.
                let mut results_this_level = vec![];
                for reflection in &final_reflections {
                    // Re-calculate appropriate float offset, starting field co-ordinates, and ray targets list for this child reflection.
                    let r_pos_offset: (f64, f64) = if reflection.diffuse == false {
                        // If it is *not* a diffuse reflection, just leave the value calculated appropriately at averaging-time in place.
                        reflection.float_offset
                    } else {
                        // If it *is* a diffuse reflection, then it will have have had it's direction appropriately set at averaging-time,
                        // but will need it's float_offset re-calculating accordingly from the set default (0.5, 0), just like eg the player.
                        self.rotate_float_offset(&(reflection.float_offset), &(reflection.direction))
                    };
                    let r_field_light_coords: (f64, f64) = ((reflection.x as f64) + 0.5 + r_pos_offset.0 - (self.map_offset_start.0 as f64), (reflection.y as f64) + 0.5 + r_pos_offset.1 - (self.map_offset_start.1 as f64));
                    let reflection_light_field_result: ((i32, i32), (i32, i32), LightField) = self.determine_light_field_dimensions(&(reflection.x, reflection.y), &(reflection.intensity_profile), brightness_tables, &(reflection.collimation));
                    let map_reflection_offset_start: (i32, i32) = reflection_light_field_result.0;
                    let map_reflection_offset_end: (i32, i32) = reflection_light_field_result.1;
                    let map_reflection_targets_list: Vec<(f64, f64)> = self.locate_perimeter_targets(&map_reflection_offset_start, &map_reflection_offset_end);
                    
                    // Raycast for this child reflection.
                    let results_this_child: Vec<ChildReflection> = self.raycast(map, &map_reflection_targets_list, &(reflection.x, reflection.y), &r_field_light_coords, &(reflection.direction), &(reflection.angular_sweep), &(reflection.intensity_profile), &reflection.collimation, brightness_tables);
                    
                    // Store resulting candidate child reflections in a buffer for this level. Once we've worked through all of the pending
                    // child reflections, this buffer will be passed back to 'the top' for filtering.
                    for result in results_this_child {
                        results_this_level.push(result);
                    }
                }
                
                // Pass all candidate child reflections back to the top for filtering and subsequent raycasting.
                candidate_reflections = results_this_level;
            }
            reflection_level_index += 1;
        }
    }
    
    // Raycast around light-source in direction of input targets. For each ray, store any resulting candidate reflections. Once rays have been cast at all targets return overall list of candidate child reflections.
    fn raycast(&mut self, map: &Map, map_targets_list: &Vec<(f64, f64)>, map_light_coords: &(i32, i32), field_light_coords: &(f64, f64), direction: &f64, beam_sweep: &f64, intensity_profile: &(f64, f64, f64), collimation: &f64, brightness_tables: &BrightnessTables) -> Vec<ChildReflection> {
        let mut candidate_reflections = vec![];
        'target: for current_target in map_targets_list {
            let map_target_x_coord: f64 = current_target.0 as f64;
            let map_target_y_coord: f64 = current_target.1 as f64;
                // Get co-ordinates of target tile and distance components from light-source to
                // target tile in field space (again, adding 0.5 to make it easy to convert back to map space just
                // by adding the start offset and truncating).
                let field_target_coords: (f64, f64) = ((map_target_x_coord as f64) - (self.map_offset_start.0 as f64), (map_target_y_coord as f64) - (self.map_offset_start.1 as f64));
                let field_target_dist_comps: (f64, f64) = ((field_target_coords.0 - field_light_coords.0), (field_target_coords.1 - field_light_coords.1));
                
                // Determine target alpha angle, corrected for light-source direction.
                let alpha_angle: f64 = self.determine_alpha_angle(&field_target_dist_comps, direction, brightness_tables);
                
                // Check if we need to cast a ray (Is target tile within angular field of view?)
                if !((alpha_angle <= *beam_sweep) || (alpha_angle >= (360.0 - *beam_sweep))) {
                    // If not, stop this pass and start on next target tile in x.
                    continue 'target;
                }
                
                let field_target_distance: f64 = ((field_target_dist_comps.0).powi(2) + (field_target_dist_comps.1).powi(2)).sqrt();
                let field_dist_step: f64 = RAYCAST_DISTANCE_STEP;
                let field_dist_increments: f64 = field_target_distance / field_dist_step;
                let mut field_dist_step_comps: (f64, f64) = ((field_target_dist_comps.0 / field_dist_increments), (field_target_dist_comps.1 / field_dist_increments));
                
                let mut field_ray_distance: f64 = 0.0;
                let mut field_ray_coords: (f64, f64) = *field_light_coords;
                let mut field_ray_brightness: (f64, f64, f64) = *intensity_profile;
                
                for reflection in self.cast_ray(map, &map_light_coords, (field_dist_increments as i32), &mut field_ray_distance, field_dist_step, &mut field_dist_step_comps, &mut field_ray_coords, &mut field_ray_brightness, intensity_profile, beam_sweep, collimation, brightness_tables) {
                    candidate_reflections.push(reflection);
                }
        }
        candidate_reflections
    }
    
    // Cast a single ray. En-route, light up tiles that do not block or block sight, light up (and stop at) tiles which block and block sight, and light up and become attenuated by tiles that block but do not block sight. If any reflections are generated en-route, these are returned in a vector.
    fn cast_ray(&mut self, map: &Map, map_light_coords: &(i32, i32), field_dist_increments: i32, field_ray_distance: &mut f64, field_dist_step: f64, field_dist_step_comps: &mut (f64, f64), field_ray_coords: &mut (f64, f64), field_ray_brightness: &(f64, f64, f64), intensity_profile: &(f64, f64, f64), beam_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables) -> Vec<ChildReflection> {
        let mut ray_brightness: (f64, f64, f64) = *field_ray_brightness;
        let mut candidate_reflections = vec![];
        let mut parent_intensity: (f64, f64, f64) = *intensity_profile;
        'ray: for increment in 0..field_dist_increments {
            field_ray_coords.0 += field_dist_step_comps.0;
            field_ray_coords.1 += field_dist_step_comps.1;
            *field_ray_distance += field_dist_step;
            
            // Scale ray brightness.
            ray_brightness = self.determine_ray_brightness(&field_ray_distance, &ray_brightness, &parent_intensity, collimation, brightness_tables);
            
            // NOTE - Is this a sensible place to put this?? Should I include this check at all? It serves to calm
            // down the number of sub-reflections calculated when REFLECTION_LEVEL is set high by killing any rays
            // with all channels darker than 0.00393 ( = 1/255).
            if (ray_brightness.0 < 0.00393) && (ray_brightness.1 < 0.00393) && (ray_brightness.2 < 0.00393) {
                break 'ray;
            }
            
            // Grab where the ray is right now in field and map co-ordinates.
            let field_write_coords: (i32, i32) = ((field_ray_coords.0).trunc() as i32, (field_ray_coords.1).trunc() as i32);
            let map_check_coords: (i32, i32) = (((field_ray_coords.0).trunc() as i32) + self.map_offset_start.0, ((field_ray_coords.1).trunc() as i32) + self.map_offset_start.1);
            
            // Check if ray has left the LightField or Map.
            if (field_write_coords.0 < 0) || (field_write_coords.0 > (self.map_offset_end.0 - self.map_offset_start.0)) || (field_write_coords.1 < 0) || (field_write_coords.1 > (self.map_offset_end.1 - self.map_offset_start.1)) || (map_check_coords.0 < 0) || (map_check_coords.0 > (MAP_WIDTH - 1)) || (map_check_coords.1 < 0) || (map_check_coords.1 > (MAP_HEIGHT - 1)) {
                // If we have, stop this ray.
                break 'ray;
            }
                        
            if map[map_check_coords.0 as usize][map_check_coords.1 as usize].block_sight {
                // Ray location blocks sight, but is still the light source location, keep going, without lighting up the ray location.
                if (map_check_coords.0 == map_light_coords.0) && (map_check_coords.1 == map_light_coords.1) {
                    continue 'ray;
                }
                
                // Or, it blocks sight, and is *not* within the light source location. Light the tile up, create diffuse and reflective
                // reflections as appropriate, and then stop this ray.
                self.overwrite_tile(&ray_brightness, &field_write_coords);
                candidate_reflections = self.opaque_interaction(map, &map_check_coords, &field_ray_coords, &field_dist_step_comps, &ray_brightness, beam_sweep, collimation, brightness_tables);
                break 'ray;
            } else {
                // The ray location does *not* block sight...
                if !map[map_check_coords.0 as usize][map_check_coords.1 as usize].blocked {
                    // ...and does not block motion either, light up the tile and continue this ray.
                    self.overwrite_tile(&ray_brightness, &field_write_coords);
                    continue 'ray;
                } else {
                    // ...but does block motion, so it's a transparent solid. Light it up, have it 'filter' (modulate)
                    // the ray as it passes through according to it's color, refract the ray path according to the ratio of
                    // refractive indices and create an appropriate incident reflection.
                    self.overwrite_tile(&ray_brightness, &field_write_coords);
                    for current_candidate_reflection in self.transparent_interaction(map, &map_check_coords, &field_ray_coords, field_ray_distance, field_dist_step_comps, &ray_brightness, &mut parent_intensity, beam_sweep, collimation, brightness_tables) {
                        candidate_reflections.push(current_candidate_reflection);
                    }
                    continue 'ray;
                }
            }
        }
        candidate_reflections
    }
    
    fn transparent_interaction(&self, map: &Map, map_check_coords: &(i32, i32), field_ray_coords: &(f64, f64), field_ray_distance: &f64, field_dist_step_comps: &mut(f64, f64), ray_brightness: &(f64, f64, f64), parent_intensity: &mut(f64, f64, f64), beam_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables) -> Vec<ChildReflection> {
        let mut candidate_reflections = vec![];
        
        // Need to determine here if we have i) just entered the tile, ii) just left the tile or iii) done neither. 
        let entered: bool = ((field_ray_coords.0 - field_dist_step_comps.0).trunc() as i32 != map_check_coords.0) || ((field_ray_coords.1 - field_dist_step_comps.1).trunc() as i32 != map_check_coords.1);
        let exited: bool = ((field_ray_coords.0 + field_dist_step_comps.0).trunc() as i32 != map_check_coords.0) || ((field_ray_coords.1 + field_dist_step_comps.1).trunc() as i32 != map_check_coords.1);
        
        if entered && !exited {
            // If we have just entered the tile, modulate the ray brightness profile (as it is re-calculated anew from the lookup table value 
            // and parent brightness profile each tick, we need to modify the parent brightness profile by reference), refract the ray direction 
            // (field_dist_step_comps) by reference, and if transparent reflections are enabled we also need to create an incident reflection 
            // (this is just a normal reflection creation step). 
            *parent_intensity = self.attenuate_ray(parent_intensity, &(map[map_check_coords.0 as usize][map_check_coords.1 as usize]).color, &1.0);
            if TRANSPARENT_OBJECTS_REFRACT == true {
                // Going to need to do something along the lines of the self.reflect_direction() method, whereby there are four cases for
                // the four sides, each with two sub cases for rays hitting from either aspect. In each we do the appropriate trig to work
                // out the new ray direction components.
            }
            if TRANSPARENT_OBJECTS_REFLECTIVE == true {
                // Create candidate reflective reflection.
                let (diffuse_reflection_face, diffuse_reflection_direction) = self.determine_collision_face(field_ray_coords);
                let reflection_direction: f64 = self.reflect_direction(&diffuse_reflection_direction, field_dist_step_comps, brightness_tables);
                let reflection_brightness: (f64, f64, f64) = self.determine_ray_brightness(field_ray_distance, ray_brightness, parent_intensity, collimation, brightness_tables);
                let reflection_sweep: f64 = *beam_sweep;
                let reflection_collimation: f64 = *collimation;
                candidate_reflections.push(ChildReflection::new(false, map_check_coords.0, map_check_coords.1, (0.5, 0.0), diffuse_reflection_face, reflection_direction, reflection_brightness, reflection_sweep, reflection_collimation));
            }
        }
        candidate_reflections
    }
    
    fn opaque_interaction(&self, map: &Map, map_check_coords: &(i32, i32), field_ray_coords: &(f64, f64), field_dist_step_comps: &(f64, f64), ray_brightness: &(f64, f64, f64), beam_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables) -> Vec<ChildReflection> {
        let mut candidate_reflections = vec![];
        // Create candidate reflection(s).
        let target_reflectivity: f64 = map[map_check_coords.0 as usize][map_check_coords.1 as usize].reflectivity;
        let (diffuse_reflection_face, diffuse_reflection_direction) = self.determine_collision_face(field_ray_coords);
        if target_reflectivity == 0.0 {
            // Create the diffuse reflected component.
            let diffuse_reflection_profile: (f64, f64, f64) = self.attenuate_ray(ray_brightness, &(map[map_check_coords.0 as usize][map_check_coords.1 as usize]).color, &(1.0 - map[map_check_coords.0 as usize][map_check_coords.1 as usize].reflectivity));
            candidate_reflections.push(ChildReflection::new(true, map_check_coords.0, map_check_coords.1, (0.5, 0.0), diffuse_reflection_face, diffuse_reflection_direction, diffuse_reflection_profile, 90.0, 90.0));
        }
        if target_reflectivity == 1.0 {
            // Create the reflective component.
            let reflective_reflection_profile: (f64, f64, f64) = self.attenuate_ray(ray_brightness, &(map[map_check_coords.0 as usize][map_check_coords.1 as usize]).color, &1.0);
            let reflective_reflection_direction: f64 = self.reflect_direction(&diffuse_reflection_direction, field_dist_step_comps, brightness_tables);
            let reflective_reflection_sweep: f64 = *beam_sweep;
            let reflective_reflection_collimation: f64 = *collimation;
            candidate_reflections.push(ChildReflection::new(false, map_check_coords.0, map_check_coords.1, (0.5, 0.0), diffuse_reflection_face, reflective_reflection_direction, reflective_reflection_profile, reflective_reflection_sweep, reflective_reflection_collimation));
        }
        if (target_reflectivity > 0.0) && (target_reflectivity < 1.0) {
            // Create the diffuse reflected component.
            let diffuse_reflection_profile: (f64, f64, f64) = self.attenuate_ray(ray_brightness, &(map[map_check_coords.0 as usize][map_check_coords.1 as usize]).color, &(1.0 - target_reflectivity));
            candidate_reflections.push(ChildReflection::new(true, map_check_coords.0, map_check_coords.1, (0.5, 0.0), diffuse_reflection_face, diffuse_reflection_direction, diffuse_reflection_profile, 90.0, 90.0));
            // Create the reflective component.
            let reflective_reflection_profile: (f64, f64, f64) = self.attenuate_ray(&ray_brightness, &(map[map_check_coords.0 as usize][map_check_coords.1 as usize]).color, &1.0);
            let reflective_reflection_direction: f64 = self.reflect_direction(&diffuse_reflection_direction, field_dist_step_comps, brightness_tables);
            let reflective_reflection_sweep: f64 = *beam_sweep;
            let reflective_reflection_collimation: f64 = *collimation + ((1.0 - target_reflectivity) * (90.0 - *collimation));
            candidate_reflections.push(ChildReflection::new(false, map_check_coords.0, map_check_coords.1, (0.5, 0.0), diffuse_reflection_face, reflective_reflection_direction, reflective_reflection_profile, reflective_reflection_sweep, reflective_reflection_collimation));
        }
        candidate_reflections
    }

    fn filter_candidate_reflections(&self, candidate_reflections: Vec<ChildReflection>) -> Vec<ChildReflection> {
        let mut unique_diffuse_reflections = vec![];    
        let mut unique_reflective_reflections = vec![];    
        
        // Build vector lists of unique diffuse and reflective candidate reflections.
        for candidate_reflection in &candidate_reflections {
            let mut diffuse_flag: bool = false;
            let mut reflective_flag: bool = false;
            if (candidate_reflection.diffuse == true) && !unique_diffuse_reflections.contains(&(candidate_reflection.x, candidate_reflection.y, candidate_reflection.face, candidate_reflection.direction)) {
                diffuse_flag = true;
            }
            if (candidate_reflection.diffuse == false) && !unique_reflective_reflections.contains(&(candidate_reflection.x, candidate_reflection.y, candidate_reflection.face)) {
                reflective_flag = true;
            }
            if diffuse_flag == true {
                unique_diffuse_reflections.push((candidate_reflection.x, candidate_reflection.y, candidate_reflection.face, candidate_reflection.direction));
            }
            if reflective_flag == true {
                unique_reflective_reflections.push((candidate_reflection.x, candidate_reflection.y, candidate_reflection.face));
            }
        }
        
        let mut final_reflections = vec![];
        
        // Filter diffuse reflections.
        for averaging_location in &unique_diffuse_reflections {
            let mut average_intensity: (f64, f64, f64) = (0.0, 0.0, 0.0);
            let mut count: i32 = 0;
            for current_reflection in &candidate_reflections {
                if (current_reflection.diffuse == true) && (current_reflection.x == averaging_location.0) && (current_reflection.y == averaging_location.1) && (current_reflection.direction == averaging_location.3) {
                    average_intensity.0 += (current_reflection.intensity_profile).0;
                    average_intensity.1 += (current_reflection.intensity_profile).1;
                    average_intensity.2 += (current_reflection.intensity_profile).2;
                    count += 1;
                }
            }
            average_intensity.0 = average_intensity.0 / (count as f64);
            average_intensity.1 = average_intensity.1 / (count as f64);
            average_intensity.2 = average_intensity.2 / (count as f64);
            final_reflections.push(ChildReflection::new(true, averaging_location.0, averaging_location.1, (0.5, 0.0), averaging_location.2, averaging_location.3, average_intensity, 90.0, 90.0));
        }
        
        // Filter reflective reflections.
        for averaging_location in &unique_reflective_reflections {
            let mut average_intensity: (f64, f64, f64) = (0.0, 0.0, 0.0);
            let mut average_direction: f64 = 0.0;
            let mut average_collimation: f64 = 0.0;
            let mut average_sweep: f64 = 0.0;
            let mut count: i32 = 0;
            for current_reflection in &candidate_reflections {
                if (current_reflection.diffuse == false) && (current_reflection.x == averaging_location.0) && (current_reflection.y == averaging_location.1) && (current_reflection.face == averaging_location.2)  {
                    average_intensity.0 += (current_reflection.intensity_profile).0;
                    average_intensity.1 += (current_reflection.intensity_profile).1;
                    average_intensity.2 += (current_reflection.intensity_profile).2;
                    
                    // Still not 100% about the best way to average the directions together. If you don't re-scale
                    // the reflection directions onto a +/- 180 deg range, then rays leaving face 0 get flipped through
                    // 180 degrees incorrectly. If you do re-scale the reflection directions, then rays leaving face 2
                    // get flipped incorrectly. Tried to fix this by only re-scaling directions for rays leaving face 0.
                    // Not sure how well this works around the cut-off though...
                    if averaging_location.2 == 0 {
                        average_direction += self.normalise_angle(&current_reflection.direction);
                    } else {
                        average_direction += current_reflection.direction;
                    }
                    
                    average_collimation = current_reflection.collimation;
                    average_sweep = current_reflection.angular_sweep;
                    count += 1;
                }
            }
            average_intensity.0 = average_intensity.0 / (count as f64);
            average_intensity.1 = average_intensity.1 / (count as f64);
            average_intensity.2 = average_intensity.2 / (count as f64);
            average_direction = self.de_normalise_angle(&(average_direction / (count as f64)));
            
            // Set the appropriate float position offset according to the face the reflection will leave.
            let mut pos_offset: (f64, f64) = (0.0, 0.0);
            if averaging_location.2 == 0 {
                pos_offset = (0.5, 0.0);
            } else if averaging_location.2 == 1 {
                pos_offset = (0.0, 0.5);
            } else if averaging_location.2 == 2 {
                pos_offset = (-0.5, 0.0);
            } else if averaging_location.2 == 3 {
                pos_offset = (0.0, -0.5);
            }
            final_reflections.push(ChildReflection::new(false, averaging_location.0, averaging_location.1, pos_offset, averaging_location.2, average_direction, average_intensity, average_sweep, average_collimation));
        }
        final_reflections
    }
    
    fn normalise_angle(&self, angle: &f64) -> f64 {
        let output_angle = if *angle >= 180.0 {
                                *angle - 360.0
                            } else {
                                *angle
                            };
        output_angle
    }
    
    fn de_normalise_angle(&self, angle: &f64) -> f64 {
        let output_angle = if *angle < 0.0 {
                                *angle + 360.0
                            } else  if *angle > 360.0 {
                                *angle - 360.0
                            } else {
                                *angle
                            };
        output_angle
    }
    
    fn reflect_direction(&self, collision_face: &f64, field_dist_step_comps: &(f64, f64), brightness_tables: &BrightnessTables) -> f64 {
        // Switched from calculating inverse tangents to the use of a lookup table.
        //let atan_rad: f64 = ((field_dist_step_comps.1) / (field_dist_step_comps.0)).atan();
        //let atan_deg: f64 = atan_rad.to_degrees().abs();
        let ratio = if (field_dist_step_comps.1 / field_dist_step_comps.0) > 100.0 {
            100.0
        } else if (field_dist_step_comps.1 / field_dist_step_comps.0) < -100.0 {
            -100.0
        } else {
            field_dist_step_comps.1 / field_dist_step_comps.0
        };
        let atan_deg: f64 = brightness_tables.read_tangent_table(&ratio).abs();
        
        let mut reflected_direction: f64 = 0.0;
        
        if *collision_face == 0.0 {
            if field_dist_step_comps.1 > 0.0 {
                reflected_direction = atan_deg;
            }
            if field_dist_step_comps.1 <= 0.0 {
                reflected_direction = 360.0 - atan_deg;
            }
        }
        if *collision_face == 90.0 {
            if field_dist_step_comps.0 >= 0.0 {
                reflected_direction = atan_deg;
            }
            if field_dist_step_comps.0 < 0.0 {
                reflected_direction = 180.0 - atan_deg;
            }
        }
        if *collision_face == 180.0 {
            if field_dist_step_comps.1 >= 0.0 {
                reflected_direction = 180.0 - atan_deg;
            }
            if field_dist_step_comps.1 < 0.0 {
                reflected_direction = 180.0 + atan_deg;
            }
        }
        if *collision_face == 270.0 {
            if field_dist_step_comps.0 >= 0.0 {
                reflected_direction = 360.0 - atan_deg;
            }
            if field_dist_step_comps.0 < 0.0 {
                reflected_direction = 180.0 + atan_deg;
            }
        }
                
        if reflected_direction < 0.0 {
            // Underflow, add 360 deg.
            reflected_direction = reflected_direction + 360.0;
        } else {
            if reflected_direction >= 360.0 {
                // Overflow, subtract 360 deg.
                reflected_direction = reflected_direction - 360.0;
            }
        }
        reflected_direction
    }
    
    fn determine_collision_face(&self, field_ray_coords: &(f64, f64)) -> (i32, f64) {
        let field_ray_coordinates: (f64, f64) = (field_ray_coords.0 - (field_ray_coords.0).trunc(), field_ray_coords.1 - (field_ray_coords.1).trunc());
        let field_tile_coordinates: (f64, f64) = (0.0, 0.0);
        let mut face: i32 = 0;
        let mut direction: f64 = 0.0;
        
        if ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.9)) && (field_ray_coordinates.0 < (field_tile_coordinates.0 + 1.0))) && ((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.0)) && (field_ray_coordinates.1 < (field_tile_coordinates.1 + 1.0))) && (((field_ray_coordinates.1 < (field_tile_coordinates.1 + 0.5)) && (field_ray_coordinates.1 > ((-1.0 * field_ray_coordinates.0) + 1.0))) || ((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.5)) && (field_ray_coordinates.0 >= field_ray_coordinates.1))) {
            face = 0;
            direction = 0.0;
        }
        if ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.0)) && (field_ray_coordinates.0 < (field_tile_coordinates.0 + 1.0))) && ((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.9)) && (field_ray_coordinates.1 < (field_tile_coordinates.1 + 1.0))) && (((field_ray_coordinates.0 < (field_tile_coordinates.0 + 0.5)) && (field_ray_coordinates.1 >= ((-1.0 * field_ray_coordinates.0) + 1.0))) || ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.5)) && (field_ray_coordinates.0 < field_ray_coordinates.1))) {
            face = 1;
            direction = 90.0;
        }
        if ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.0)) && (field_ray_coordinates.0 < (field_tile_coordinates.0 + 0.1))) && ((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.0)) && (field_ray_coordinates.1 < (field_tile_coordinates.1 + 1.0))) && (((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.5)) && (field_ray_coordinates.1 < ((-1.0 * field_ray_coordinates.0) + 1.0))) || ((field_ray_coordinates.1 < (field_tile_coordinates.1 + 0.5)) && (field_ray_coordinates.0 <= field_ray_coordinates.1))) {
            face = 2;
            direction = 180.0;
        }
        if ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.0)) && (field_ray_coordinates.0 < (field_tile_coordinates.0 + 1.0))) && ((field_ray_coordinates.1 >= (field_tile_coordinates.1 + 0.0)) && (field_ray_coordinates.1 < (field_tile_coordinates.1 + 0.1))) && (((field_ray_coordinates.0 < (field_tile_coordinates.1 + 0.5)) && (field_ray_coordinates.0 >= field_ray_coordinates.1)) || ((field_ray_coordinates.0 >= (field_tile_coordinates.0 + 0.5)) && (field_ray_coordinates.1 < ((-1.0 * field_ray_coordinates.0) + 1.0)))) {
            face = 3;
            direction = 270.0;
        }
        (face, direction)
    }
    
    fn attenuate_ray(&self, ray_intensity: &(f64, f64, f64), tile_color: &(f64, f64, f64), scale_factor: &f64) -> (f64, f64, f64) {
        let trb = ray_intensity.0 * tile_color.0 * *scale_factor * REFLECTION_BRIGHTNESS_SCALING;
        let tgb = ray_intensity.1 * tile_color.1 * *scale_factor * REFLECTION_BRIGHTNESS_SCALING;
        let tbb = ray_intensity.2 * tile_color.2 * *scale_factor * REFLECTION_BRIGHTNESS_SCALING;
        (trb, tgb, tbb)
    }
    
    fn overwrite_tile(&mut self, ray_brightness: &(f64, f64, f64), field_coords: &(i32, i32)) {
        if self.light_field[field_coords.0 as usize][field_coords.1 as usize].0 < ray_brightness.0 {
            self.light_field[field_coords.0 as usize][field_coords.1 as usize].0 = ray_brightness.0;
        }
        if self.light_field[field_coords.0 as usize][field_coords.1 as usize].1 < ray_brightness.1 {
            self.light_field[field_coords.0 as usize][field_coords.1 as usize].1 = ray_brightness.1;
        }
        if self.light_field[field_coords.0 as usize][field_coords.1 as usize].2 < ray_brightness.2 {
            self.light_field[field_coords.0 as usize][field_coords.1 as usize].2 = ray_brightness.2;
        }
    }
    
    fn check_adjacent(&self, map_target_position: &(i32, i32), map_light_source_position: &(i32, i32)) -> bool {
        let mut adjacent: bool = false;
        if (((map_target_position.0 - map_light_source_position.0) == 0) && ((map_target_position.1 - map_light_source_position.1).abs() == 1)) || (((map_target_position.0 - map_light_source_position.0).abs() == 1) && ((map_target_position.1 - map_light_source_position.1) == 0)) {
            adjacent = true;
        }
        adjacent
    }
    
    fn determine_ray_brightness(&self, ray_distance_travelled: &f64, ray_intensity_profile: &(f64, f64, f64), parent_intensity_profile: &(f64, f64, f64), collimation: &f64, brightness_tables: &BrightnessTables) -> (f64, f64, f64) {
        // Reduce ray brightness -----------------------------------------------------------------------------
        let mut modulation_distance: f64 = *ray_distance_travelled;
        if modulation_distance < 1.0 {
            modulation_distance = 1.0;
        }
        let modulation: f64 = brightness_tables.read_brightness_table(&modulation_distance, collimation);
        let mut intensity_profile: (f64, f64, f64) = (ray_intensity_profile.0, ray_intensity_profile.1, ray_intensity_profile.2);
        
        intensity_profile.0 = parent_intensity_profile.0 * modulation;
        if intensity_profile.0 > parent_intensity_profile.0 {
            intensity_profile.0 = parent_intensity_profile.0;
        }
        intensity_profile.1 = parent_intensity_profile.1 * modulation;
        if intensity_profile.1 > parent_intensity_profile.1 {
            intensity_profile.1 = parent_intensity_profile.1;
        }
        intensity_profile.2 = parent_intensity_profile.2 * modulation;
        if intensity_profile.2 > parent_intensity_profile.2 {
            intensity_profile.2 = parent_intensity_profile.2;
        }
        (intensity_profile.0, intensity_profile.1, intensity_profile.2)
    }

    fn determine_alpha_angle(&self, field_light_target_dist_comps: &(f64, f64), direction: &f64, brightness_tables: &BrightnessTables) -> f64 {
        // Determine which quadrant the target is in with respect to alpha = 0 deg and calculate
        // the corresponding target alpha angle.
        let mut alpha_angle: f64 = 0.0;
        
        // Switched from calculating inverse tangents to the use of a lookup table.
        //~let atan_rad: f64 = ((field_light_target_dist_comps.1) / (field_light_target_dist_comps.0)).atan();
        //~let atan_deg: f64 = atan_rad.to_degrees().abs();
        let ratio = if (field_light_target_dist_comps.1 / field_light_target_dist_comps.0) > 100.0 {
            100.0
        } else if (field_light_target_dist_comps.1 / field_light_target_dist_comps.0) < -100.0 {
            -100.0
        } else {
            field_light_target_dist_comps.1 / field_light_target_dist_comps.0
        };
        let atan_deg: f64 = brightness_tables.read_tangent_table(&ratio).abs();
        
        if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
            alpha_angle = atan_deg;
        } else {
            if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
                alpha_angle = 180.0 - atan_deg;
            } else {
                if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                    alpha_angle = atan_deg + 180.0;
                } else {
                    if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                        alpha_angle = 360.0 - atan_deg;
                    }
                }
            }
        }        
        // Apply alpha angle direction modifier.
        alpha_angle = alpha_angle - *direction;
        if alpha_angle < 0.0 {
            // Underflow, add 360 deg.
            alpha_angle = alpha_angle + 360.0;
        } else {
            if alpha_angle >= 360.0 {
                // Overflow, subtract 360 deg.
                alpha_angle = alpha_angle - 360.0;
            }
        }
        alpha_angle
    }
    
    fn determine_light_field_dimensions(&self, position: &(i32, i32), intensity_profile: &(f64, f64, f64), brightness_tables: &BrightnessTables, collimation: &f64) -> ((i32, i32), (i32, i32), LightField) {
        // Determine maximum intensity.
        let max_intensity: f64 = (intensity_profile.0).max(intensity_profile.1).max(intensity_profile.2);
        
        // Determine the map and field space beam radius according to the highest intensity component.
        // The origin of the 'magic number' comes from the fact that if the reference brightness is defined
        // at a distance of 1.0, then the maximum radius corresponds to the square root of the ratio of initial and 
        // minimum brightnesses. In this case the minimum brightness is 1/255 (8-bit color) = 0.0039215...
        let float_light_radius: f64 = brightness_tables.read_distance_table(&max_intensity, collimation);
        
        // Make sure that integer radius is rounded-up, this makes sure we always catch all of the dark
        // tiles to the periphery of the light field.
        let mut conversion: f64 = float_light_radius;
        if conversion - conversion.trunc() < 0.5 {
            conversion = conversion + 0.5;
        }
        let int_light_radius: i32 = conversion.round() as i32;
        
        // Grab light-source co-ordinates in map space and with the radius calculate the
        // co-ordinates of the light fields bounding box (in map space). We store these along
        // with the light field in the LightFieldObject, as it is what we use to know where to
        // composit the light field into the overall map light field at rendering time.
        let map_light_coords: (i32, i32) = (position.0, position.1);
        let mut map_offset_start: (i32, i32) = ((map_light_coords.0 - int_light_radius), (map_light_coords.1 - int_light_radius));
        let mut map_offset_end: (i32, i32) = ((map_light_coords.0 + int_light_radius), (map_light_coords.1 + int_light_radius));
        
        if map_offset_start.0 < 0 {
            map_offset_start.0 = 0;
        }
        if map_offset_end.0 > MAP_WIDTH - 1{
            map_offset_end.0 = MAP_WIDTH - 1;
        }
        if map_offset_start.1 < 0 {
            map_offset_start.1 = 0;
        } 
        if map_offset_end.1 > MAP_HEIGHT - 1 {
            map_offset_end.1 = MAP_HEIGHT - 1;
        }
        
        let light_field_dimensions: (i32, i32) = ((map_offset_end.0 - map_offset_start.0) + 1, (map_offset_end.1 - map_offset_start.1) + 1);
        let light_field: LightField = vec![vec![(0.0, 0.0, 0.0); light_field_dimensions.1 as usize]; light_field_dimensions.0 as usize];
        
        (map_offset_start, map_offset_end, light_field)
    }
    
    fn rotate_float_offset(&self, float_offset: &(f64, f64), alpha_angle_modifier: &f64) -> (f64, f64) {
        let offset_vector_resultant: f64 = ((float_offset.0).powi(2) + (float_offset.1).powi(2)).sqrt();
        let mut pos_offset: (f64, f64) = (0.0, 0.0);
        if (float_offset.0 >= 0.0) && (float_offset.1 >= 0.0) {
            // Quadrant 0.
            pos_offset.0 = offset_vector_resultant * (alpha_angle_modifier.to_radians()).cos();
            pos_offset.1 = offset_vector_resultant * (alpha_angle_modifier.to_radians()).sin();
        } else {
            if (float_offset.0 < 0.0) && (float_offset.1 >= 0.0) {
                // Quadrant 1.
                pos_offset.0 = -1.0 * offset_vector_resultant * ((180.0 - alpha_angle_modifier).to_radians()).cos();
                pos_offset.1 = offset_vector_resultant * ((180.0 - alpha_angle_modifier).to_radians()).sin();
            } else {
                if (float_offset.0 < 0.0) && (float_offset.1 < 0.0) {
                    // Quadrant 2.
                    pos_offset.0 = -1.0 * offset_vector_resultant * ((alpha_angle_modifier - 180.0).to_radians()).cos();
                    pos_offset.1 = -1.0 * offset_vector_resultant * ((alpha_angle_modifier - 180.0).to_radians()).sin();
                } else {
                    if (float_offset.0 >= 0.0) && (float_offset.1 < 0.0) {
                        // Quadrant 3.
                        pos_offset.0 = offset_vector_resultant * ((360.0 - alpha_angle_modifier).to_radians()).cos();
                        pos_offset.1 = -1.0 * offset_vector_resultant * ((360.0 - alpha_angle_modifier).to_radians()).sin();
                    }
                }
            }
        }
        pos_offset
    }
    
    fn locate_perimeter_targets(&self, map_offset_start: &(i32, i32), &map_offset_end: &(i32, i32)) -> Vec<(f64, f64)> {
        let mut targets_list = vec![];
        let raycast_spacing: f64 = 1.0/(RAYCAST_FINENESS as f64);
        // (min, min) corner point.
        targets_list.push(((map_offset_start.0 as f64) + (1.0 - (raycast_spacing * 0.5)), (map_offset_start.1 as f64) + (1.0 - (raycast_spacing * 0.5))));
        // (min) row.
        for x_ind in (map_offset_start.0 + 1)..(map_offset_end.0) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((x_ind as f64) + (0.5 * raycast_spacing) + (raycast_spacing * (subray_index as f64)), (map_offset_start.1 as f64) + (1.0 - (raycast_spacing * 0.5))));
            }
        }
        // (max, min) corner point.
        targets_list.push(((map_offset_end.0 as f64) + (raycast_spacing * 0.5), (map_offset_start.1 as f64) + (1.0 - (raycast_spacing * 0.5))));
        // (max) column.
        for y_ind in (map_offset_start.1 + 1)..(map_offset_end.1) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((map_offset_end.0 as f64) + (raycast_spacing * 0.5), (y_ind as f64) + (0.5 * raycast_spacing) + (raycast_spacing * (subray_index as f64))));
            }
        }
        // (max, max) corner point.
        targets_list.push(((map_offset_end.0 as f64) + (raycast_spacing * 0.5), (map_offset_end.1 as f64) + (raycast_spacing * 0.5)));;
        // (max) row.
        for x_ind in (map_offset_start.0 + 1)..(map_offset_end.0) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((x_ind as f64) + (0.5 * raycast_spacing) + (raycast_spacing * (subray_index as f64)), (map_offset_end.1 as f64) + (raycast_spacing * 0.5)));
            }
        }
        // (min, max) corner point.
        targets_list.push(((map_offset_start.0 as f64) + (1.0 - (0.5 * raycast_spacing)), (map_offset_end.1 as f64) + (0.5 * raycast_spacing)));
        // (min) column.
        for y_ind in (map_offset_start.1 + 1)..(map_offset_end.1) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((map_offset_start.0 as f64) + (1.0 - (0.5 * raycast_spacing)), (y_ind as f64) + (0.5 * raycast_spacing) + (raycast_spacing * (subray_index as f64))));
            }
        }
        targets_list
    }
}



// Define our 'Object' object, which will be used to represent all in-world objects.
#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    direction: f64,
    char: char,
    color: (f64, f64, f64),
    light_source: (bool, (f64, f64, f64), f64, f64, (f64, f64), bool),
    light_field_object: LightFieldObject,
}

// Here we define the 'Object' object methods.
impl Object {
    pub fn new(x: i32, y: i32, direction: f64, char: char, color: (f64, f64, f64), light_source: (bool, (f64, f64, f64), f64, f64, (f64, f64), bool)) -> Self {
        let light_field_object: LightFieldObject = LightFieldObject::new();
        
        Object {
            x: x,
            y: y,
            direction: direction,
            char: char,
            color: color,
            light_source: light_source,
            light_field_object: light_field_object,
        }
    }
    
    pub fn set_recalculation_flag(&mut self) {
        self.light_source.5 = true;
    }
    
    pub fn clear_recalculation_flag(&mut self) {
        self.light_source.5 = false;
    }
    
    // Move object by dx, dy.
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map) {
        if !map[(self.x + dx) as usize][(self.y + dy) as usize].blocked {
            if !(((self.x + dx) < 0) || ((self.x + dx) > (MAP_WIDTH - 1))) {
                self.x += dx;
            }
            if !(((self.y + dy) < 0) || ((self.y + dy) > (MAP_HEIGHT - 1))) {
                self.y += dy;
            }
            if dx > 0 {
                self.direction = 0.0;
            }
            if dx < 0 {
                self.direction = 180.0;
            }
            if dy > 0 {
                self.direction = 90.0;
            }
            if dy < 0 {
                self.direction = 270.0;
            }
            // If the object is illuminated, we will need to recompute it's light field
            // whenever it moves. Strictly, we only need to recompute the light field whenever
            // it moves if we have reflections enabled, otherwise symmetrical light fields do
            // not change when moving. Non-symmetrical light fields would only change when
            // rotating, not when translating. As we want to enable reflections, we will
            // leave this without any additional conditonality.
            self.set_recalculation_flag();
        }
    }
    
    pub fn toggle_light(&mut self) {
        if !(self.light_source.0) {
            self.light_source.0 = true;
        } else {
            self.light_source.0 = false;
        }
        // Recompute light field.
        self.set_recalculation_flag();
        println!("Flashlight toggled.");
    }
    
    pub fn pivot(&mut self, clockwise: bool) {
        if !clockwise {
            self.direction = self.direction + 5.0;
            println!("Turned clockwise.");
        } else {
            self.direction = self.direction - 5.0;
            println!("Turned counter-clockwise");
        }
        if self.direction < 0.0 {
            self.direction = self.direction + 360.0;
        } else {
            if self.direction >= 360.0 {
                self.direction = self.direction - 360.0;
            }
        }
        // Recompute light field.
        println!("{}.", self.direction);
        self.set_recalculation_flag();
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
fn make_map() -> (Map, (i32, i32), Vec<Rect>) {
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
            
            // Create 3x3 central island.
            map[(new_x - 1) as usize][(new_y - 1) as usize].blocked = true;
            map[(new_x - 1) as usize][(new_y - 1) as usize].block_sight = false;
            map[(new_x - 1) as usize][(new_y - 1) as usize].color = (1.0, 0.0, 0.0);
            map[(new_x - 1) as usize][(new_y - 1) as usize].reflectivity = 1.0;
            map[(new_x - 1) as usize][(new_y - 1) as usize].refractive_index = 2.0;
            map[(new_x + 1) as usize][(new_y - 1) as usize].blocked = true;
            map[(new_x + 1) as usize][(new_y - 1) as usize].block_sight = false;
            map[(new_x + 1) as usize][(new_y - 1) as usize].color = (0.0, 1.0, 0.0);
            map[(new_x + 1) as usize][(new_y - 1) as usize].reflectivity = 1.0;
            map[(new_x + 1) as usize][(new_y - 1) as usize].refractive_index = 2.0;
            map[(new_x - 1) as usize][(new_y + 1) as usize].blocked = true;
            map[(new_x - 1) as usize][(new_y + 1) as usize].block_sight = false;
            map[(new_x - 1) as usize][(new_y + 1) as usize].color = (0.0, 0.0, 1.0);
            map[(new_x - 1) as usize][(new_y + 1) as usize].reflectivity = 1.0;
            map[(new_x - 1) as usize][(new_y + 1) as usize].refractive_index = 2.0;
            map[(new_x + 1) as usize][(new_y + 1) as usize].blocked = true;
            map[(new_x + 1) as usize][(new_y + 1) as usize].block_sight = false;
            map[(new_x + 1) as usize][(new_y + 1) as usize].color = (1.0, 0.0, 1.0);
            map[(new_x + 1) as usize][(new_y + 1) as usize].reflectivity = 1.0;
            map[(new_x + 1) as usize][(new_y + 1) as usize].refractive_index = 2.0;
            
            if rooms.is_empty() {
                // Then this is the first room, so we set the player start
                // co-ordinates appropriately.
                starting_position = (new_x + 3, new_y + 3);
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
    (map, starting_position, rooms)
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
        Key { printable: 'a', .. } => player.pivot(true),
        Key { printable: 's', .. } => player.pivot(false),
        // Function keys.
        Key { printable: 'f', .. } => player.toggle_light(),
        // Default behaviour.
        _ => {},
    }
    false
}

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &mut[Object], map: &mut Map, fov_map: &mut FovMap, light_field: &mut LightField) {
    
    // Recompte player (objects[0]) FOV.
    fov_map.compute_fov((&objects[0]).x, (&objects[0]).y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    
    // Composit all object light-fields into the map-wide light field.
    integrate_light_fields(objects, light_field);
    
    // Next, draw all world tiles.
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = fov_map.is_in_fov(x, y) && ((light_field[x as usize][y as usize].0 + light_field[x as usize][y as usize].1 + light_field[x as usize][y as usize].2) > 0.0);
            
            // If we borrow map as mutable first, then we cannot borrow it as unmutable
            // afterwards to create the mutable wall_color.
            let wall_color = map[x as usize][y as usize].color;
            let explored = &mut map[x as usize][y as usize].explored;
            
            let mut display_color: (i32, i32, i32) = (0, 0, 0);
            
            if visible {
                let float_r_channel_output: f64 = wall_color.0 * ((light_field[x as usize][y as usize].0) + AMBIENT_ILLUMINATION.0);
                let float_g_channel_output: f64 = wall_color.1 * ((light_field[x as usize][y as usize].1) + AMBIENT_ILLUMINATION.1);
                let float_b_channel_output: f64 = wall_color.2 * ((light_field[x as usize][y as usize].2) + AMBIENT_ILLUMINATION.2);
                
                // -- Code to turn linearised total brightness into a log brightness --
                display_color = correct_colors(&float_r_channel_output, &float_g_channel_output, &float_b_channel_output);
                // --------------------------------------------------------------------
                *explored = true;
                
            } else {
                
                let mut not_visible_illumination: (f64, f64, f64) = AMBIENT_ILLUMINATION;
                not_visible_illumination.0 = not_visible_illumination.0 / 5.0;
                if not_visible_illumination.0 < MIN_NOT_VISIBLE_ILLUMINATION.0 {
                    not_visible_illumination.0 = MIN_NOT_VISIBLE_ILLUMINATION.0;
                }
                not_visible_illumination.1 = not_visible_illumination.1 / 5.0;
                if not_visible_illumination.1 < MIN_NOT_VISIBLE_ILLUMINATION.1 {
                    not_visible_illumination.1 = MIN_NOT_VISIBLE_ILLUMINATION.1;
                }
                not_visible_illumination.2 = not_visible_illumination.2 / 5.0;
                    not_visible_illumination.2 = MIN_NOT_VISIBLE_ILLUMINATION.2;
                if not_visible_illumination.2 < MIN_NOT_VISIBLE_ILLUMINATION.2 {
                }
                let float_r_channel_output: f64 = wall_color.0 * not_visible_illumination.0;
                let float_g_channel_output: f64 = wall_color.1 * not_visible_illumination.1;
                let float_b_channel_output: f64 = wall_color.2 * not_visible_illumination.2;
                
                // -- Code to turn linearised total brightness into a log brightness --
                display_color = correct_colors(&float_r_channel_output, &float_g_channel_output, &float_b_channel_output);
                // --------------------------------------------------------------------
                
            }
            
            if *explored {
                con.set_char_background(x, y, Color { r: (display_color.0 as u8), g: (display_color.1 as u8), b: (display_color.2 as u8) }, BackgroundFlag::Set);
            }
        }
    }
    
    // Next draw all world objects.
    for object in objects {
        let visible = fov_map.is_in_fov(object.x, object.y);
        let mut display_color: (i32, i32, i32) = (0, 0, 0);
        
        if visible {
            let float_r_channel_output: f64 = ((object.color).0) * ((light_field[object.x as usize][object.y as usize].0) + AMBIENT_ILLUMINATION.0);
            let float_g_channel_output: f64 = ((object.color).1) * ((light_field[object.x as usize][object.y as usize].1) + AMBIENT_ILLUMINATION.1);
            let float_b_channel_output: f64 = ((object.color).2) * ((light_field[object.x as usize][object.y as usize].2) + AMBIENT_ILLUMINATION.2);
            
            // -- Code to turn linearised total brightness into a log brightness --
            display_color = correct_colors(&float_r_channel_output, &float_g_channel_output, &float_b_channel_output);
            // --------------------------------------------------------------------
                    
            object.draw(con, Color { r: (display_color.0 as u8), g: (display_color.1 as u8), b: (display_color.2 as u8) });
        }
    }
    
    // Lastly, blit the composition terminal contents into the root terminal.
    blit(con, (0,0), (SCREEN_WIDTH, SCREEN_HEIGHT), root, (0,0), 1.0, 1.0);
}

fn update_direct_light_fields(map: &Map, objects: &mut[Object], brightness_tables: &BrightnessTables) -> bool {
    // Loop through all objects. Any which are illuminated, and which have their recalculate flag set to true, have their light-field
    // recalculated and their recalculate flag is then cleared. If any light-fields have been recalculated, return redraw = true to
    // instruct the rendering function to re-render the world.
    // Recalculate the light-fields for any illuminated objects for which the recalculation flag is set.
    let mut redraw: bool = false;
    for object in objects {
        if ((object.light_source).0) && ((object.light_source).5) {
            redraw = true;
            let float_offset: (f64, f64) = object.light_source.4;
            let angular_sweep: f64 = object.light_source.2;
            let collimation: f64 = object.light_source.3;
            let intensity: (f64, f64, f64) = object.light_source.1;
            let direction: f64 = object.direction;
            object.light_field_object.recalculate(&map, &object.x, &object.y, &float_offset, &intensity, &direction, &angular_sweep, &collimation, brightness_tables);
            object.clear_recalculation_flag();
        }
    }
    // Return the redraw flag.
    redraw
}
    
fn integrate_light_fields(objects: &[Object], light_field: &mut LightField) {
    // First, zero the map-wide light-field.
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            light_field[x as usize][y as usize].0 = 0.0;
            light_field[x as usize][y as usize].1 = 0.0;
            light_field[x as usize][y as usize].2 = 0.0;
        }
    }
    // Next, sum each object's light-fields into the map-wide light-field.
    for object in objects {
        if object.light_source.0 {
            let object_light_field = &object.light_field_object.light_field;
            let map_start_offset = &object.light_field_object.map_offset_start;
            let map_end_offset = &object.light_field_object.map_offset_end;
            
            'y_loop: for y in (map_start_offset.1)..(map_end_offset.1) {
                        if (y < 0) || (y > (MAP_HEIGHT - 1)) {
                            continue 'y_loop;
                        }
                        'x_loop: for x in (map_start_offset.0)..(map_end_offset.0) {
                                    if (x < 0) || (x > (MAP_WIDTH - 1)) {
                                        continue 'x_loop;
                                    }
                                    if (object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].0 > 0.0) || (object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].1 > 0.0) || (object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].2 > 0.0) {
                                        light_field[x as usize][y as usize].0 = light_field[x as usize][y as usize].0 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].0;
                                        light_field[x as usize][y as usize].1 = light_field[x as usize][y as usize].1 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].1;
                                        light_field[x as usize][y as usize].2 = light_field[x as usize][y as usize].2 + object_light_field[(x - map_start_offset.0) as usize][(y - map_start_offset.1) as usize].2;
                                    }
                        }
                }
            
        }
    }
}

fn correct_colors(float_r_channel_output: &f64, float_g_channel_output: &f64, float_b_channel_output: &f64) -> (i32, i32, i32) {
    // -- Code to turn linearised total brightness into a log brightness --
    let a = -1.01179495;
    let b = -4.47099458;
    let c = 1.01214152;
    
    let mut display_color: (i32, i32, i32) = (0, 0, 0);
    let corrected_r_channel_output: f64 = (((b * float_r_channel_output).exp()) * a) + c;
    let corrected_g_channel_output: f64 = (((b * float_g_channel_output).exp()) * a) + c;
    let corrected_b_channel_output: f64 = (((b * float_b_channel_output).exp()) * a) + c;
    
    let mut channels: (f64, f64, f64) = (corrected_r_channel_output, corrected_g_channel_output, corrected_b_channel_output);
    let mut scaling_proportion: f64 = 0.0;
    
    // If any color channels have overflowed.
    if (corrected_r_channel_output > 1.0) || (corrected_g_channel_output > 1.0) || (corrected_b_channel_output > 1.0) {
        // Scale brightest channel down to 1.0 and then scale down remaining channels by same proportion.
        if corrected_r_channel_output >= corrected_g_channel_output.max(corrected_b_channel_output) {
            scaling_proportion = 1.0 / channels.0;
        } else {
            if corrected_g_channel_output >= corrected_r_channel_output.max(corrected_b_channel_output) {
                scaling_proportion = 1.0 / channels.1;
            } else {
                if corrected_b_channel_output >= corrected_r_channel_output.max(corrected_g_channel_output) {
                    scaling_proportion = 1.0 / channels.2;
                }
            }
        }
        channels.0 = channels.0 * scaling_proportion;
        channels.1 = channels.1 * scaling_proportion;
        channels.2 = channels.2 * scaling_proportion;
    }
    display_color.0 = (channels.0 * 255.0) as i32;
    display_color.1 = (channels.1 * 255.0) as i32;
    display_color.2 = (channels.2 * 255.0) as i32;
    
    display_color
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
    
    // Create our BrightnessTables.
    let brightness_tables: BrightnessTables = BrightnessTables::new();
    
    // Create our 'composition' terminal, off-screen, in which we will compose each frame.
    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);
    
    // Create the FOV map.
    let mut fov_map = FovMap::new(MAP_WIDTH, MAP_HEIGHT);
    
    // Instantiate a map.
    let (mut map, (player_x, player_y), rooms) = make_map();
    
    // Setup field of view map.
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            fov_map.set(x, y, !map[x as usize][y as usize].block_sight, !map[x as usize][y as usize].blocked);
        }
    }
    
    // First calculation of FOV map.
    fov_map.compute_fov(player_x, player_y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    
    // Instantiate the objects vector, and create the player and cat buddy objects and append them.
    let mut objects = vec![];
    objects.push(Object::new(player_x, player_y, 0.0, '@', COLOR_PLAYER, (true, (0.6, 0.6, 0.6), 30.0, 60.0, (0.4, 0.0), true)));
    objects.push(Object::new(player_x - 1, player_y-1, 0.0, 'c', COLOR_CAT_BUDDY, (false, (0.0, 0.0, 0.0), 0.0, 0.0, (0.4, 0.0), true)));
    
    // Populate created rooms with light objects.
    for new_room in rooms {
        // Add corner lights pointing inwards diagonally.
        let directions: (f64, f64, f64, f64) = (45.0, 135.0, 225.0, 315.0);
        let torch_brightness: (f64, f64, f64) = (0.3, 0.3, 0.0);
        let torch_angle: f64 = 45.0;
        let torch_collimation: f64 = 70.0;
        let float_offset: (f64, f64) = (0.0, 0.0);
        objects.push(Object::new(new_room.x1+1, new_room.y1+1, directions.0, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation, float_offset, true)));
        objects.push(Object::new(new_room.x2-1, new_room.y1+1, directions.1, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation, float_offset, true)));
        objects.push(Object::new(new_room.x2-1, new_room.y2-1, directions.2, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation, float_offset, true)));
        objects.push(Object::new(new_room.x1+1, new_room.y2-1, directions.3, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation, float_offset, true)));
    }
    
    // Generate master illumination map.
    let mut light_field: LightField = vec![vec![(0.0, 0.0, 0.0); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    // Catch initial player state.
    let mut old_flashlight_state: bool = (objects[0].light_source).0;
    let mut old_player_position: (i32, i32) = (objects[0].x, objects[0].y);
    
    // Main world loop.
    while !root.window_closed() {
        
        // Check if any light-sources have changed, or if player state has changed in a manner which requires redrawing.
        let redraw: bool = update_direct_light_fields(&map, &mut objects, &brightness_tables) || ((objects[0].light_source).0 != old_flashlight_state) || ((objects[0].x != old_player_position.0) || (objects[0].y != old_player_position.1));
        
        // Redraw the world and objects if needed.
        if redraw == true {
            old_flashlight_state = (objects[0].light_source).0;
            old_player_position = (objects[0].x, objects[0].y);
            render_all(&mut root, &mut con, &mut objects, &mut map, &mut fov_map, &mut light_field);
        }
        
        // Display the contents of the root terminal.
        root.flush();
        
        // Clear all objects from composition terminal.
        for object in &objects {
            object.clear(&mut con);
        }
        
        //let exit = handle_keys(&mut root, &mut objects[0]);
        let exit = handle_keys(&mut root, &mut objects[0], &map);
        
        if exit { break; }
    }
}
