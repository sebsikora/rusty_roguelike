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
//const COLOR_GROUND: (f64, f64, f64) = (0.247, 0.471, 0.0039);
const COLOR_GROUND: (f64, f64, f64) = (0.1, 0.1, 0.1);

const COLOR_PLAYER: (f64, f64, f64) = (1.0, 1.0, 0.0);
const COLOR_CAT_BUDDY: (f64, f64, f64) = (1.0, 0.502, 0.0);

const ROOM_MAX_SIZE: i32 = 35;
const ROOM_MIN_SIZE: i32 = 15;
const MAX_ROOMS: i32 = 5;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 0;        // 0 = unlimited.

const AMBIENT_ILLUMINATION: (f64, f64, f64) = (0.0, 0.0, 0.0);
const MIN_NOT_VISIBLE_ILLUMINATION: (f64, f64, f64) = (0.015, 0.015, 0.015);
const ILLUMINATION_MODULATION: f64 = 0.0;
const RAYCAST_DISTANCE_STEP: f64 = 0.05;
const RAYCAST_FINENESS: i32 = 5;
const REFLECTION_LEVEL: i32 = 1;
const REFLECTION_STATUS: bool = false;

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


struct BrightnessTables {
    brightness_tables: Vec<Vec<f64>>,
    distance_tables: Vec<Vec<f64>>,
}

impl BrightnessTables {
    pub fn new() -> BrightnessTables {
        let mut brightness_tables = vec![];
        for double_angle in 0..181 {
            let mut angle: f64 = (double_angle as f64) / 2.0;
            if angle > 90.0 {
                angle = 90.0;
            }
            let mut distance: f64 = 0.0;
            let mut table = vec![];
            let power_coeff: f64 = angle / 45.0;
            for increment in 1..4001 {
                let distance: f64 = 1.0 + ((increment as f64) * 0.05);
                let brightness_scaling: f64 = 1.0 / distance.powf(power_coeff);
                table.push(brightness_scaling);
            }
            brightness_tables.push(table);
        }
        
        let mut distance_tables = vec![];
        for double_collimation in 0..181 {
            let mut collimation: f64 = (double_collimation as f64) / 2.0;
            if collimation > 90.0 {
                collimation = 90.0;
            }
            let mut power_coeff_2: f64 = 0.0;
            let mut table_2 = vec![];
            let power_coeff_2: f64 = collimation / 45.0;
            for increment_2 in 0..1001 {
                let brightness: f64 = (increment_2 as f64) * 0.001;
                let mut distance_2: f64 = 0.0;
                if power_coeff_2 == 0.0 {
                    distance_2 = 4000.0;
                } else {
                    distance_2 = (brightness / 0.002).powf(1.0/power_coeff_2);
                }
                table_2.push(distance_2);
            }
            distance_tables.push(table_2);
        }
        
        BrightnessTables {
            brightness_tables: brightness_tables,
            distance_tables: distance_tables,
        }
    }
    
    pub fn read_brightness_table(&self, distance: &f64, collimation: &f64) -> f64 {
        let brightness_index: i32 = ((*distance - 1.0) * 20.0) as i32;
        let power_index: i32 = (*collimation * 2.0) as i32; 
        let mut brightness_scaling = self.brightness_tables[power_index as usize][brightness_index as usize];
        brightness_scaling
    }
    
    pub fn read_distance_table(&self, brightness: &f64, collimation: &f64) -> f64 {
        let distance_index: i32 = (*brightness * 1000.0) as i32;
        let power_index: i32 = (*collimation * 2.0) as i32; 
        let mut distance = self.distance_tables[power_index as usize][distance_index as usize];
        distance
    }
}


// Define our light field object.
#[derive(Debug)]
struct LightFieldObject {
    // LightFieldObject just stores a vector of LightFields and their associated
    // bounding corners in map space.
    light_fields: Vec<(LightField, (i32, i32), (i32, i32))>,
}

impl LightFieldObject {
    // But - the methods are what is important.
    pub fn new() -> LightFieldObject {
        let light_field: LightField = vec![vec![(0.0, 0.0, 0.0); 1 as usize]; 1 as usize];
        let map_offset_start: (i32, i32) = (0, 0);
        let map_offset_end: (i32, i32) = (0, 0);
        let light_fields: Vec<(LightField, (i32, i32), (i32, i32))> = vec![(light_field, map_offset_start, map_offset_end); 1 as usize];
        LightFieldObject {
            light_fields: light_fields,
        }
    }
    
