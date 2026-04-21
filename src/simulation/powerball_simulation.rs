use crate::{
    map::{Map, TILE_ID_GOAL},
    powerball::{Powerball, is_team_goal},
};

pub fn integrate_powerball(
    map: &Map,
    powerball_mode: u8,
    powerball_bounce: bool,
    powerball: &mut Powerball,
) -> bool {
    if powerball.remaining_pickup_ticks > 0 {
        powerball.remaining_pickup_ticks -= 1;
    }

    if powerball.friction > 0 {
        let prev_x = powerball.position.x;
        powerball.position.x = powerball.position.x + powerball.velocity.x;

        if powerball.position.x.0 <= 0
            || powerball.position.x.0 >= crate::math::MAX_POSITION
            || (powerball_bounce && map.is_solid_position(powerball.position))
        {
            powerball.position.x = prev_x;
            powerball.velocity.x.0 *= -1;
        }

        let prev_y = powerball.position.y;
        powerball.position.y = powerball.position.y + powerball.velocity.y;

        if powerball.position.y.0 <= 0
            || powerball.position.y.0 >= crate::math::MAX_POSITION
            || (powerball_bounce && map.is_solid_position(powerball.position))
        {
            powerball.position.y = prev_y;
            powerball.velocity.y.0 *= -1;
        }

        let friction = (powerball.friction / 1000) as i32;

        powerball.velocity.x.0 = (powerball.velocity.x.0 * friction) / 1000;
        powerball.velocity.y.0 = (powerball.velocity.y.0 * friction) / 1000;

        powerball.friction = powerball
            .friction
            .saturating_sub(powerball.friction_delta as u32);
    }

    let tile_id = map.get_tile(
        (powerball.position.x.0 / 16000) as u16,
        (powerball.position.y.0 / 16000) as u16,
    );

    if tile_id == TILE_ID_GOAL
        && !is_team_goal(powerball_mode, powerball.position, powerball.frequency)
    {
        return true;
    }

    false
}
