use crate::math::Position;

pub fn is_team_goal(powerball_mode: u8, position: Position, frequency: u16) -> bool {
    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match powerball_mode {
        0 => false,
        1 => {
            if frequency & 1 != 0 {
                x >= 512
            } else {
                x < 512
            }
        }
        2 => {
            if frequency & 1 != 0 {
                y >= 512
            } else {
                y < 512
            }
        }
        3 => is_team_goal_mode3(position, frequency),
        4 => !is_team_goal_mode3(position, frequency),
        5 => is_team_goal_mode5(position, frequency),
        6 => !is_team_goal_mode5(position, frequency),
        _ => false,
    }
}

fn is_team_goal_mode3(position: Position, frequency: u16) -> bool {
    let corner = frequency % 4;

    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match corner {
        0 => x < 512 && y < 512,
        1 => y >= 512 && y < 512,
        2 => x < 512 && y >= 512,
        3 => x >= 512 && y >= 512,
        _ => false,
    }
}

fn is_team_goal_mode5(position: Position, frequency: u16) -> bool {
    let direction = frequency % 4;

    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match direction {
        0 => {
            if y < 512 {
                x < y
            } else {
                x + y < 1024
            }
        }
        1 => {
            if x < 512 {
                x + y >= 1024
            } else {
                x < y
            }
        }
        2 => {
            if x < 512 {
                x >= y
            } else {
                x + y < 1024
            }
        }
        3 => {
            if y <= 512 {
                x + y >= 1024
            } else {
                x >= y
            }
        }
        _ => false,
    }
}