    // Public function that can be called on an object LightFieldObject to reacalculate it's immediate
    // LightField, and if requested, iteratively calculate any resulting reflections.
    pub fn recalculate(&mut self, map: &Map, pos_x: &i32, pos_y: &i32, intensity_profile: &(f64, f64, f64), direction: &f64, angular_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) {
        // Zero the LightFieldObject vector.
        let light_field: LightField = vec![vec![(0.0, 0.0, 0.0); 1 as usize]; 1 as usize];
        let map_offset_start: (i32, i32) = (0, 0);
        let map_offset_end: (i32, i32) = (0, 0);
        self.light_fields = vec![(light_field, map_offset_start, map_offset_end); 1 as usize];
        
        // Create an empty vector buffer to hold any temporary light-source objects that will be created by any reflections
        // resulting from the first, parent pass at recalculating the object light field.
        let mut outer_buffer: Vec<Object> = vec![];
        let initial_result: (LightField, (i32, i32), (i32, i32), Vec<Object>) = self.compute_lightfield(map, pos_x, pos_y, intensity_profile, direction, angular_sweep, collimation, brightness_tables, fov_map);
        // Commit the resulting LightFields to the LightFieldObject vector.
        self.light_fields.push((initial_result.0, initial_result.1, initial_result.2));
        
        if REFLECTION_LEVEL > 0 {
            // ---------------------------------------------------------------------
            if REFLECTION_STATUS {
                println!("------------------------   Reflections 0   ------------------------");
                println!("(In buffer) {} unfiltered initial reflections.", (initial_result.3).len());
            }
            let mut resolved_objects = vec![];
            for object_outer in &(initial_result.3) {
                let mut resolved_object: (i32, i32, f64) = (0, 0, 0.0);
                if !resolved_objects.contains(&(object_outer.x, object_outer.y, object_outer.direction)) {
                    let mut found: bool = false;
                    let mut inner_x: i32 = 0;
                    let mut inner_y: i32 = 0;
                    let mut inner_direction: f64 = 0.0;
                    let mut inner_intensity: (f64, f64, f64) = (0.0, 0.0, 0.0);
                    let mut inner_sweep: f64 = 0.0;
                    let mut inner_collimation: f64 = 0.0;
                    
                    for object_inner in &(initial_result.3) {
                        if (object_outer.x == object_inner.x) && (object_outer.y == object_inner.y) && (object_outer.direction == object_inner.direction) {
                            if ((object_inner.light_source.1).0 + (object_inner.light_source.1).1 + (object_inner.light_source.1).2) > ((object_outer.light_source.1).0 + (object_outer.light_source.1).1 + (object_outer.light_source.1).2) {
                                found = true;
                                inner_x = object_inner.x;
                                inner_y = object_inner.y;
                                inner_direction = object_inner.direction;
                                inner_intensity = object_inner.light_source.1;
                                inner_sweep = object_inner.light_source.2;
                                inner_collimation = object_inner.light_source.3;
                            }
                        }
                    }
                    
                    if found {
                        outer_buffer.push(Object::new(&map, inner_x, inner_y, inner_direction, ' ', (0.0, 0.0, 0.0), (true, inner_intensity, inner_sweep, inner_collimation), true, brightness_tables, fov_map));
                        resolved_object = (inner_x, inner_y, inner_direction);
                    } else {
                        outer_buffer.push(Object::new(&map, object_outer.x, object_outer.y, object_outer.direction, ' ', (0.0, 0.0, 0.0), (true, (object_outer.light_source).1, (object_outer.light_source).2, (object_outer.light_source).3), true, brightness_tables, fov_map));
                        resolved_object = (object_outer.x, object_outer.y, object_outer.direction);
                    }
                }
                resolved_objects.push(resolved_object);
            }
            if REFLECTION_STATUS {
                println!("(Out buffer) {} filtered reflections.", outer_buffer.len());
            }
            
            // -------------------------------------------------------------------------------
            // While there are any temporary light objects in the buffer...
            let mut reflection_level_index: i32 = 0;
            'reflections: while (outer_buffer.len() > 0) && (reflection_level_index < (REFLECTION_LEVEL)) {
                if REFLECTION_STATUS {
                    println!("------------------------   Reflection level {}   ------------------------", reflection_level_index + 1);
                }
                // Zero a sub-buffer.
                let mut inner_buffer: Vec<Object> = vec![];
                // For each object in the buffer.
                let mut sub_reflection_index: i32 = 0;
                'sub_reflections: for outer_object in &outer_buffer {
                    // Calculate the immediate LightField and resulting further temporary light objects.
                    let new_result = self.compute_lightfield(map, &outer_object.x, &outer_object.y, &outer_object.light_source.1, &outer_object.direction, &outer_object.light_source.2, &outer_object.light_source.3, brightness_tables, fov_map);
                    if REFLECTION_STATUS {
                        println!("---> Reflection {}, {} : (In buffer) {} resulting unfiltered reflections.", reflection_level_index + 1, sub_reflection_index, (new_result.3).len());
                    }
                    // Commit the resulting LightField to the LightFieldObject.
                    self.light_fields.push((new_result.0, new_result.1, new_result.2));
                    // Commit the resulting temporary light objects to the sub buffer,
                    let mut resolved_objects = vec![];
                    for object_outer in &(new_result.3) {
                        let mut resolved_object: (i32, i32, f64) = (0, 0, 0.0);
                        if !resolved_objects.contains(&(object_outer.x, object_outer.y, object_outer.direction)) {
                            let mut found: bool = false;
                            let mut inner_x: i32 = 0;
                            let mut inner_y: i32 = 0;
                            let mut inner_direction: f64 = 0.0;
                            let mut inner_intensity: (f64, f64, f64) = (0.0, 0.0, 0.0);
                            let mut inner_sweep: f64 = 0.0;
                            let mut inner_collimation: f64 = 0.0;
                            
                            for object_inner in &(new_result.3) {
                                if (object_outer.x == object_inner.x) && (object_outer.y == object_inner.y) && (object_outer.direction == object_inner.direction) {
                                    if ((object_inner.light_source.1).0 + (object_inner.light_source.1).1 + (object_inner.light_source.1).2) > ((object_outer.light_source.1).0 + (object_outer.light_source.1).1 + (object_outer.light_source.1).2) {
                                        found = true;
                                        inner_x = object_inner.x;
                                        inner_y = object_inner.y;
                                        inner_direction = object_inner.direction;
                                        inner_intensity = object_inner.light_source.1;
                                        inner_sweep = object_inner.light_source.2;
                                        inner_collimation = object_inner.light_source.3;
                                    }
                                }
                            }
                            
                            if found {
                                inner_buffer.push(Object::new(&map, inner_x, inner_y, inner_direction, ' ', (0.0, 0.0, 0.0), (true, inner_intensity, inner_sweep, inner_collimation), true, brightness_tables, fov_map));
                                resolved_object = (inner_x, inner_y, inner_direction);
                            } else {
                                inner_buffer.push(Object::new(&map, object_outer.x, object_outer.y, object_outer.direction, ' ', (0.0, 0.0, 0.0), (true, (object_outer.light_source).1, (object_outer.light_source).2, (object_outer.light_source).3), true, brightness_tables, fov_map));
                                resolved_object = (object_outer.x, object_outer.y, object_outer.direction);
                            }
                        }
                        resolved_objects.push(resolved_object);
                    }
                    sub_reflection_index = sub_reflection_index + 1;
                }
                // Once this pass at the buffer is completed, refill the buffer from the sub buffer and repeat.
                if REFLECTION_STATUS {
                    println!("---> Reflection {} result : (Out buffer) {} resulting filtered reflections.", reflection_level_index + 1, inner_buffer.len());
                }
                outer_buffer = inner_buffer;
                reflection_level_index = reflection_level_index + 1;
            }
            // No more resulting reflections, we are done!
        }
    }
    
    // Private function to actually compute the LightField and reflection details.
    fn compute_lightfield(&mut self, map: &Map, pos_x: &i32, pos_y: &i32, intensity_profile: &(f64, f64, f64), direction: &f64, angular_sweep: &f64, collimation: &f64, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) -> (LightField, (i32, i32), (i32, i32), Vec<Object>) {
        let mut resulting_reflections: Vec<Object> = vec![];
        
        // Determine the maximum intensity
        let float_light_r_intensity: f64 = intensity_profile.0;
        let float_light_g_intensity: f64 = intensity_profile.1;
        let float_light_b_intensity: f64 = intensity_profile.2;
        let max_intensity: f64 = (float_light_r_intensity.max(float_light_g_intensity)).max(float_light_b_intensity);
        
        // Determine the map and field space beam radius according to the highest intensity component.
        // The origin of the 'magic number' comes from the fact that if the reference brightness is defined
        // at a distance of 1.0, then the maximum radius corresponds to the square root of the ratio of initial and 
        // minimum brightnesses. In this case the minimum brightness is 1/255 (8-bit color) = 0.0039215...
        let mut float_light_radius: f64 = brightness_tables.read_distance_table(&max_intensity, collimation);
        
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
        let map_light_coords: (i32, i32) = (*pos_x, *pos_y);
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
        
        // Light field spans 1 radius on each side of the light-source tile.
        let light_field_dimensions: (i32, i32) = ((map_offset_end.0 - map_offset_start.0) + 1, (map_offset_end.1 - map_offset_start.1) + 1);
        let mut light_field: LightField = vec![vec![(0.0, 0.0, 0.0); light_field_dimensions.1 as usize]; light_field_dimensions.0 as usize];
        
        // Recalculate FOV for the current illuminated object.
        fov_map.compute_fov(*pos_x, *pos_y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
        
        // Calculate the light-source co-ordinates in field space (as opposed to map space).
        //
        // NOTE - Adding 0.5 to each makes it easy to convert back from map space to field space - we just need to
        // add the start offset and .trunc() :D
        let field_light_coords: (f64, f64) = (((map_light_coords.0 as f64) + 0.5) - (map_offset_start.0 as f64), ((map_light_coords.1 as f64) + 0.5) - (map_offset_start.1 as f64));
        
        // Get beam sweep angle - either side of the beam centre (alpha angle = 0 deg).
        let beam_sweep: f64 = *angular_sweep;
        
        // Determine angle modifier to set 'direction'.
        // This value is subtracted from the alpha angle calculated for each target tile, in effect rotating the light source.
        let alpha_angle_modifier: f64 = *direction;
        
        // Identify target tiles along periphery of LightField. Create RAYCAST_FINENESS target points spaced 0.05 deep within the
        // inner edges of each of those targets. 
        let mut targets_list = vec![];
        for x_ind in (map_offset_start.0)..(map_offset_end.0) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((x_ind as f64) + ((1.0/(RAYCAST_FINENESS as f64))*(subray_index as f64)), (map_offset_start.1 as f64) + 0.95));
            }
        }
        for x_ind in (map_offset_start.0)..(map_offset_end.0) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((x_ind as f64) + ((1.0/(RAYCAST_FINENESS as f64))*(subray_index as f64)), (map_offset_end.1 as f64) + 0.05));
            }
        }
        for y_ind in (map_offset_start.1)..(map_offset_end.1) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((map_offset_start.0 as f64) + 0.95, (y_ind as f64) + ((1.0/(RAYCAST_FINENESS as f64))*(subray_index as f64))));
            }
        }
        for y_ind in (map_offset_start.1)..(map_offset_end.1) {
            for subray_index in 0..RAYCAST_FINENESS {
                targets_list.push(((map_offset_end.0 as f64) + 0.05, (y_ind as f64) + ((1.0/(RAYCAST_FINENESS as f64))*(subray_index as f64))));
            }
        }
        
        // Begin iterating through the list of targets.
        let mut target_index: i32 = 0;
        'target: for current_target in targets_list {
            let map_target_x_coord: f64 = current_target.0 as f64;
            let map_target_y_coord: f64 = current_target.1 as f64;
                // Get co-ordinates of target tile and distance components from light-source to
                // target tile in field space (again, adding 0.5 to make it easy to convert back to map space just
                // by adding the start offset and truncating).
                let field_target_coords: (f64, f64) = (((map_target_x_coord as f64) + 0.0) - (map_offset_start.0 as f64), ((map_target_y_coord as f64) + 0.0) - (map_offset_start.1 as f64));
                let field_light_target_dist_comps: (f64, f64) = ((field_target_coords.0 - field_light_coords.0), (field_target_coords.1 - field_light_coords.1));
                
                // Determine which quadrant the target is in with respect to alpha = 0 deg and calculate
                // the corresponding target alpha angle.
                let mut alpha_angle: f64 = 0.0;
                let atan_rad: f64 = ((field_light_target_dist_comps.1) / (field_light_target_dist_comps.0)).atan();
                let atan_deg: f64 = atan_rad.to_degrees().abs();
                if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
                    alpha_angle = atan_deg.abs();
                } else {
                    if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
                        alpha_angle = 180.0 - atan_deg.abs();
                    } else {
                        if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                            alpha_angle = atan_deg.abs() + 180.0;
                        } else {
                            if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                                alpha_angle = 360.0 - atan_deg.abs();
                            }
                        }
                    }
                }
                
                // Apply alpha angle direction modifier.
                let uncorrected_alpha_angle: f64 = alpha_angle;
                alpha_angle = alpha_angle - alpha_angle_modifier;
                if alpha_angle < 0.0 {
                    // Underflow, add 360 deg.
                    alpha_angle = alpha_angle + 360.0;
                } else {
                    if alpha_angle >= 360.0 {
                        // Overflow, subtract 360 deg.
                        alpha_angle = alpha_angle - 360.0;
                    }
                }
                
                // Check if we need to cast a ray (Is target tile within angular field of view?)
                if !((alpha_angle <= beam_sweep) || (alpha_angle >= (360.0 - beam_sweep))) {
                    // If not, stop this pass and start on next target tile in x.
                    target_index = target_index + 1;
                    continue 'target;
                }
                
                let field_light_target_distance: f64 = ((field_light_target_dist_comps.0).powi(2) + (field_light_target_dist_comps.1).powi(2)).sqrt();
                let mut field_ray_coords: (f64, f64) = (field_light_coords.0, field_light_coords.1);
                let mut field_ray_brightness: (f64, f64, f64) = (float_light_r_intensity, float_light_g_intensity, float_light_b_intensity);
                let field_dist_step: f64 = RAYCAST_DISTANCE_STEP;
                let field_dist_increments: f64 = field_light_target_distance / field_dist_step;
                let field_dist_step_comps: (f64, f64) = ((field_light_target_dist_comps.0 / field_dist_increments), (field_light_target_dist_comps.1 / field_dist_increments));
                let mut field_travelled_dist_this_target: (f64, (f64, f64)) = (0.0, (0.0, 0.0));
                
                // Shadowcasting begins!
                'ray: for increment in 0..(field_dist_increments as i32) {
                    field_ray_coords.0 += field_dist_step_comps.0;
                    field_ray_coords.1 += field_dist_step_comps.1;
                    field_travelled_dist_this_target.0 += field_dist_step;
                    (field_travelled_dist_this_target.1).0 += field_dist_step_comps.0;
                    (field_travelled_dist_this_target.1).1 += field_dist_step_comps.1;
                    
                    // Have we left the map?
                    let map_check_coords: (i32, i32) = (((field_ray_coords.0).trunc() as i32) + map_offset_start.0, ((field_ray_coords.1).trunc() as i32) + map_offset_start.1);
                    if (map_check_coords.0 < 0) || (map_check_coords.0 > (MAP_WIDTH - 1)) || (map_check_coords.1 < 0) || (map_check_coords.1 > (MAP_HEIGHT - 1)) {
                        // If so, continue with next target location.
                        target_index = target_index + 1;
                        continue 'target;
                    }
                    
                    // Reduce ray brightness -----------------------------------------------------------------------------
                    let mut modulation_distance: f64 = field_travelled_dist_this_target.0;
                    if modulation_distance < 1.0 {
                        modulation_distance = 1.0;
                    }
                    let modulation: f64 = brightness_tables.read_brightness_table(&modulation_distance, collimation);
                    field_ray_brightness.0 = float_light_r_intensity * modulation;
                    if field_ray_brightness.0 > float_light_r_intensity {
                        field_ray_brightness.0 = float_light_r_intensity;
                    }
                    field_ray_brightness.1 = float_light_g_intensity * modulation;
                    if field_ray_brightness.1 > float_light_g_intensity {
                        field_ray_brightness.1 = float_light_g_intensity;
                    }
                    field_ray_brightness.2 = float_light_b_intensity * modulation;
                    if field_ray_brightness.2 > float_light_b_intensity {
                        field_ray_brightness.2 = float_light_b_intensity;
                    }
                    
                    // NOTE - Is this a sensible place to put this?? Should I include this check at all? It serves to calm
                    // down the number of sub-reflections calculated when REFLECTION_LEVEL is set high by killing any rays
                    // with all channels darker than 0.00393 ( = 1/255).
                    if (field_ray_brightness.0 < 0.00393) && (field_ray_brightness.1 < 0.00393) && (field_ray_brightness.2 < 0.00393) {
                        continue 'target;
                    }
                    // ---------------------------------------------------------------------------------------------------
                    
                    let field_write_coords: (i32, i32) = ((field_ray_coords.0).trunc() as i32, (field_ray_coords.1).trunc() as i32);
                    if map[map_check_coords.0 as usize][map_check_coords.1 as usize].block_sight {
                        if !((map_check_coords.0 == map_light_coords.0) && (map_check_coords.1 == map_light_coords.1)) {
                            // The ray his hit an opaque thing, that is not at the same co-ordinates as the light-source.
                            //
                            // If the opaque target the ray has hit occupies a location immediately horizontally or vertically adjacent
                            // to the light source location, and if the light source location is set to block sight, do not illuminate it
                            // , or generate any reflections. This prevents light-sources embedded into the walls from illuminating the 
                            // adjacent wall tiles or creating spurious reflections inside walls. 
                            let mut adjacent_solid_light_source: bool = false;
                            if (((map_check_coords.0 - map_light_coords.0).abs() <= 1) && ((map_check_coords.1 - map_light_coords.1).abs() <= 1)) && !(((map_check_coords.0 - map_light_coords.0).abs() == 1) && ((map_check_coords.1 - map_light_coords.1).abs() == 1)) && map[map_light_coords.0 as usize][map_light_coords.1 as usize].block_sight {
                                adjacent_solid_light_source = true;
                            }
                            if !adjacent_solid_light_source {
                                if FOV_LIGHT_WALLS {
                                    if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 < field_ray_brightness.0 {
                                        light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 = field_ray_brightness.0;
                                    }
                                    if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 < field_ray_brightness.1 {
                                        light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 = field_ray_brightness.1;
                                    }
                                    if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 < field_ray_brightness.2 {
                                        light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 = field_ray_brightness.2;
                                    }
                                }
                                
                                // Generate appropriate reflection light-source object . Rather than being appended to the master
                                // objects list, this will be appended to a temporary vector which is then returned along with the
                                // object LightField and light field bounding box coordinates.
                                //let rtbm: f64 = 1.0 / ((RAYCAST_FINENESS as f64) * 6.0);
                                let rtbm: f64 = 0.1;
                                let reflection_sweep: f64 = beam_sweep;
                                let reflection_collimation: f64 = *collimation;
                                let mut reflection_direction: f64 = 0.0;
                                
                                // Determine target face with which ray has come into contact, and appropriately
                                // set ray direction modifiers which will flip either the x or y ray component.
                                let floating_target: (f64, f64) = (map_check_coords.0 as f64, map_check_coords.1 as f64);
                                let mut contact: i32 = 0;
                                if ((field_ray_coords.0 >= (floating_target.0 - (map_offset_start.0 as f64))) && (field_ray_coords.0 < (floating_target.0 - (map_offset_start.0 as f64) + 0.05))) && ((field_ray_coords.1 >= (floating_target.1 - (map_offset_start.1 as f64)) + 0.05) && (field_ray_coords.1 < (floating_target.1 - (map_offset_start.1 as f64)) + 0.95)) {
                                    contact = 2;
                                } else {
                                    if ((field_ray_coords.0 >= (floating_target.0 - (map_offset_start.0 as f64) + 0.95)) && (field_ray_coords.0 < (floating_target.0 - (map_offset_start.0 as f64) + 1.0))) && ((field_ray_coords.1 >= (floating_target.1 - (map_offset_start.1 as f64)) + 0.05) && (field_ray_coords.1 < (floating_target.1 - (map_offset_start.1 as f64)) + 0.95)) {
                                        contact = 0;
                                    } else {
                                        if ((field_ray_coords.1 >= (floating_target.1 - (map_offset_start.1 as f64))) && (field_ray_coords.1 < (floating_target.1 - (map_offset_start.1 as f64) + 0.05))) && ((field_ray_coords.0 >= (floating_target.0 - (map_offset_start.0 as f64)) + 0.05) && (field_ray_coords.0 < (floating_target.0 - (map_offset_start.0 as f64)) + 0.95)) {
                                            contact = 3;
                                        } else {
                                            if ((field_ray_coords.1 >= (floating_target.1 - (map_offset_start.1 as f64) + 0.95)) && (field_ray_coords.1 < (floating_target.1 - (map_offset_start.1 as f64) + 1.0))) && ((field_ray_coords.0 >= (floating_target.0 - (map_offset_start.0 as f64)) + 0.05) && (field_ray_coords.0 < (floating_target.0 - (map_offset_start.0 as f64)) + 0.95)) {
                                                contact = 1;
                                            }
                                        }
                                    }
                                }
                                
                                // ------- Determine ray direction: ------- 
                                let mut ray_alpha_angle: f64 = 0.0;
                                let ray_atan_rad: f64 = ((field_dist_step_comps.1) / (field_dist_step_comps.0)).atan();
                                let ray_atan_deg: f64 = (ray_atan_rad.to_degrees()).abs();
                                if contact == 0 {
                                    if field_dist_step_comps.1 < 0.0 {
                                        ray_alpha_angle = 360.0 - ray_atan_deg;
                                    } else {
                                        ray_alpha_angle = ray_atan_deg;
                                    }
                                } else {
                                    if contact == 1 {
                                        if field_dist_step_comps.0 < 0.0 {
                                            ray_alpha_angle = 180.0 - ray_atan_deg;
                                        } else {
                                            ray_alpha_angle = ray_atan_deg;
                                        }
                                    } else {
                                        if contact == 2 {
                                            if field_dist_step_comps.1 < 0.0 {
                                                ray_alpha_angle = 180.0 + ray_atan_deg;
                                            } else {
                                                ray_alpha_angle = 180.0 - ray_atan_deg;
                                            }
                                        } else {
                                            if contact == 3 {
                                                if field_dist_step_comps.0 < 0.0 {
                                                    ray_alpha_angle = 180.0 + ray_atan_deg;
                                                } else {
                                                    ray_alpha_angle = 360.0 - ray_atan_deg;
                                                }
                                            }
                                        }
                                    }
                                }
                                if ray_alpha_angle < 0.0 {
                                    ray_alpha_angle = ray_alpha_angle + 360.0;
                                } else {
                                    if ray_alpha_angle >= 360.0 {
                                        ray_alpha_angle = ray_alpha_angle - 360.0;
                                    }
                                }
                                // ------- ------- ------- ------- ------- 
                                
                                resulting_reflections.push(Object::new(&map, map_check_coords.0, map_check_coords.1, ray_alpha_angle, ' ', (0.0, 0.0, 0.0), (true, ((field_ray_brightness.0 * rtbm), (field_ray_brightness.1 * rtbm), (field_ray_brightness.2 * rtbm)), reflection_sweep, reflection_collimation), true, brightness_tables, fov_map));
                                
                                target_index = target_index + 1;
                                continue 'target;
                                // -----------------------------------------------------------------------------------------------------
                            } else {
                                continue 'ray;
                            }
                        } else {
                            // The ray has hit an opaque thing, but is still in the same map location as the light-source.
                            //
                            // This suggests we are still inside the light source. We will allow the ray to continue, but
                            // we won't light up this location (it's supposed to be opaque!).
                            continue 'ray;
                        }
                    } else {
                        // The ray is inside a location that does not block sight. If the ray brightness values in each
                        // channel are higher than the current value stored for that location in this light field, over-
                        // write them.
                        if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 < field_ray_brightness.0 {
                            light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].0 = field_ray_brightness.0;
                        }
                        if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 < field_ray_brightness.1 {
                            light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].1 = field_ray_brightness.1;
                        }
                        if light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 < field_ray_brightness.2 {
                            light_field[field_write_coords.0 as usize][field_write_coords.1 as usize].2 = field_ray_brightness.2;
                        }
                        continue 'ray;
                    }
                }
            }
        // Return the results.
        (light_field, map_offset_start, map_offset_end, resulting_reflections)
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
    light_source: (bool, (f64, f64, f64), f64, f64),
    light_field_object: LightFieldObject,
}

