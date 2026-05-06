use crate::{
    arena_settings::ArenaSettings,
    map::{Map, TILE_ID_SAFE},
    math::{PixelUnit, Position, PositionUnit, Velocity},
    player::{Player, StatusFlags},
    ship::ShipKind,
};

pub const PLAYER_FLASH_DURATION: u32 = 54;
pub const PLAYER_EXPLOSION_DURATION: u32 = 72;

pub fn integrate_player(map: &Map, settings: &ArenaSettings, player: &mut Player) {
    if player.status & StatusFlags::Flash != 0 {
        player.status &= !StatusFlags::Flash;
        player.flash_remaining_ticks = PLAYER_FLASH_DURATION;
    }

    if player.flash_remaining_ticks > 0 {
        player.flash_remaining_ticks -= 1;
    }

    if player.explosion_remaining_ticks > 0 {
        player.explosion_remaining_ticks -= 1;
    }

    if player.enter_delay > 0 {
        player.enter_delay = player.enter_delay.saturating_sub(1);

        if settings.enter_delay > 0 && player.enter_delay < settings.enter_delay as u16 {
            player.position = None;
        }
    }

    if player.ship_kind == ShipKind::Spectator {
        return;
    }

    let Some(mut player_position) = player.position else {
        return;
    };

    let mut bounce_factor = settings.bounce_factor as i32;

    if bounce_factor == 0 {
        bounce_factor = 16;
    }

    let radius = settings.get_ship_settings(player.ship_kind).get_radius();

    let mut delta_x = player.velocity.x.0;
    let prev_x = player_position.x;
    player_position.x = player_position.x + player.velocity.x;

    if player.lerp_remaining_ticks > 0 {
        player_position.x = player_position.x + player.lerp_velocity.x;
        delta_x += player.lerp_velocity.x.0;
    }

    let sign_x: i32 = if delta_x >= 0 { 1 } else { -1 };
    let check_x = player_position.x + PixelUnit(radius as i32 * sign_x).into();

    let start_tile_y = (player_position.y.0 - radius as i32 * 1000) / 16000;
    let end_tile_y: i32 = (player_position.y.0 + radius as i32 * 1000) / 16000;

    for y in start_tile_y..=end_tile_y {
        if map.is_solid((check_x.0 / 16000) as u16, y as u16, player.frequency) {
            player_position.x = prev_x;
            player.velocity.x = PositionUnit((-player.velocity.x.0 * 16) / bounce_factor);
            player.velocity.y = PositionUnit((player.velocity.y.0 * 16) / bounce_factor);

            player.lerp_velocity.x = PositionUnit((-player.lerp_velocity.x.0 * 16) / bounce_factor);
            player.lerp_velocity.y = PositionUnit((player.lerp_velocity.y.0 * 16) / bounce_factor);
            break;
        }
    }

    let mut delta_y = player.velocity.y.0;
    let prev_y = player_position.y;
    player_position.y = player_position.y + player.velocity.y;

    if player.lerp_remaining_ticks > 0 {
        player_position.y = player_position.y + player.lerp_velocity.y;
        delta_y += player.lerp_velocity.y.0;
    }

    let sign_y: i32 = if delta_y >= 0 { 1 } else { -1 };
    let check_y = player_position.y + PixelUnit(radius as i32 * sign_y).into();

    let start_tile_x = (player_position.x.0 - radius as i32 * 1000) / 16000;
    let end_tile_x: i32 = (player_position.x.0 + radius as i32 * 1000) / 16000;

    for x in start_tile_x..=end_tile_x {
        if map.is_solid(x as u16, (check_y.0 / 16000) as u16, player.frequency) {
            player_position.y = prev_y;
            player.velocity.x = PositionUnit((player.velocity.x.0 * 16) / bounce_factor);
            player.velocity.y = PositionUnit((-player.velocity.y.0 * 16) / bounce_factor);

            player.lerp_velocity.x = PositionUnit((player.lerp_velocity.x.0 * 16) / bounce_factor);
            player.lerp_velocity.y = PositionUnit((-player.lerp_velocity.y.0 * 16) / bounce_factor);
            break;
        }
    }

    if player.lerp_remaining_ticks > 0 {
        player.lerp_remaining_ticks -= 1;
    }

    if map.get_tile_from_position(&player_position) == TILE_ID_SAFE {
        player.status |= StatusFlags::Safety;
    } else {
        player.status &= !StatusFlags::Safety;
    }

    player.position = Some(player_position);
}

pub fn update_player_lerp_target(
    player: &mut Player,
    position: Position,
    map: &Map,
    settings: &ArenaSettings,
    sim_ticks: i32,
) {
    let mut projected_player = player.clone();

    projected_player.position = Some(position);
    projected_player.lerp_remaining_ticks = 0;
    projected_player.explosion_remaining_ticks = 0;

    for _ in 0..sim_ticks {
        integrate_player(map, settings, &mut projected_player);
    }

    let Some(player_position) = player.position else {
        player.position = projected_player.position;
        player.velocity = projected_player.velocity;
        player.lerp_remaining_ticks = 0;
        return;
    };

    let position_offset = Position::new(
        projected_player.position.unwrap().x - player_position.x,
        projected_player.position.unwrap().y - player_position.y,
    );

    if position_offset.x.0.abs() > 64000
        || position_offset.y.0.abs() > 64000
        || player.status & StatusFlags::Flash != 0
    {
        player.position = projected_player.position;
        player.velocity = projected_player.velocity;
        player.lerp_remaining_ticks = 0;
    } else {
        player.lerp_remaining_ticks = 20;

        let vel_x = position_offset.x.0 / player.lerp_remaining_ticks as i32;
        let vel_y = position_offset.y.0 / player.lerp_remaining_ticks as i32;

        player.lerp_velocity = Velocity::new(PositionUnit(vel_x), PositionUnit(vel_y));
    }
}
