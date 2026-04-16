use crate::{
    arena_settings::ArenaSettings,
    map::Map,
    math::{Position, PositionUnit},
    rng::VieRng,
    ship::ShipKind,
};

pub fn generate_spawn_position(
    settings: &ArenaSettings,
    map: &Map,
    ship: ShipKind,
    frequency: u16,
    rng: VieRng,
    player_count: usize,
) -> Position {
    let mut spawn_count: usize = 0;
    let mut rng = rng;

    for i in 0..settings.spawn_settings.len() {
        if settings.spawn_settings[i].x != 0
            || settings.spawn_settings[i].y != 0
            || settings.spawn_settings[i].radius != 0
        {
            spawn_count += 1;
        }
    }

    let ship_radius = settings.get_ship_settings(ship).get_radius();
    let mut result;

    if spawn_count == 0 {
        result = Position::new(PositionUnit(512), PositionUnit(512));

        for _ in 0..100 {
            let x;
            let y;

            match settings.radar_mode {
                1 | 3 => {
                    let rng_x = rng.next();
                    let rng_y = rng.next();

                    x = (frequency as u32 & 1) * 0x300 + rng_x;
                    y = rng_y + 0x100;
                }
                2 | 4 => {
                    let rng_x = rng.next();
                    let rng_y = rng.next();

                    x = (frequency as u32 & 1) * 0x300 + rng_x;
                    y = (((frequency as u32) / 2) & 1) * 0x300 + rng_y;
                }
                _ => {
                    let mut spawn_radius =
                        (((player_count as u32) / 8) * 0x200 + 0x400) / 0x60 + 0x100;

                    if spawn_radius > settings.warp_radius_limit as u32 {
                        spawn_radius = settings.warp_radius_limit as u32;
                    }

                    if spawn_radius < 3 {
                        spawn_radius = 3;
                    }

                    let offset_x = rng.next() % 0x14;
                    let offset_y = rng.next() % 0x14;

                    x = (rng.next() % (spawn_radius - 2))
                        .wrapping_sub(9)
                        .wrapping_add(((0x400 - spawn_radius) / 2) + offset_x);
                    y = (rng.next() % (spawn_radius - 2))
                        .wrapping_sub(9)
                        .wrapping_add(((0x400 - spawn_radius) / 2) + offset_y);
                }
            }

            if map.can_fit(x as u16, y as u16, ship_radius, frequency) {
                result = Position::new(PositionUnit(x as i32), PositionUnit(y as i32));
                break;
            }
        }
    } else {
        let spawn_index = frequency as usize % spawn_count;

        let mut x_center = settings.spawn_settings[spawn_index].x;
        let mut y_center = settings.spawn_settings[spawn_index].y;
        let radius = settings.spawn_settings[spawn_index].radius;

        if x_center == 0 {
            x_center = 512;
        } else if x_center < 0 {
            x_center += 1024;
        }

        if y_center == 0 {
            y_center = 512;
        } else {
            y_center += 1024;
        }

        result = Position::new(PositionUnit(x_center as i32), PositionUnit(y_center as i32));

        if radius > 0 {
            for _ in 0..100 {
                let xrand = rng.next();
                let yrand = rng.next();

                let x_offset = ((xrand % (radius as u32 * 2)) as i32) - radius as i32;
                let y_offset = ((yrand % (radius as u32 * 2)) as i32) - radius as i32;

                let x = x_center as i32 + x_offset;
                let y = y_center as i32 + y_offset;

                if map.can_fit(x as u16, y as u16, ship_radius, frequency) {
                    result = Position::new(PositionUnit(x), PositionUnit(y));
                    break;
                }
            }
        }
    }

    result
}