// Here we define the 'Object' object methods.
impl Object {
    pub fn new(map: &Map, x: i32, y: i32, direction: f64, char: char, color: (f64, f64, f64), light_source: (bool, (f64, f64, f64), f64, f64), temporary: bool, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) -> Self {
        //
        // Create the objects LightFieldObject. If the object is actually unlit, don't run the LightFieldObjects
        // compute() method just yet, so it just retains the little placeholder stub for now. We can light it up later!
        let mut light_field_object: LightFieldObject = LightFieldObject::new();
        
        // However, if it is lit, compute it's lightfield and position limits.
        if light_source.0 && !temporary {
            light_field_object.recalculate(&map, &x, &y, &light_source.1, &direction, &light_source.2, &light_source.3, brightness_tables, fov_map);
        }
        
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
    
    // Move object by dx, dy.
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) {
        if !map[(self.x + dx) as usize][(self.y + dy) as usize].blocked {
            self.x += dx;
            self.y += dy;
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
            self.light_field_object.recalculate(&map, &self.x, &self.y, &(self.light_source.1), &self.direction, &(self.light_source.2), &(self.light_source.3), brightness_tables, fov_map);
        }
    }
    
    pub fn toggle_light(&mut self, map: &Map, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) {
        if !(self.light_source.0) {
            self.light_source.0 = true;
        } else {
            self.light_source.0 = false;
        }
        // Recompute light field.
        self.light_field_object.recalculate(&map, &self.x, &self.y, &(self.light_source.1), &self.direction, &(self.light_source.2), &(self.light_source.3), brightness_tables, fov_map);
        println!("Flashlight toggled.");
    }
    
