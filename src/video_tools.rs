type LightField = Vec<Vec<(f64, f64, f64)>>;
type Map = Vec<Vec<Tile>>;

const ILLUMINATION_MODULATION: f64 = 0.5;

pub fn compute_lightfield(map: &mut Map, object: &Object) -> (LightField, (i32, i32), (i32, i32)) {
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
    let int_light_radius: i32 = float_light_radius.round() as i32;
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
    
    // Get beam sweep angle - either side of the beam centre (alpha angle = 0 deg).
    let beam_sweep: f64 = object.light_source.2;
    
    // Determine angle modifier to set 'direction'.
    //
    // This value is subtracted from the alpha angle calculated for each target tile, in effect rotating the light source.
    let mut alpha_angle_modifier: f64 = 0.0;
    if object.direction == 1 {
        alpha_angle_modifier = 0.0;
    }
    if object.direction == 2 {
        alpha_angle_modifier = 90.0;
    }
    if object.direction == 3 {
        alpha_angle_modifier = 180.0;
    }
    if object.direction == 4 {
        alpha_angle_modifier = 270.0;
    }
    
    'target_y: for map_target_y_coord in (map_offset_start.1)..(map_offset_end.1) {
        'target_x: for map_target_x_coord in (map_offset_start.0)..(map_offset_end.0) {
            total_ray_count = total_ray_count + 1;
            //println!("---------------------------------------------------------");
            
            let field_target_coords: (f64, f64) = (((map_target_x_coord as f64) + 0.5) - (map_offset_start.0 as f64), ((map_target_y_coord as f64) + 0.5) - (map_offset_start.1 as f64));
            //println!("Field target coords {} {}", field_target_coords.0, field_target_coords.1);
            
            // Get distance components to target. 
            let field_light_target_dist_comps: (f64, f64) = ((field_target_coords.0 - field_light_coords.0), (field_target_coords.1 - field_light_coords.1));
            
            // Determine which quadrant the target is in with respect to alpha = 0 deg and calculate
            // the corresponding target alpha angle.
            let mut alpha_angle: f64 = 0.0;
            let atan_rad: f64 = ((field_light_target_dist_comps.1).abs() / (field_light_target_dist_comps.0).abs()).atan();
            let atan_deg: f64 = (atan_rad / 6.28318530718) * 360.0;
            if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
                alpha_angle = atan_deg.abs();
            }
            if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 >= 0.0) {
                alpha_angle = 180.0 - atan_deg.abs();
            }
            if (field_light_target_dist_comps.0 < 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                alpha_angle = atan_deg.abs() + 180.0;
            }
            if (field_light_target_dist_comps.0 >= 0.0) && (field_light_target_dist_comps.1 < 0.0) {
                alpha_angle = 360.0 - atan_deg.abs();
            }
            
            // Apply alpha angle direction modifier.
            alpha_angle = alpha_angle - alpha_angle_modifier;
            if alpha_angle < 0.0 {
                // Underflow, add 360 deg.
                alpha_angle = alpha_angle + 360.0;
            } else {
                if alpha_angle > 360.0 {
                    // Overflow, subtract 360 deg.
                    alpha_angle = alpha_angle - 360.0;
                }
            }
            //println!("Alpha {}",alpha_angle);
            
            // Check if we need to cast a ray.
            if !((alpha_angle <= beam_sweep) || (alpha_angle >= (360.0 - beam_sweep))) {
                // If not, stop this pass and start on next target in x.
                continue 'target_x;
            }
            
            let field_light_target_distance: f64 = ((field_light_target_dist_comps.0).powi(2) + (field_light_target_dist_comps.1).powi(2)).sqrt();
            //println!("Field light -> target distance {} {} -> {}", field_light_target_dist_comps.0, field_light_target_dist_comps.1, field_light_target_distance);
            
            let mut field_ray_coords: (f64, f64) = (field_light_coords.0, field_light_coords.1);
            let mut field_ray_brightness: (f64, f64, f64) = (float_light_r_intensity, float_light_g_intensity, float_light_b_intensity);
            
            let field_dist_step: f64 = 0.05;
            let field_dist_increments: f64 = field_light_target_distance / field_dist_step;
            
            let field_dist_step_comps: (f64, f64) = ((field_light_target_dist_comps.0 / field_dist_increments), (field_light_target_dist_comps.1 / field_dist_increments));
            //println!("-> Step {} --> {} {}", field_dist_step, field_dist_step_comps.0, field_dist_step_comps.1);
            //println!("-> Increments {}", field_dist_increments);
            
            let mut field_travelled_dist_this_target: (f64, (f64, f64)) = (0.0, (0.0, 0.0));
            
            'ray: for increment in 0..(field_dist_increments as i32) {
                
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
                let mut modulation_distance = field_travelled_dist_this_target.0 * ILLUMINATION_MODULATION;
                if modulation_distance < 1.0 {
                    modulation_distance = 1.0;
                }
                
                field_ray_brightness.0 = float_light_r_intensity / ((modulation_distance).powi(2));
                if field_ray_brightness.0 > float_light_r_intensity {
                    field_ray_brightness.0 = float_light_r_intensity;
                }
                field_ray_brightness.1 = float_light_g_intensity / ((modulation_distance).powi(2));
                if field_ray_brightness.1 > float_light_g_intensity {
                    field_ray_brightness.1 = float_light_g_intensity;
                }
                field_ray_brightness.2 = float_light_b_intensity / ((modulation_distance).powi(2));
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
