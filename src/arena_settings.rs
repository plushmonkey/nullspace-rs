use crate::ship::ShipKind;
use std::mem::{self, MaybeUninit};

#[derive(Debug, Copy, Clone)]
pub struct ShipSettings {
    // How long Super lasts on the ship (in ticks)
    pub super_time: u32,
    // How long Shield lasts on the ship (in ticks)
    pub shield_time: u32,
    // How strong of an effect the wormhole has on this ship (0 = none)
    pub gravity: i16,
    // Ship are allowed to move faster than their maximum speed while
    // effected by a wormhole.This determines how much faster they can
    // go (0 = no extra speed)
    pub gravity_top_speed: i16,
    // Amount of energy it takes a ship to fire a single L1 bullet
    pub bullet_fire_energy: u16,
    // Amount of energy it takes a ship to fire multifire L1 bullets
    pub multi_fire_energy: u16,
    // Amount of energy it takes a ship to fire a single bomb
    pub bomb_fire_energy: u16,
    // Extra amount of energy it takes a ship to fire an upgraded bomb.
    // i.e. L2 = BombFireEnergy + BombFireEnergyUpgrade
    pub bomb_fire_energy_upgrade: u16,
    // Amount of energy it takes a ship to place a single L1 mine
    pub mine_fire_energy: u16,
    // Extra amount of energy it takes to place an upgraded landmine.
    // i.e. L2 = LandmineFireEnergy + LandmineFireEnergyUpgrade
    pub mine_fire_energy_upgrade: u16,
    // How fast bullets travel (Pixels / second / 10)
    pub bullet_speed: i16,
    // How fast bombs travel
    pub bomb_speed: i16,
    // If ship can see bombs on radar(0 = Disabled, 1 = All, 2 = L2 and up,
    // 3 = L3 and up, 4 = L4 bombs only)
    pub see_bomb_level: u16,
    // If firing bullets, bombs, or thors is disabled after using afterburners
    // (1 = enabled)
    pub disable_fast_shooting: bool,
    // The ship's radius from center to outside, in pixels.
    pub radius: u16,
    // Angle spread between multi-fire bullets and standard forward
    // firing bullets(111 = 1 degree, 1000 = 1 ship - rotation - point)
    pub multi_fire_angle: u16,
    // Amount of energy required to have 'Cloak' activated (thousanths per tick)
    pub cloak_energy: u16,
    // Amount of energy required to have 'Stealth' activated (thousanths per tick)
    pub stealth_energy: u16,
    // Amount of energy required to have 'Anti-Warp' activated (thousanths per
    // tick)
    pub antiwarp_energy: u16,
    // Amount of energy required to have 'X-Radar' activated (thousanths per tick)
    pub xradar_energy: u16,
    // Maximum rotation rate of the ship (0 = can't rotate, 400 = full rotation in
    // 1 second)
    pub maximum_rotation: u16,
    // Maximum thrust of ship (0 = none)
    pub maximum_thrust: u16,
    // Maximum speed of ship (0 = can't move)
    pub maximum_speed: u16,
    // Maximum recharge rate, or how quickly this ship recharges its energy
    pub maximum_recharge: u16,
    // Maximum amount of energy that the ship can have
    pub maximum_energy: u16,
    // Initial rotation rate of the ship (0 = can't rotate, 400 = full rotation in
    // 1 second)
    pub initial_rotation: u16,
    // Initial thrust of ship (0 = none)
    pub initial_thrust: u16,
    // Initial speed of ship (0 = can't move)
    pub initial_speed: u16,
    // Initial recharge rate, or how quickly this ship recharges its energy
    pub initial_recharge: u16,
    // Initial amount of energy that the ship can have
    pub initial_energy: u16,
    // Amount added per 'Rotation' Prize
    pub upgrade_rotation: u16,
    // Amount added per 'Thruster' Prize
    pub upgrade_thrust: u16,
    // Amount added per 'Speed' Prize
    pub upgrade_speed: u16,
    // Amount added per 'Recharge Rate' Prize
    pub upgrade_recharge: u16,
    // Amount added per 'Energy Upgrade' Prize
    pub upgrade_energy: u16,
    // Amount of energy required to have 'Afterburners' activated
    pub afterburner_energy: u16,
    // Amount of back-thrust you receive when firing a bomb
    pub bomb_thrust: u16,
    // How fast the burst shrapnel is for this ship
    pub burst_speed: u16,
    // Amount the ship's thrust is decreased with a turret riding
    pub turret_thrust_penalty: i16,
    // Amount the ship's speed is decreased with a turret riding
    pub turret_speed_penalty: i16,
    // Delay that ship waits after a bullet is fired until another
    // weapon may be fired(in ticks)
    pub bullet_fire_delay: u16,
    // Delay that ship waits after a multifire bullet is fired until
    // another weapon may be fired(in ticks)
    pub multi_fire_delay: u16,
    // delay that ship waits after a bomb is fired until another weapon
    // may be fired(in ticks)
    pub bomb_fire_delay: u16,
    // Delay that ship waits after a mine is fired until another weapon
    // may be fired(in ticks)
    pub mine_fire_delay: u16,
    // How long a Rocket lasts (in ticks)
    pub rocket_time: u16,
    // Number of 'Greens' given to ships when they start
    pub initial_bounty: u16,
    // How likely a the ship is to take damamage (ie. lose a prize)
    // (0 = special - case-never, 1 = extremely likely, 5000 = almost never)
    pub damage_factor: u16,
    // Maximum bounty that ships receive Team Prizes
    pub prize_share_limit: u16,
    // Bounty required by ships to attach as a turret
    pub attach_bounty: u16,
    // Time player has to carry soccer ball (in ticks)
    pub powerball_throw_timer: u16,
    // Amount the friction on the soccer ball (how quickly it slows down
    // --higher numbers mean faster slowdown)
    pub powerball_friction: u16,
    // How close the player must be in order to pick up ball (in pixels)
    pub powerball_proximity: u16,
    // Initial speed given to the ball when fired by the carrier
    pub powerball_speed: u16,
    // Number of turrets allowed on a ship
    pub turret_limit: u8,
    // Number of bullets released when a 'Burst' is activated
    pub burst_shrapnel: u8,
    // Maximum number of mines allowed in ships
    pub max_mines: u8,
    // Maximum number of Repels allowed in ships
    pub max_repel: u8,
    // Maximum number of Bursts allowed in ships
    pub max_burst: u8,
    // Maximum number of Decoys allowed in ships
    pub max_decoy: u8,
    // Maximum number of Thor's Hammers allowed in ships
    pub max_thor: u8,
    // Maximum number of Bricks allowed in ships
    pub max_brick: u8,
    // Maximum number of Rockets allowed in ships
    pub max_rocket: u8,
    // Maximum number of Portals allowed in ships
    pub max_portal: u8,
    // Initial number of Repels given to ships when they start
    pub initial_repel: u8,
    // Initial number of Bursts given to ships when they start
    pub initial_burst: u8,
    // Initial number of Bricks given to ships when they start
    pub initial_brick: u8,
    // Initial number of Rockets given to ships when they start
    pub initial_rocket: u8,
    // Initial number of Thor's Hammers given to ships when they start
    pub initial_thor: u8,
    // Initial number of Decoys given to ships when they start
    pub initial_decoy: u8,
    // Initial number of Portals given to ships when they start
    pub initial_portal: u8,
    // Number of times a ship's bombs bounce before they explode on impact
    pub bomb_bounce_count: u8,