    pub fn pivot(&mut self, map: &Map, brightness_tables: &BrightnessTables, fov_map: &mut FovMap, clockwise: bool) {
        if !clockwise {
            self.direction = self.direction + 10.0;
            println!("Turned clockwise.");
        } else {
            self.direction = self.direction - 10.0;
            println!("Turned counter-clockwise.");
        }
        if self.direction < 0.0 {
            self.direction = self.direction + 360.0;
        } else {
            if self.direction > 360.0 {
                self.direction = self.direction - 360.0;
            }
        }
        // Recompute light field.
        self.light_field_object.recalculate(&map, &self.x, &self.y, &(self.light_source.1), &self.direction, &(self.light_source.2), &(self.light_source.3), brightness_tables, fov_map);
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
fn make_map(brightness_tables: &BrightnessTables) -> (Map, (i32, i32), Vec<Rect>) {
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
            map[(new_x - 1) as usize][(new_y - 1) as usize].block_sight = true;
            map[(new_x - 1) as usize][(new_y - 1) as usize].color = COLOR_WALL;
            map[(new_x + 1) as usize][(new_y - 1) as usize].blocked = true;
            map[(new_x + 1) as usize][(new_y - 1) as usize].block_sight = true;
            map[(new_x + 1) as usize][(new_y - 1) as usize].color = COLOR_WALL;
            map[(new_x - 1) as usize][(new_y + 1) as usize].blocked = true;
            map[(new_x - 1) as usize][(new_y + 1) as usize].block_sight = true;
            map[(new_x - 1) as usize][(new_y + 1) as usize].color = COLOR_WALL;
            map[(new_x + 1) as usize][(new_y + 1) as usize].blocked = true;
            map[(new_x + 1) as usize][(new_y + 1) as usize].block_sight = true;
            map[(new_x + 1) as usize][(new_y + 1) as usize].color = COLOR_WALL;
            
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
            
            //~// Add mid-side lights pointing inwards.
            //~light_sources.push(Object::new(&map, ((new_room.x2 - 1 - new_room.x1 + 1) / 2) + (new_room.x1 + 1), new_room.y1 + 0, 90.0, '*', COLOR_PLAYER, (true, torch_brightness, torch_angle * 2.0), false));
            //~light_sources.push(Object::new(&map, ((new_room.x2 - 1 - new_room.x1 + 1) / 2) + (new_room.x1 + 1), new_room.y2 - 0, 270.0, '*', COLOR_PLAYER, (true, torch_brightness, torch_angle * 2.0), false));
            //~light_sources.push(Object::new(&map, new_room.x1 + 0, ((new_room.y2 - 1 - new_room.y1 + 1) / 2) + (new_room.y1 + 1), 0.0, '*', COLOR_PLAYER, (true, torch_brightness, torch_angle * 2.0), false));
            //~light_sources.push(Object::new(&map, new_room.x2 - 0, ((new_room.y2 - 1 - new_room.y1 + 1) / 2) + (new_room.y1 + 1), 180.0, '*', COLOR_PLAYER, (true, torch_brightness, torch_angle * 2.0), false));
            
            rooms.push(new_room);
        }
    }
    
