use crate::{
    arena_settings::ArenaSettings,
    player::{PlayerId, PlayerManager},
    ship::ShipKind,
    ship_controller::ShipController,
};

#[derive(Copy, Clone)]
pub enum AttachKind {
    DetachSelf,
    DetachChildren,
    Attach(PlayerId),
}

#[derive(Copy, Clone)]
pub enum AttachError {
    // Self PlayerId does not exist in arena.
    InvalidSelf,
    // Target PlayerId does not exist in arena.
    InvalidTarget,
    Energy,
    Bounty,
    SelfAttach,
    Frequency,
    Spectator,
    TargetShip,
    TooManyTurrets,
}

impl AttachError {
    pub fn get_notification_string(&self) -> &'static str {
        match self {
            Self::InvalidSelf => "",
            Self::InvalidTarget => "",
            Self::Energy => "Not enough energy to attach.",
            Self::Bounty => "Bounty not high enough to attach.",
            Self::SelfAttach => "Cannot attach to self",
            Self::Frequency => "Must attach to somebody of same frequency.",
            Self::Spectator => "Cannot attach to spectator.",
            Self::TargetShip => "Target ship is not attachable.",
            Self::TooManyTurrets => "Too many turrets already attached.",
        }
    }
}

pub fn can_attach_to(
    player_manager: &PlayerManager,
    ship_controller: &ShipController,
    settings: &ArenaSettings,
    target_id: PlayerId,
) -> Result<AttachKind, AttachError> {
    let (already_attached, has_children) = if let Some(me) = player_manager.get_self() {
        (me.attach_parent.valid(), !me.children.is_empty())
    } else {
        return Err(AttachError::InvalidSelf);
    };

    if !target_id.valid() || already_attached {
        return Ok(AttachKind::DetachSelf);
    }

    if has_children {
        return Ok(AttachKind::DetachChildren);
    }

    let target_id = if let Some(target) = player_manager.get_by_id(target_id) {
        if target.attach_parent.valid() {
            target.attach_parent
        } else {
            target_id
        }
    } else {
        return Err(AttachError::InvalidTarget);
    };

    if ship_controller.ship.current_energy < ship_controller.ship.max_energy {
        return Err(AttachError::Energy);
    }

    let Some(me) = player_manager.get_self() else {
        return Err(AttachError::InvalidSelf);
    };

    if me.id == target_id {
        return Err(AttachError::SelfAttach);
    }

    if me.ship_kind == ShipKind::Spectator {
        return Err(AttachError::Spectator);
    }

    if me.bounty < settings.get_ship_settings(me.ship_kind).attach_bounty {
        return Err(AttachError::Bounty);
    }

    let Some(target) = player_manager.get_by_id(target_id) else {
        return Err(AttachError::InvalidTarget);
    };

    if target.frequency != me.frequency {
        return Err(AttachError::Frequency);
    }

    if target.ship_kind == ShipKind::Spectator {
        return Err(AttachError::Spectator);
    }

    let target_turret_limit = settings.get_ship_settings(target.ship_kind).turret_limit;

    if target_turret_limit == 0 {
        return Err(AttachError::TargetShip);
    }

    if target.children.len() as u8 >= target_turret_limit {
        return Err(AttachError::TooManyTurrets);
    }

    Ok(AttachKind::Attach(target_id))
}