    pub max_shrapnel: u8,
    pub shrapnel_rate: u8,
    pub cloak_status: u8,
    pub stealth_status: u8,
    pub xradar_status: u8,
    pub antiwarp_status: u8,
    pub initial_guns: u8,
    pub max_guns: u8,
    pub initial_bombs: u8,
    pub max_bombs: u8,
    pub double_barrel: bool,
    pub emp_bomb: bool,
    pub see_mines: bool,
}

// Structure to define the starting coordinates for teams 0-3
#[derive(Debug, Copy, Clone)]
pub struct SpawnSettings {
    // X Coordinate for the center point where this team will start
    pub x: i16,
    // Y Coordinate for the center point where this team will start
    pub y: i16,
    // How large of a circle from the center point this team can start
    pub radius: u16,
}

// Likelihood of each prize appearing
#[derive(Debug, Copy, Clone)]
pub struct PrizeWeightSettings {
    pub recharge: u8,
    pub energy: u8,
    pub rotation: u8,
    pub stealth: u8,
    pub cloak: u8,
    pub xradar: u8,
    pub warp: u8,
    pub gun: u8,
    pub bomb: u8,
    pub bouncing_bullets: u8,
    pub thruster: u8,
    pub top_speed: u8,
    pub full_charge: u8,
    pub engine_shutdown: u8,
    pub multi_fire: u8,
    pub proximity: u8,
    pub ship_super: u8,
    pub shields: u8,
    pub shrapnel: u8,
    pub anti_warp: u8,
    pub repel: u8,
    pub burst: u8,
    pub decoy: u8,
    pub thor: u8,
    pub multi_prize: u8,
    pub brick: u8,
    pub rocket: u8,
    pub portal: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct ArenaSettings {
    // This is necessary for computing checksums.
    pub raw_bytes: [u8; 1428],

    // Whether to use exact bullet damage
    pub exact_damage: bool,
    // Whether to show dropped flags to spectators
    pub no_spec_flags: bool,
    // Whether spectators are disallowed from having X radar
    pub no_spec_xradar: bool,

    pub slow_framerate: u8,
    // Whether to disable Continuum's screenshot feature
    pub disable_screenshot: bool,

    pub max_timer_drift: u8,
    // Whether to disable ball-passing through walls
    pub disable_ball_through_walls: bool,
    // Whether to disable ball killing in safe zones
    pub disable_ball_killing: bool,

    pub ship_settings: [ShipSettings; 8],
    // Maximum amount of damage that a L1 bullet will cause
    pub bullet_damage_level: i32,
    // Amount of damage a bomb causes at its center point (for all bomb levels)
    pub bomb_damage_level: i32,
    // How long bullets live before disappearing (in ticks)
    pub bullet_alive_time: i32,
    // Time bomb is alive (in ticks)
    pub bomb_alive_time: i32,
    // Time a decoy is alive (in ticks)
    pub decoy_alive_time: i32,
    // Amount of time that can be spent in the safe zone (in ticks)
    pub safety_limit: i32,
    // Amount of random frequency shift applied to sounds in the game
    pub frequency_shift: i32,
    // One more than the highest frequency allowed
    pub max_frequency: i32,
    // Speed at which players are repelled
    pub repel_speed: i32,
    // Time that mines are active (in ticks)
    pub mine_alive_time: i32,
    // Maximum amount of damage caused by a single burst bullet
    pub burst_damage_level: i32,
    // Amount of extra damage each bullet level will cause
    pub bullet_damage_upgrade: i32,
    // Time before flag is dropped by carrier (0=never)
    pub flag_drop_delay: i32,
    // Time a new player must wait before they are allowed to see flags
    pub enter_game_flagging_delay: i32,
    // Thrust value given while a rocket is active
    pub rocket_thrust: i32,
    // Speed value given while a rocket is active
    pub rocket_speed: i32,
    // Amount of damage shrapnel causes in its first 1/4 second of life
    pub inactive_shrap_damage: i32,
    // How often the wormhole switches its destination
    pub wormhole_switch_time: i32,
    // Amount of time a ship is shutdown after application is reactivated
    pub activate_app_shutdown_time: i32,
    // Speed that shrapnel travels
    pub shrapnel_speed: i32,

    pub spawn_settings: [SpawnSettings; 4],
    // Percentage of the ping time that is spent on the C2S portion of the
    // ping(used in more accurately syncronizing clocks)
    pub send_route_percent: i16,
    // How long after the proximity sensor is triggered before bomb explodes
    pub bomb_explode_delay: i16,
    // Amount of time between position packets sent by client
    pub send_position_delay: i16,
    // Blast radius in pixels for an L1 bomb (L2 bombs double this, L3 bombs
    // triple this)
    pub bomb_explode_pixels: i16,
    // How long the prize exists that appears after killing somebody
    pub death_prize_time: i16,
    // How long the screen jitters from a bomb hit (in ticks)
    pub jitter_time: i16,
    // How long after a player dies before he can re-enter the game (in ticks)
    pub enter_delay: i16,
    // Time the player is affected by an 'Engine Shutdown' Prize (in ticks)
    pub engine_shutdown_time: i16,
    // Radius of proximity trigger in tiles (each bomb level adds 1 to this
    // amount)
    pub proximity_distance: i16,
    // Number of points added to players bounty each time he kills an opponent
    pub bounty_increase_for_kill: i16,
    // How bouncy the walls are (16 = no speed loss)
    pub bounce_factor: i16,
    // A number representing how much the map is zoomed out for radar.
    // (48 = whole map on radar, 49 + = effectively disable radar)
    pub map_zoom_factor: i16,

    pub max_bonus: i16,

    pub max_penalty: i16,

    pub reward_base: i16,
    // Time players are affected by the repel (in ticks)
    pub repel_time: i16,
    // Number of pixels from the player that are affected by a repel
    pub repel_distance: i16,
    // Amount of time between ticker help messages
    pub ticker_delay: i16,
    // Whether the flaggers appear on radar in red
    pub flagger_on_radar: i16,
    // Number of times more points are given to a flagger (1 = double points, 2 =
    // triple points)
    pub flagger_kill_multiplier: i16,
    // Number of prizes hidden is based on number of players in game.
    // This number adjusts the formula, higher numbers mean more prizes.
    // (Note: 10000 is max, 10 greens per person)
    pub prize_factor: i16,
    // How often prizes are regenerated (in ticks)
    pub prize_delay: i16,
    // Distance from center of arena that prizes/flags/soccer-balls will spawn
    pub minimum_virtual: i16,
    // Amount of additional distance added to MinimumVirtual for each player that
    // is in the game
    pub upgrade_virtual: i16,
    // Maximum amount of time that a hidden prize will remain on screen. (actual
    // time is random)
    pub prize_max_exist: i16,
    // Minimum amount of time that a hidden prize will remain on screen. (actual
    // time is random)
    pub prize_min_exist: i16,
    //  Odds of getting a negative prize.  (1 = every prize, 32000 = extremely
    //  rare)
    pub prize_negative_factor: i16,
    // How often doors attempt to switch their state
    pub door_delay: i16,
    // Distance Anti-Warp affects other players (in pixels) (note: enemy must also
    // be on radar)
    pub antiwarp_pixels: i16,
    // Door mode (-2=all doors completely random, -1=weighted random
    // (some doors open more often than others), 0 - 255 = fixed doors (1
    // bit of byte for each door specifying whether it is open or not)
    pub door_mode: i16,
    // Amount of time that a user can get no data from server before flags are
    // hidden from view for 10 seconds
    pub flag_blank_delay: i16,
    // Amount of time that a user can get no data from server before flags he is
    // carrying are dropped
    pub no_data_flag_drop_delay: i16,
    // Number of random greens given with a MultiPrize
    pub multi_prize_count: i16,
    // How long bricks last (in ticks)
    pub brick_time: i16,
    // When ships are randomly placed in the arena, this parameter will
    // limit how far from the center of the arena they can be placed
    // (1024 = anywhere)
    pub warp_radius_limit: i16,
    // Maximum time recharge is stopped on players hit with an EMP bomb
    pub ebomb_shutdown_time: i16,
    // Percentage of normal damage applied to an EMP bomb (in 0.1%)
    pub ebomb_damage_percent: i16,
    // Size of area between blinded radar zones (in pixels)
    pub radar_neutral_size: i16,
    // How long a portal is active
    pub warp_point_delay: i16,
    // Amount of energy that constitutes a near-death experience (ships
    // bounty will be decreased by 1 when this occurs -- used for dueling zone)
    pub near_death_level: i16,
    // Percentage of normal damage applied to a bouncing bomb (in 0.1%)
    pub bouncing_bomb_damage_percent: i16,
    // Percentage of normal damage applied to shrapnel (relative to bullets of
    // same level) (in 0.1%)
    pub shrapnel_damage_percent: i16,
    // Amount of latency S2C that constitutes a slow packet
    pub client_slow_packet_time: i16,
    // Minimum kill reward that a player must get in order to have his flag drop
    // timer reset
    pub flag_drop_reset_reward: i16,
    // Percentage of normal weapon firing cost for flaggers (in 0.1%)
    pub flagger_fire_cost_percent: i16,
    // Percentage of normal damage received by flaggers (in 0.1%)
    pub flagger_damage_percent: i16,
    // Delay given to flaggers for firing bombs (zero is ships normal firing rate)
    // (do not set this number less than 20)
    pub flagger_bomb_fire_delay: i16,
    // How long after the ball is fired before anybody can pick it up (in ticks)
    pub powerball_pass_delay: i16,
    // Amount of time a player can receive no data from server and still pick up
    // the soccer ball
    pub powerball_blank_delay: i16,
    // Amount of time a user can receive no data from server before connection is
    // terminated
    pub s2c_no_data_kickout_delay: i16,
    // Amount of thrust adjustment player carrying flag gets (negative numbers
    // mean less thrust)
    pub flagger_thrust_adjustment: i16,
    // Amount of speed adjustment player carrying flag gets (negative numbers mean
    // slower)
    pub flagger_speed_adjustment: i16,
    // Number of packets to sample S2C before checking for kickout
    pub client_slow_packet_sample_size: i16,

    // Whether shrapnel spreads in circular or random patterns
    pub shrapnel_random: bool,
    // Whether the ball bounces off walls
    pub powerball_bounce: bool,
    // Whether the ball carrier can fire his bombs
    pub powerball_bomb_allowed: bool,
    // Whether the ball carrier can fire his guns
    pub powerball_gun_allowed: bool,
    // Goal configuration ($GOAL_ALL, $GOAL_LEFTRIGHT, $GOAL_TOPBOTTOM,
    // $GOAL_CORNERS_3_1, $GOAL_CORNERS_1_3, $GOAL_SIDES_3_1,
    // $GOAL_SIDES_1_3)
    pub powerball_mode: u8,
    // Maximum number of people on a public team
    pub max_per_team: u8,
    // Maximum number of people on a private team
    pub max_per_private_team: u8,
    // Maximum number of mines allowed to be placed by an entire team
    pub team_max_mines: u8,
    // Whether a wormhole affects bombs
    pub gravity_bombs: bool,
    // Whether proximity bombs have a firing safety.  If enemy ship is
    // within proximity radius, will it allow you to fire
    pub bomb_safety: bool,
    // Whether chat packets are sent reliably (C2S)
    pub message_reliable: bool,
    // Whether prize packets are sent reliably (C2S)
    pub take_prize_reliable: bool,
    // Whether players can send audio messages
    pub allow_audio_messages: bool,
    // Number of prizes that are regenerated every PrizeDelay
    pub prize_hide_count: u8,
    // Whether regular players receive sysop data about a ship
    pub extra_position_data: bool,
    // Whether to check for slow frames on the client (possible cheat
    // technique) (flawed on some machines, do not use)
    pub slow_frame_check: bool,
    // Whether the flags can be picked up and carried
    // (0=no, 1=yes, 2 = yes - one at a time, 3 = yes - two at a time, 4 = three,
    // etc..)
    pub carry_flags: u8,
    // Whether saved ships are allowed (do not allow saved ship in zones where sub
    // - arenas may have differing parameters)
    pub allow_saved_ships: bool,
    // Radar mode (0=normal, 1=half/half, 2=quarters, 3=half/half-see team mates,
    // 4 = quarters - see team mates)
    pub radar_mode: u8,
    // Whether the zone plays victory music or not
    pub victory_music: bool,
    // Whether the flaggers get a gun upgrade
    pub flagger_gun_upgrade: bool,
    // Whether the flaggers get a bomb upgrade
    pub flagger_bomb_upgrade: bool,
    // If player with soccer ball should use the Flag:Flagger* ship adjustments or
    // not
    pub powerball_flag_upgrades: bool,
    // Whether the balls location is displayed at all times or not
    pub powerball_global_position: bool,
    // How many ticks to activate a fake antiwarp after attaching, portaling, or
    // warping
    pub antiwarp_settle_delay: u8,

    pub prize_weights: PrizeWeightSettings,
}

impl SpawnSettings {
    pub fn parse(data: &[u8]) -> Option<SpawnSettings> {
        if data.len() != 4 {
            return None;
        }

        let bytes = u32::from_le_bytes(data.try_into().unwrap());

        let x = ((bytes >> 0) & 0x3FF) as i16;
        let y = ((bytes >> 10) & 0x3FF) as i16;
        let radius = ((bytes >> 20) & 0x1FF) as u16;

        Some(SpawnSettings { x, y, radius })
    }
}

impl PrizeWeightSettings {
    pub fn parse(data: &[u8]) -> Option<PrizeWeightSettings> {
        if data.len() != 28 {
            return None;
        }

        Some(PrizeWeightSettings {
            recharge: data[0],
            energy: data[1],
            rotation: data[2],
            stealth: data[3],
            cloak: data[4],
            xradar: data[5],
            warp: data[6],
            gun: data[7],
            bomb: data[8],
            bouncing_bullets: data[9],
            thruster: data[10],
            top_speed: data[11],
            full_charge: data[12],
            engine_shutdown: data[13],
            multi_fire: data[14],
            proximity: data[15],
            ship_super: data[16],
            shields: data[17],
            shrapnel: data[18],
            anti_warp: data[19],
            repel: data[20],
            burst: data[21],
            decoy: data[22],
            thor: data[23],
            multi_prize: data[24],
            brick: data[25],
            rocket: data[26],
            portal: data[27],
        })
    }