    (map, starting_position, rooms)
}


// Keystroke handler.
fn handle_keys(root: &mut Root, player: &mut Object, map: &Map, brightness_tables: &BrightnessTables, fov_map: &mut FovMap) -> bool {
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
        Key { code: Up, .. } => player.move_by(0, -1, map, brightness_tables, fov_map),
        Key { code: Down, .. } => player.move_by(0, 1, map, brightness_tables, fov_map),
        Key { code: Left, .. } => player.move_by(-1, 0, map, brightness_tables, fov_map),
        Key { code: Right, .. } => player.move_by(1, 0, map, brightness_tables, fov_map),
        Key { printable: 'a', .. } => player.pivot(map, brightness_tables, fov_map, true),
        Key { printable: 's', .. } => player.pivot(map, brightness_tables, fov_map, false),
        
        // Function keys.
        Key { printable: 'f', .. } => player.toggle_light(map, brightness_tables, fov_map),
        
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
                // When we draw tiles below, we will get ther luminance value from the
                // illumination map.
                //
                // Instead of computing it each time, we will only read the values from
                // the objects stored light_fields. These get updated whenever an object
                // moves.
                let object_light_fields = &object.light_field_object.light_fields;
                
                for current_light_field in object_light_fields {
                    let object_light_field = &current_light_field.0;
                    let map_start_offset = &current_light_field.1;
                    let map_end_offset = &current_light_field.2;
                    
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
        }
        
        // Draw all world tiles.
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
                    if not_visible_illumination.2 < MIN_NOT_VISIBLE_ILLUMINATION.2 {
                        not_visible_illumination.2 = MIN_NOT_VISIBLE_ILLUMINATION.2;
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
            display_color = correct_colors(&float_r_channel_output, &float_g_channel_output, &float_b_channel_output);
            // --------------------------------------------------------------------
                    
            object.draw(con, Color { r: (display_color.0 as u8), g: (display_color.1 as u8), b: (display_color.2 as u8) });
        }
    }
    
    // Blit the composition terminal contents into the root terminal.
    blit(con, (0,0), (SCREEN_WIDTH, SCREEN_HEIGHT), root, (0,0), 1.0, 1.0);
}


