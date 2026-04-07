use crate::{
    arena_settings::ArenaSettings,
    map::Map,
    math::{PixelUnit, Position, PositionUnit, Velocity},
    player::Player,
    ship::ShipKind,
};

pub fn integrate_player(map: &Map, settings: &ArenaSettings, player: &mut Player) {
    if player.ship_kind == ShipKind::Spectator {
        return;
    }

    let mut bounce_factor = settings.bounce_factor as i32;

    if bounce_factor == 0 {
        bounce_factor = 16;
    }

    let radius = settings.get_ship_settings(player.ship_kind).radius;

    let mut delta_x = player.velocity.x.0;
    let prev_x = player.position.x;
    player.position.x = player.position.x + player.velocity.x;

    if player.lerp_remaining_ticks > 0 {
        player.position.x = player.position.x + player.lerp_velocity.x;
        delta_x += player.lerp_velocity.x.0;
    }

    let sign_x: i32 = if delta_x >= 0 { 1 } else { -1 };
    let check_x = player.position.x + PixelUnit(radius as i32 * sign_x).into();

    if check_x.0 <= 0
        || check_x.0 >= crate::math::MAX_POSITION
        || map.is_solid_position(Position::new(check_x, player.position.y))
    {
        player.position.x = prev_x;
        player.velocity.x = PositionUnit((-player.velocity.x.0 * 16) / bounce_factor);
        player.velocity.y = PositionUnit((player.velocity.y.0 * 16) / bounce_factor);
    }

    let mut delta_y = player.velocity.y.0;
    let prev_y = player.position.y;
    player.position.y = player.position.y + player.velocity.y;

    if player.lerp_remaining_ticks > 0 {
        player.position.y = player.position.y + player.lerp_velocity.y;
        delta_y += player.lerp_velocity.y.0;
    }

    let sign_y: i32 = if delta_y >= 0 { 1 } else { -1 };
    let check_y = player.position.y + PixelUnit(radius as i32 * sign_y).into();

    if check_y.0 <= 0
        || check_y.0 >= crate::math::MAX_POSITION
        || map.is_solid_position(Position::new(player.position.x, check_y))
    {
        player.position.y = prev_y;
        player.velocity.x = PositionUnit((player.velocity.x.0 * 16) / bounce_factor);
        player.velocity.y = PositionUnit((-player.velocity.y.0 * 16) / bounce_factor);
    }

    if player.lerp_remaining_ticks > 0 {
        player.lerp_remaining_ticks -= 1;
    }
}

pub fn update_player_lerp_target(
    player: &mut Player,
    position: Position,
    map: &Map,
    settings: &ArenaSettings,
    sim_ticks: i32,
) {
    let mut projected_player = player.clone();

    projected_player.position = position;
    projected_player.lerp_remaining_ticks = 0;

    for _ in 0..sim_ticks {
        integrate_player(map, settings, &mut projected_player);
    }

    let position_offset = Position::new(
        projected_player.position.x - player.position.x,
        projected_player.position.y - player.position.y,
    );
    if position_offset.x.0.abs() > 64000 || position_offset.y.0.abs() > 64000 {
        player.position = projected_player.position;
        player.lerp_remaining_ticks = 0;
    } else {
        player.lerp_remaining_ticks = 20;

        let vel_x = position_offset.x.0 / player.lerp_remaining_ticks as i32;
        let vel_y = position_offset.y.0 / player.lerp_remaining_ticks as i32;

        player.lerp_velocity = Velocity::new(PositionUnit(vel_x), PositionUnit(vel_y));
    }
}