    pub fn get_weights(&self) -> [u8; 28] {
        [
            self.recharge,
            self.energy,
            self.rotation,
            self.stealth,
            self.cloak,
            self.xradar,
            self.warp,
            self.gun,
            self.bomb,
            self.bouncing_bullets,
            self.thruster,
            self.top_speed,
            self.full_charge,
            self.engine_shutdown,
            self.multi_fire,
            self.proximity,
            self.ship_super,
            self.shields,
            self.shrapnel,
            self.anti_warp,
            self.repel,
            self.burst,
            self.decoy,
            self.thor,
            self.multi_fire,
            self.brick,
            self.rocket,
            self.portal,
        ]
    }

    pub fn calculate_total_weight(&self) -> u32 {
        let mut total = 0;

        total += self.recharge as u32;
        total += self.energy as u32;
        total += self.rotation as u32;
        total += self.stealth as u32;
        total += self.cloak as u32;
        total += self.xradar as u32;
        total += self.warp as u32;
        total += self.gun as u32;
        total += self.bomb as u32;
        total += self.bouncing_bullets as u32;
        total += self.thruster as u32;
        total += self.top_speed as u32;
        total += self.full_charge as u32;
        total += self.engine_shutdown as u32;
        total += self.multi_fire as u32;
        total += self.proximity as u32;
        total += self.ship_super as u32;
        total += self.shields as u32;
        total += self.shrapnel as u32;
        total += self.anti_warp as u32;
        total += self.repel as u32;
        total += self.burst as u32;
        total += self.decoy as u32;
        total += self.thor as u32;
        total += self.multi_prize as u32;
        total += self.brick as u32;
        total += self.rocket as u32;
        total += self.portal as u32;

        total
    }
}

impl ShipSettings {
    pub fn parse(data: &[u8]) -> Option<ShipSettings> {
        if data.len() != 144 {
            return None;
        }

        let super_time = u32::from_le_bytes(data[..4].try_into().unwrap());
        let shield_time = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let gravity = i16::from_le_bytes(data[8..10].try_into().unwrap());
        let gravity_top_speed = i16::from_le_bytes(data[10..12].try_into().unwrap());
        let bullet_fire_energy = u16::from_le_bytes(data[12..14].try_into().unwrap());
        let multi_fire_energy = u16::from_le_bytes(data[14..16].try_into().unwrap());
        let bomb_fire_energy = u16::from_le_bytes(data[16..18].try_into().unwrap());
        let bomb_fire_energy_upgrade = u16::from_le_bytes(data[18..20].try_into().unwrap());
        let mine_fire_energy = u16::from_le_bytes(data[20..22].try_into().unwrap());
        let mine_fire_energy_upgrade = u16::from_le_bytes(data[22..24].try_into().unwrap());
        let bullet_speed = i16::from_le_bytes(data[24..26].try_into().unwrap());
        let bomb_speed = i16::from_le_bytes(data[26..28].try_into().unwrap());

        let packed = i16::from_le_bytes(data[28..30].try_into().unwrap());
        let see_bomb_level = ((packed >> 0) & 0x03) as u16;
        let disable_fast_shooting = ((packed >> 2) & 1) != 0;
        let radius = ((packed >> 3) & 0xFF) as u16;

        let multi_fire_angle = u16::from_le_bytes(data[30..32].try_into().unwrap());
        let cloak_energy = u16::from_le_bytes(data[32..34].try_into().unwrap());
        let stealth_energy = u16::from_le_bytes(data[34..36].try_into().unwrap());
        let antiwarp_energy = u16::from_le_bytes(data[36..38].try_into().unwrap());
        let xradar_energy = u16::from_le_bytes(data[38..40].try_into().unwrap());
        let maximum_rotation = u16::from_le_bytes(data[40..42].try_into().unwrap());
        let maximum_thrust = u16::from_le_bytes(data[42..44].try_into().unwrap());
        let maximum_speed = u16::from_le_bytes(data[44..46].try_into().unwrap());
        let maximum_recharge = u16::from_le_bytes(data[46..48].try_into().unwrap());
        let maximum_energy = u16::from_le_bytes(data[48..50].try_into().unwrap());
        let initial_rotation = u16::from_le_bytes(data[50..52].try_into().unwrap());
        let initial_thrust = u16::from_le_bytes(data[52..54].try_into().unwrap());
        let initial_speed = u16::from_le_bytes(data[54..56].try_into().unwrap());
        let initial_recharge = u16::from_le_bytes(data[56..58].try_into().unwrap());
        let initial_energy = u16::from_le_bytes(data[58..60].try_into().unwrap());
        let upgrade_rotation = u16::from_le_bytes(data[60..62].try_into().unwrap());
        let upgrade_thrust = u16::from_le_bytes(data[62..64].try_into().unwrap());
        let upgrade_speed = u16::from_le_bytes(data[64..66].try_into().unwrap());
        let upgrade_recharge = u16::from_le_bytes(data[66..68].try_into().unwrap());
        let upgrade_energy = u16::from_le_bytes(data[68..70].try_into().unwrap());
        let afterburner_energy = u16::from_le_bytes(data[70..72].try_into().unwrap());
        let bomb_thrust = u16::from_le_bytes(data[72..74].try_into().unwrap());
        let burst_speed = u16::from_le_bytes(data[74..76].try_into().unwrap());
        let turret_thrust_penalty = i16::from_le_bytes(data[76..78].try_into().unwrap());
        let turret_speed_penalty = i16::from_le_bytes(data[78..80].try_into().unwrap());
        let bullet_fire_delay = u16::from_le_bytes(data[80..82].try_into().unwrap());
        let multi_fire_delay = u16::from_le_bytes(data[82..84].try_into().unwrap());
        let bomb_fire_delay = u16::from_le_bytes(data[84..86].try_into().unwrap());
        let mine_fire_delay = u16::from_le_bytes(data[86..88].try_into().unwrap());
        let rocket_time = u16::from_le_bytes(data[88..90].try_into().unwrap());
        let initial_bounty = u16::from_le_bytes(data[90..92].try_into().unwrap());
        let damage_factor = u16::from_le_bytes(data[92..94].try_into().unwrap());
        let prize_share_limit = u16::from_le_bytes(data[94..96].try_into().unwrap());
        let attach_bounty = u16::from_le_bytes(data[96..98].try_into().unwrap());
        let powerball_throw_timer = u16::from_le_bytes(data[98..100].try_into().unwrap());
        let powerball_friction = u16::from_le_bytes(data[100..102].try_into().unwrap());
        let powerball_proximity = u16::from_le_bytes(data[102..104].try_into().unwrap());
        let powerball_speed = u16::from_le_bytes(data[104..106].try_into().unwrap());
        let turret_limit = data[106] as u8;
        let burst_shrapnel = data[107] as u8;
        let max_mines = data[108] as u8;
        let max_repel = data[109] as u8;
        let max_burst = data[110] as u8;
        let max_decoy = data[111] as u8;
        let max_thor = data[112] as u8;
        let max_brick = data[113] as u8;
        let max_rocket = data[114] as u8;
        let max_portal = data[115] as u8;
        let initial_repel = data[116] as u8;
        let initial_burst = data[117] as u8;
        let initial_brick = data[118] as u8;
        let initial_rocket = data[119] as u8;
        let initial_thor = data[120] as u8;
        let initial_decoy = data[121] as u8;
        let initial_portal = data[122] as u8;
        let bomb_bounce_count = data[123] as u8;

        let packed = u32::from_le_bytes(data[124..128].try_into().unwrap());
        let max_shrapnel = ((packed >> 0) & 0x1F) as u8;
        let shrapnel_rate = ((packed >> 5) & 0x1F) as u8;
        let cloak_status = ((packed >> 10) & 0x03) as u8;
        let stealth_status = ((packed >> 12) & 0x03) as u8;
        let xradar_status = ((packed >> 14) & 0x03) as u8;
        let antiwarp_status = ((packed >> 16) & 0x03) as u8;
        let initial_guns = ((packed >> 18) & 0x03) as u8;
        let max_guns = ((packed >> 20) & 0x03) as u8;
        let initial_bombs = ((packed >> 22) & 0x03) as u8;
        let max_bombs = ((packed >> 24) & 0x03) as u8;
        let double_barrel = ((packed >> 26) & 1) != 0;
        let emp_bomb = ((packed >> 27) & 1) != 0;
        let see_mines = ((packed >> 28) & 1) != 0;

        Some(ShipSettings {
            super_time,
            shield_time,
            gravity,
            gravity_top_speed,
            bullet_fire_energy,
            multi_fire_energy,
            bomb_fire_energy,
            bomb_fire_energy_upgrade,
            mine_fire_energy,
            mine_fire_energy_upgrade,
            bullet_speed,
            bomb_speed,
            see_bomb_level,
            disable_fast_shooting,
            radius,
            multi_fire_angle,
            cloak_energy,
            stealth_energy,
            antiwarp_energy,
            xradar_energy,
            maximum_rotation,
            maximum_thrust,
            maximum_speed,
            maximum_recharge,
            maximum_energy,
            initial_rotation,
            initial_thrust,
            initial_speed,
            initial_recharge,
            initial_energy,
            upgrade_rotation,
            upgrade_thrust,
            upgrade_speed,
            upgrade_recharge,
            upgrade_energy,
            afterburner_energy,
            bomb_thrust,
            burst_speed,
            turret_thrust_penalty,
            turret_speed_penalty,
            bullet_fire_delay,
            multi_fire_delay,
            bomb_fire_delay,
            mine_fire_delay,
            rocket_time,
            initial_bounty,
            damage_factor,
            prize_share_limit,
            attach_bounty,
            powerball_throw_timer,
            powerball_friction,
            powerball_proximity,
            powerball_speed,
            turret_limit,
            burst_shrapnel,
            max_mines,
            max_repel,
            max_burst,
            max_decoy,
            max_thor,
            max_brick,
            max_rocket,
            max_portal,
            initial_repel,
            initial_burst,
            initial_brick,
            initial_rocket,
            initial_thor,
            initial_decoy,
            initial_portal,
            bomb_bounce_count,
            max_shrapnel,
            shrapnel_rate,
            cloak_status,
            stealth_status,
            xradar_status,
            antiwarp_status,
            initial_guns,
            max_guns,
            initial_bombs,
            max_bombs,
            double_barrel,
            emp_bomb,
            see_mines,
        })
    }
}

impl Default for ArenaSettings {
    fn default() -> Self {
        const DEFAULT_SETTINGS: [u8; 1428] = [
            15, 1, 7, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0,
            14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 34, 1, 19,
            0, 178, 12, 126, 4, 164, 6, 34, 1, 18, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 0, 0, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0, 125, 0, 232,
            3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3, 3,
            3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 72, 80, 181, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0, 14,
            1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0, 17, 0,
            166, 14, 126, 4, 164, 6, 200, 0, 17, 0, 116, 14, 232, 3, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0, 125, 0,
            232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3,
            3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 72, 80, 189, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0,
            14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0, 18,
            0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 76, 4, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0, 125, 0,
            232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3,
            3, 3, 3, 3, 0, 0, 0, 0, 0, 1, 0, 0, 72, 100, 53, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0,
            14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0, 18,
            0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0, 125, 0,
            232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3,
            3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 72, 80, 245, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0,
            14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0, 18,
            0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 29, 0, 50, 0, 150, 0, 125, 0,
            232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3,
            3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 72, 80, 53, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50, 0,
            14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0, 18,
            0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0, 250, 0,
            166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0, 125, 0,
            232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5, 3, 3, 3,
            3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 64, 80, 181, 26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1, 50,
            0, 14, 1, 150, 0, 208, 7, 213, 7, 0, 0, 244, 1, 100, 0, 77, 1, 100, 0, 250, 0, 230, 0,
            18, 0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0,
            250, 0, 166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 24, 0, 50, 0, 150, 0,
            125, 0, 232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5,
            3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 2, 72, 80, 181, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 112, 23, 0, 0, 160, 15, 0, 0, 220, 5, 100, 0, 20, 0, 30, 0, 44, 1,
            50, 0, 44, 1, 150, 0, 208, 7, 213, 7, 1, 0, 244, 1, 44, 1, 44, 1, 100, 0, 250, 0, 4, 1,
            18, 0, 178, 12, 126, 4, 164, 6, 200, 0, 17, 0, 184, 11, 232, 3, 64, 6, 40, 0, 2, 0,
            250, 0, 166, 0, 100, 0, 176, 4, 144, 1, 184, 11, 1, 0, 125, 0, 16, 0, 40, 0, 150, 0,
            125, 0, 232, 3, 75, 0, 44, 1, 100, 0, 80, 0, 224, 46, 12, 0, 64, 0, 196, 9, 10, 24, 5,
            3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 72, 80, 53, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 112, 17, 1, 0, 176, 113, 11, 0, 38, 2, 0, 0, 112, 23, 0, 0, 184,
            11, 0, 0, 64, 105, 71, 0, 132, 3, 0, 0, 15, 39, 0, 0, 104, 16, 0, 0, 224, 46, 0, 0, 48,
            87, 5, 0, 88, 21, 1, 0, 80, 70, 0, 0, 112, 23, 0, 0, 25, 0, 0, 0, 148, 17, 0, 0, 184,
            11, 0, 0, 232, 3, 0, 0, 0, 0, 0, 0, 184, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 244, 1, 185, 0, 10, 0, 80, 0, 32, 3, 72, 0, 200, 0, 188, 2, 3, 0, 20, 0,
            22, 0, 10, 0, 15, 0, 0, 0, 0, 0, 39, 1, 0, 2, 232, 3, 1, 0, 0, 0, 48, 12, 44, 1, 0, 1,
            0, 0, 112, 23, 160, 15, 32, 78, 88, 2, 176, 4, 254, 255, 200, 0, 200, 0, 10, 0, 224,
            46, 192, 1, 144, 1, 232, 3, 128, 0, 112, 23, 0, 0, 232, 3, 232, 3, 50, 0, 90, 0, 226,
            4, 232, 3, 244, 1, 20, 0, 100, 0, 208, 7, 0, 0, 0, 0, 44, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 1, 1, 3, 4, 2, 12, 1, 0, 0, 0, 1, 85, 1, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0,
            0, 0, 0, 0, 0, 70, 90, 50, 40, 30, 20, 5, 60, 60, 40, 80, 70, 60, 3, 30, 40, 5, 2, 60,
            10, 40, 5, 10, 15, 20, 10, 10, 30,
        ];

        ArenaSettings::parse(&DEFAULT_SETTINGS).expect("Default settings should parse correctly.")
    }
}

impl ArenaSettings {
    pub fn parse(data: &[u8]) -> Option<ArenaSettings> {
        if data.len() < 1428 {
            return None;
        }

        let packed = u32::from_le_bytes(data[..4].try_into().unwrap());

        let exact_damage = ((packed >> 8) & 1) != 0;
        let no_spec_flags = ((packed >> 9) & 1) != 0;
        let no_spec_xradar = ((packed >> 10) & 1) != 0;
        let slow_framerate = ((packed >> 11) & 7) as u8;
        let disable_screenshot = ((packed >> 14) & 1) != 0;
        let max_timer_drift = ((packed >> 16) & 7) as u8;
        let disable_ball_through_walls = ((packed >> 19) & 1) != 0;
        let disable_ball_killing = ((packed >> 20) & 1) != 0;

        let mut offset = 4;
        const SHIP_SETTINGS_SIZE: usize = 144;

        let ship_settings = {
            let mut ship_settings: [MaybeUninit<ShipSettings>; 8] =
                [const { MaybeUninit::uninit() }; 8];

            for i in 0..8 {
                let current = ShipSettings::parse(&data[offset..offset + SHIP_SETTINGS_SIZE]);
                if let None = current {
                    return None;
                }

                ship_settings[i].write(current.unwrap());

                offset += SHIP_SETTINGS_SIZE;
            }

            unsafe { mem::transmute::<_, [ShipSettings; 8]>(ship_settings) }
        };

        let bullet_damage_level = i32::from_le_bytes(data[1156..1160].try_into().unwrap());
        let bomb_damage_level = i32::from_le_bytes(data[1160..1164].try_into().unwrap());
        let bullet_alive_time = i32::from_le_bytes(data[1164..1168].try_into().unwrap());
        let bomb_alive_time = i32::from_le_bytes(data[1168..1172].try_into().unwrap());
        let decoy_alive_time = i32::from_le_bytes(data[1172..1176].try_into().unwrap());
        let safety_limit = i32::from_le_bytes(data[1176..1180].try_into().unwrap());
        let frequency_shift = i32::from_le_bytes(data[1180..1184].try_into().unwrap());
        let max_frequency = i32::from_le_bytes(data[1184..1188].try_into().unwrap());
        let repel_speed = i32::from_le_bytes(data[1188..1192].try_into().unwrap());
        let mine_alive_time = i32::from_le_bytes(data[1192..1196].try_into().unwrap());
        let burst_damage_level = i32::from_le_bytes(data[1196..1200].try_into().unwrap());
        let bullet_damage_upgrade = i32::from_le_bytes(data[1200..1204].try_into().unwrap());
        let flag_drop_delay = i32::from_le_bytes(data[1204..1208].try_into().unwrap());
        let enter_game_flagging_delay = i32::from_le_bytes(data[1208..1212].try_into().unwrap());
        let rocket_thrust = i32::from_le_bytes(data[1212..1216].try_into().unwrap());
        let rocket_speed = i32::from_le_bytes(data[1216..1220].try_into().unwrap());
        let inactive_shrap_damage = i32::from_le_bytes(data[1220..1224].try_into().unwrap());
        let wormhole_switch_time = i32::from_le_bytes(data[1224..1228].try_into().unwrap());
        let activate_app_shutdown_time = i32::from_le_bytes(data[1228..1232].try_into().unwrap());
        let shrapnel_speed = i32::from_le_bytes(data[1232..1236].try_into().unwrap());

        let spawn_settings_0 = SpawnSettings::parse(&data[1236..1240]).unwrap();
        let spawn_settings_1 = SpawnSettings::parse(&data[1240..1244]).unwrap();
        let spawn_settings_2 = SpawnSettings::parse(&data[1244..1248]).unwrap();
        let spawn_settings_3 = SpawnSettings::parse(&data[1248..1252]).unwrap();
        let spawn_settings = [
            spawn_settings_0,
            spawn_settings_1,
            spawn_settings_2,
            spawn_settings_3,
        ];

        let send_route_percent = i16::from_le_bytes(data[1252..1254].try_into().unwrap());
        let bomb_explode_delay = i16::from_le_bytes(data[1254..1256].try_into().unwrap());
        let send_position_delay = i16::from_le_bytes(data[1256..1258].try_into().unwrap());
        let bomb_explode_pixels = i16::from_le_bytes(data[1258..1260].try_into().unwrap());
        let death_prize_time = i16::from_le_bytes(data[1260..1262].try_into().unwrap());
        let jitter_time = i16::from_le_bytes(data[1262..1264].try_into().unwrap());
        let enter_delay = i16::from_le_bytes(data[1264..1266].try_into().unwrap());
        let engine_shutdown_time = i16::from_le_bytes(data[1266..1268].try_into().unwrap());
        let proximity_distance = i16::from_le_bytes(data[1268..1270].try_into().unwrap());
        let bounty_increase_for_kill = i16::from_le_bytes(data[1270..1272].try_into().unwrap());
        let bounce_factor = i16::from_le_bytes(data[1272..1274].try_into().unwrap());
        let map_zoom_factor = i16::from_le_bytes(data[1274..1276].try_into().unwrap());
        let max_bonus = i16::from_le_bytes(data[1276..1278].try_into().unwrap());
        let max_penalty = i16::from_le_bytes(data[1278..1280].try_into().unwrap());
        let reward_base = i16::from_le_bytes(data[1280..1282].try_into().unwrap());
        let repel_time = i16::from_le_bytes(data[1282..1284].try_into().unwrap());
        let repel_distance = i16::from_le_bytes(data[1284..1286].try_into().unwrap());
        let ticker_delay = i16::from_le_bytes(data[1286..1288].try_into().unwrap());
        let flagger_on_radar = i16::from_le_bytes(data[1288..1290].try_into().unwrap());
        let flagger_kill_multiplier = i16::from_le_bytes(data[1290..1292].try_into().unwrap());
        let prize_factor = i16::from_le_bytes(data[1292..1294].try_into().unwrap());
        let prize_delay = i16::from_le_bytes(data[1294..1296].try_into().unwrap());
        let minimum_virtual = i16::from_le_bytes(data[1296..1298].try_into().unwrap());
        let upgrade_virtual = i16::from_le_bytes(data[1298..1300].try_into().unwrap());

        let prize_max_exist = i16::from_le_bytes(data[1300..1302].try_into().unwrap());
        let prize_min_exist = i16::from_le_bytes(data[1302..1304].try_into().unwrap());
        let prize_negative_factor = i16::from_le_bytes(data[1304..1306].try_into().unwrap());
        let door_delay = i16::from_le_bytes(data[1306..1308].try_into().unwrap());
        let antiwarp_pixels = i16::from_le_bytes(data[1308..1310].try_into().unwrap());
        let door_mode = i16::from_le_bytes(data[1310..1312].try_into().unwrap());
        let flag_blank_delay = i16::from_le_bytes(data[1312..1314].try_into().unwrap());
        let no_data_flag_drop_delay = i16::from_le_bytes(data[1314..1316].try_into().unwrap());
        let multi_prize_count = i16::from_le_bytes(data[1316..1318].try_into().unwrap());
        let brick_time = i16::from_le_bytes(data[1318..1320].try_into().unwrap());
        let warp_radius_limit = i16::from_le_bytes(data[1320..1322].try_into().unwrap());
        let ebomb_shutdown_time = i16::from_le_bytes(data[1322..1324].try_into().unwrap());
        let ebomb_damage_percent = i16::from_le_bytes(data[1324..1326].try_into().unwrap());
        let radar_neutral_size = i16::from_le_bytes(data[1326..1328].try_into().unwrap());
        let warp_point_delay = i16::from_le_bytes(data[1328..1330].try_into().unwrap());
        let near_death_level = i16::from_le_bytes(data[1330..1332].try_into().unwrap());
        let bouncing_bomb_damage_percent = i16::from_le_bytes(data[1332..1334].try_into().unwrap());
        let shrapnel_damage_percent = i16::from_le_bytes(data[1334..1336].try_into().unwrap());
        let client_slow_packet_time = i16::from_le_bytes(data[1336..1338].try_into().unwrap());
        let flag_drop_reset_reward = i16::from_le_bytes(data[1338..1340].try_into().unwrap());
        let flagger_fire_cost_percent = i16::from_le_bytes(data[1340..1342].try_into().unwrap());
        let flagger_damage_percent = i16::from_le_bytes(data[1342..1344].try_into().unwrap());
        let flagger_bomb_fire_delay = i16::from_le_bytes(data[1344..1346].try_into().unwrap());
        let powerball_pass_delay = i16::from_le_bytes(data[1346..1348].try_into().unwrap());
        let powerball_blank_delay = i16::from_le_bytes(data[1348..1350].try_into().unwrap());
        let s2c_no_data_kickout_delay = i16::from_le_bytes(data[1350..1352].try_into().unwrap());
        let flagger_thrust_adjustment = i16::from_le_bytes(data[1352..1354].try_into().unwrap());
        let flagger_speed_adjustment = i16::from_le_bytes(data[1354..1356].try_into().unwrap());
        let client_slow_packet_sample_size =
            i16::from_le_bytes(data[1356..1358].try_into().unwrap());

        let shrapnel_random = (data[1368] & 1) != 0;
        let powerball_bounce = (data[1369] & 1) != 0;
        let powerball_bomb_allowed = (data[1370] & 1) != 0;
        let powerball_gun_allowed = (data[1371] & 1) != 0;
        let powerball_mode = data[1372] as u8;
        let max_per_team = data[1373] as u8;
        let max_per_private_team = data[1374] as u8;
        let team_max_mines = data[1375] as u8;
        let gravity_bombs = (data[1376] & 1) != 0;
        let bomb_safety = (data[1377] & 1) != 0;
        let message_reliable = (data[1378] & 1) != 0;
        let take_prize_reliable = (data[1379] & 1) != 0;
        let allow_audio_messages = (data[1380] & 1) != 0;
        let prize_hide_count = data[1381] as u8;
        let extra_position_data = (data[1382] & 1) != 0;
        let slow_frame_check = (data[1383] & 1) != 0;
        let carry_flags = data[1384] as u8;
        let allow_saved_ships = (data[1385] & 1) != 0;
        let radar_mode = data[1386] as u8;
        let victory_music = (data[1387] & 1) != 0;
        let flagger_gun_upgrade = (data[1388] & 1) != 0;
        let flagger_bomb_upgrade = (data[1389] & 1) != 0;
        let powerball_flag_upgrades = (data[1390] & 1) != 0;
        let powerball_global_position = (data[1391] & 1) != 0;
        let antiwarp_settle_delay = data[1392] as u8;

        let prize_weights = PrizeWeightSettings::parse(&data[1400..1428]).unwrap();

        Some(ArenaSettings {
            raw_bytes: data.try_into().unwrap(),
            exact_damage,
            no_spec_flags,
            no_spec_xradar,
            slow_framerate,
            disable_screenshot,
            max_timer_drift,
            disable_ball_through_walls,
            disable_ball_killing,
            ship_settings,
            bullet_damage_level,
            bomb_damage_level,
            bullet_alive_time,
            bomb_alive_time,
            decoy_alive_time,
            safety_limit,
            frequency_shift,
            max_frequency,
            repel_speed,
            mine_alive_time,
            burst_damage_level,
            bullet_damage_upgrade,
            flag_drop_delay,
            enter_game_flagging_delay,
            rocket_thrust,
            rocket_speed,
            inactive_shrap_damage,
            wormhole_switch_time,
            activate_app_shutdown_time,
            shrapnel_speed,
            spawn_settings,
            send_route_percent,
            bomb_explode_delay,
            send_position_delay,
            bomb_explode_pixels,
            death_prize_time,
            jitter_time,
            enter_delay,
            engine_shutdown_time,
            proximity_distance,
            bounty_increase_for_kill,
            bounce_factor,
            map_zoom_factor,
            max_bonus,
            max_penalty,
            reward_base,
            repel_time,
            repel_distance,
            ticker_delay,
            flagger_on_radar,
            flagger_kill_multiplier,
            prize_factor,
            prize_delay,
            minimum_virtual,
            upgrade_virtual,
            prize_max_exist,
            prize_min_exist,
            prize_negative_factor,
            door_delay,
            antiwarp_pixels,
            door_mode,
            flag_blank_delay,
            no_data_flag_drop_delay,
            multi_prize_count,
            brick_time,
            warp_radius_limit,
            ebomb_shutdown_time,
            ebomb_damage_percent,
            radar_neutral_size,
            warp_point_delay,
            near_death_level,
            bouncing_bomb_damage_percent,
            shrapnel_damage_percent,
            client_slow_packet_time,
            flag_drop_reset_reward,
            flagger_fire_cost_percent,
            flagger_damage_percent,
            flagger_bomb_fire_delay,
            powerball_pass_delay,
            powerball_blank_delay,
            s2c_no_data_kickout_delay,
            flagger_thrust_adjustment,
            flagger_speed_adjustment,
            client_slow_packet_sample_size,
            shrapnel_random,
            powerball_bounce,
            powerball_bomb_allowed,
            powerball_gun_allowed,
            powerball_mode,
            max_per_team,
            max_per_private_team,
            team_max_mines,
            gravity_bombs,
            bomb_safety,
            message_reliable,
            take_prize_reliable,
            allow_audio_messages,
            prize_hide_count,
            extra_position_data,
            slow_frame_check,
            carry_flags,
            allow_saved_ships,
            radar_mode,
            victory_music,
            flagger_gun_upgrade,
            flagger_bomb_upgrade,
            powerball_flag_upgrades,
            powerball_global_position,
            antiwarp_settle_delay,
            prize_weights,
        })
    }

    pub fn get_ship_settings(&self, ship: ShipKind) -> &ShipSettings {
        &self.ship_settings[ship.network_value() as usize]
    }
}