fn correct_colors(float_r_channel_output: &f64, float_g_channel_output: &f64, float_b_channel_output: &f64) -> (i32, i32, i32) {
    // -- Code to turn linearised total brightness into a log brightness --
    let a = -1.01179495;
    let b = -4.47099458;
    let c = 1.01214152;
    
    let mut display_color: (i32, i32, i32) = (0, 0, 0);
    let mut corrected_r_channel_output: f64 = (((b * float_r_channel_output).exp()) * a) + c;
    let mut corrected_g_channel_output: f64 = (((b * float_g_channel_output).exp()) * a) + c;
    let mut corrected_b_channel_output: f64 = (((b * float_b_channel_output).exp()) * a) + c;
    
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
    let (mut map, (player_x, player_y), rooms) = make_map(&brightness_tables);
    
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
    objects.push(Object::new(&map, player_x, player_y, 0.0, '@', COLOR_PLAYER, (true, (1.0, 1.0, 1.0), 30.0, 65.0), false, &brightness_tables, &mut fov_map));
    objects.push(Object::new(&map, player_x - 1, player_y-1, 0.0, 'c', COLOR_CAT_BUDDY, (false, (0.0, 0.0, 0.0), 0.0, 0.0), false, &brightness_tables, &mut fov_map));
    
    // Populate created rooms with light objects.
    for new_room in rooms {
        // Add corner lights pointing inwards diagonally.
        let directions: (f64, f64, f64, f64) = (45.0, 135.0, 225.0, 315.0);
        let torch_brightness: (f64, f64, f64) = (1.0, 0.0, 0.0);
        let torch_angle: f64 = 45.0;
        let torch_collimation: f64 = 70.0;
        objects.push(Object::new(&map, new_room.x1+1, new_room.y1+1, directions.0, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation), false, &brightness_tables, &mut fov_map));
        objects.push(Object::new(&map, new_room.x2-1, new_room.y1+1, directions.1, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation), false, &brightness_tables, &mut fov_map));
        objects.push(Object::new(&map, new_room.x2-1, new_room.y2-1, directions.2, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation), false, &brightness_tables, &mut fov_map));
        objects.push(Object::new(&map, new_room.x1+1, new_room.y2-1, directions.3, '*', torch_brightness, (true, torch_brightness, torch_angle, torch_collimation), false, &brightness_tables, &mut fov_map));
    }
    
    // Set a ficticious previous player position to make sure that fov is calculated
    // on first pass of game loop.
    let mut previous_player_position = (-1, -1);
    let mut previous_flashlight_state: bool = true;
    let mut previous_direction: f64 = 0.0;
    
    // Generate master illumination map.
    //
    // This is a vector field of (f64, f64, f64) illumination values. These are zeroed at the start of each
    // FOV update, and then all light-sources (including the ambient illumination) are summed into
    // it. Tiles and Objects are drawn with their 'lightness' value scaled according to this value
    // at their position. The values are re-scaled from the native linear 0.0 -> 1.0 to
    // log 0.0 -> 1.0.
    let mut light_field: LightField = vec![vec![(0.0, 0.0, 0.0); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
    // Main world loop.
    while !root.window_closed() {
        // Set flag to recompute fov if player position has changed.
        let fov_recompute = (previous_player_position != (objects[0].x, objects[0].y)) || (previous_flashlight_state != objects[0].light_source.0) || (objects[0].direction != previous_direction);
        
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
        previous_flashlight_state = player.light_source.0;
        
        //let exit = handle_keys(&mut root, &mut objects[0]);
        let exit = handle_keys(&mut root, player, &map, &brightness_tables, &mut fov_map);
        
        if exit {
            break
        }
    }
}
