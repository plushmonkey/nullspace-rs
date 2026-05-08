use crate::arena_settings::ArenaSettings;
use crate::attach::AttachKind;
use crate::attach::can_attach_to;
use crate::chat::ChatCommand;
use crate::chat::ChatController;
use crate::checksum;
use crate::clock::*;
use crate::flag::FlagController;
use crate::game_view::render_explosions;
use crate::game_view::render_trails;
use crate::input::InputAction;
use crate::input::InputState;
use crate::map::DoorRng;
use crate::map::Map;
use crate::math::PixelUnit;
use crate::math::PositionUnit;
use crate::math::Rectangle;
use crate::math::{Position, Velocity};
use crate::net::connection::ConnectionError;
use crate::net::connection::SocketKind;
use crate::net::connection::{Connection, ConnectionState};
use crate::net::packet::bi::*;
use crate::net::packet::c2s::*;
use crate::net::packet::s2c::*;
use crate::notification::NotificationManager;
use crate::player::*;
use crate::prize::Prize;
use crate::prize::PrizeManager;
use crate::prize::apply_prize_id;
use crate::radar::Radar;
use crate::render::game_sprites::GAME_SPRITE_SHEET_DEFINITIONS;
use crate::render::game_sprites::GameSpriteKind;
use crate::render::layer::Layer;
use crate::render::render_state::RenderState;
use crate::render::text_renderer::TextColor;
use crate::rng::VieRng;
use crate::select_box::SelectBox;
use crate::ship::ShipKind;
use crate::ship_controller::ShipController;
use crate::simulation::game_simulation::Simulation;
use crate::simulation::game_simulation::SimulationEventKind;
use crate::simulation::player_simulation::PLAYER_EXPLOSION_DURATION;
use crate::simulation::player_simulation::PLAYER_FLASH_DURATION;
use crate::simulation::player_simulation::update_player_lerp_target;
use crate::spawn::generate_spawn_position;
use crate::spectate_controller::SpectateController;
use crate::statbox::Statbox;
use crate::weapon::WeaponKind;

use miniz_oxide::inflate::decompress_to_vec_zlib;

#[cfg(not(target_arch = "wasm32"))]
fn build_zone_directory(zone: &str) -> Result<(), std::io::Error> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .create(format!("zones/{}", zone))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn get_zone_path(zone: &str, filename: &str) -> String {
    format!("zones/{}/{}", zone, filename)
}

pub enum MovementController {
    Ship(ShipController),
    Spectate(SpectateController),
}

// This is a queue for outgoing position messages.
// We only send position packets on new local ticks to try to keep packets ordered.
// Continuum has a bug where it doesn't handle weapon packets if the timestamp was already handled
// with a non-weapon packet. By sending these on separate local ticks, we give time for the packets to arrive in order
// instead of being sent on same overlapping local ticks.
struct OutboundPositionQueue {
    messages: Vec<PositionMessage>,
    last_sent_local_tick: GameTick,
}

impl OutboundPositionQueue {
    fn new() -> Self {
        Self {
            messages: vec![],
            last_sent_local_tick: GameTick::now(0),
        }
    }

    fn push(&mut self, message: PositionMessage) {
        self.messages.push(message);
    }

    fn next(&mut self, local_tick: GameTick) -> Option<PositionMessage> {
        if self.messages.is_empty() {
            return None;
        }

        if local_tick > self.last_sent_local_tick {
            self.last_sent_local_tick = local_tick;
            Some(self.messages.remove(0))
        } else {
            None
        }
    }
}

pub struct Client {
    pub connection: Connection,
    pub map: Map,
    pub settings: Box<ArenaSettings>,
    pub last_position_tick: GameTick,

    pub username: String,
    pub password: String,
    pub zone: String,

    pub registration: RegistrationFormMessage,

    pub simulation: Simulation,
    pub flag_controller: FlagController,
    pub prize_manager: PrizeManager,

    // This is the local tick for the last processed tick.
    local_tick: GameTick,

    pub radar: Radar,

    pub chat_controller: ChatController,
    pub statbox: Statbox,
    pub notifications: NotificationManager,

    pub controller: MovementController,
    outbound_position_queue: OutboundPositionQueue,

    pub camera_jitter_time: u32,
}

impl Client {
    pub fn new(
        username: &str,
        password: &str,
        zone: &str,
        socket: SocketKind,
        registration: RegistrationFormMessage,
    ) -> Result<Client, ConnectionError> {
        let connection = Connection::new(socket)?;

        Ok(Client {
            connection,
            map: Map::empty(""),
            settings: Box::new(ArenaSettings::default()),
            last_position_tick: GameTick::now(0),
            username: username.to_owned(),
            password: password.to_owned(),
            zone: zone.to_owned(),
            registration,
            simulation: Simulation::new(GameTick::now(0)),
            flag_controller: FlagController::new(),
            prize_manager: PrizeManager::new(),
            local_tick: GameTick::now(0),
            radar: Radar::new(),
            chat_controller: ChatController::new(),
            statbox: Statbox::new(),
            notifications: NotificationManager::new(),
            controller: MovementController::Spectate(SpectateController::new()),
            outbound_position_queue: OutboundPositionQueue::new(),
            camera_jitter_time: 0,
        })
    }

    pub fn update(
        &mut self,
        render_state: Option<&mut RenderState>,
        input_state: &mut InputState,
        dt: f32,
    ) -> Result<i32, ConnectionError> {
        let mut render_state = render_state;

        self.receive_messages(&mut render_state)?;

        let local_now = GameTick::now(0);

        // If we have a stable connection with the map downloaded, start using the actual game tick to perform tick updates.
        // This is preferred over local ticks in case of timer drift.
        let tick_count = if let ConnectionState::Playing = self.connection.state {
            self.connection
                .get_current_server_tick()
                .diff(&self.connection.get_game_tick())
        } else {
            local_now.diff(&self.local_tick)
        };

        self.radar.render_full = input_state.is_down(InputAction::FullRadar);

        for _ in 0..tick_count {
            self.statbox
                .handle_input(input_state, &self.simulation.player_manager);

            // Tentative outbound position packet. This will be preempted by a weapon packet for the new tick if a weapon is fired.
            // We generate our previous tick packet so our non-weapon packets and weapon packets will have a reduced chance of
            // aligning due to network jitter.
            let previous_tick_position_message = self.generate_position_message();

            self.connection.tick();

            self.map
                .tick(&self.settings, self.connection.get_game_tick());

            // Simulation must be updated before spectate controller so the positions are updated for the player we're spectating.
            self.simulation.tick(&mut self.map, &self.settings);

            self.flag_controller.tick(
                &mut self.simulation.player_manager,
                &mut self.connection,
                &self.settings,
            );

            self.notifications.tick();

            if self.camera_jitter_time > 0 {
                self.camera_jitter_time -= 1;
            }

            self.process_controller(render_state.as_deref_mut(), input_state);

            self.process_simulation_events();

            if let Some(render_state) = &mut render_state {
                render_trails(self, render_state);

                let self_position = Position::new(
                    PositionUnit(render_state.camera.position.x as i32 * 16000),
                    PositionUnit(render_state.camera.position.y as i32 * 16000),
                );

                match &self.connection.state {
                    ConnectionState::Playing | ConnectionState::Disconnected => {
                        self.radar.update(
                            render_state.config.width,
                            self.settings.map_zoom_factor as u16,
                            self_position,
                            self.connection.get_game_tick(),
                        );
                    }
                    _ => {}
                }

                render_explosions(self, render_state);
            }

            match self.connection.state {
                ConnectionState::Playing => {
                    self.send_position_message(previous_tick_position_message);
                }
                ConnectionState::Disconnected => {
                    break;
                }
                _ => {
                    // Move our world position so the stars move while we join the game.
                    if let Some(render_state) = &mut render_state {
                        let scroll_speed = 10.0f32;
                        render_state.camera.position = render_state.camera.position
                            - glam::Vec2::new(0.0f32, scroll_speed * dt);
                    }
                }
            }

            input_state.clear_triggered();
        }

        if let Some(position_message) = self.outbound_position_queue.next(local_now) {
            self.connection.send(&position_message)?;
        }

        self.local_tick = self.local_tick + tick_count;

        match &self.connection.state {
            ConnectionState::Playing | ConnectionState::Disconnected => {
                if let Some(render_state) = &mut render_state {
                    if let Some(me) = self.simulation.player_manager.get_self() {
                        if let Some(me_position) = me.position {
                            render_state.camera.position.x =
                                (me_position.x.0 / 1000) as f32 / 16.0f32;
                            render_state.camera.position.y =
                                (me_position.y.0 / 1000) as f32 / 16.0f32;
                        }
                    }
                }
            }

            _ => {}
        }

        Ok(tick_count)
    }

    fn process_controller(
        &mut self,
        render_state: Option<&mut RenderState>,
        input_state: &mut InputState,
    ) {
        match &self.connection.state {
            ConnectionState::Playing | ConnectionState::Disconnected => {
                match &mut self.controller {
                    MovementController::Spectate(spectate_controller) => {
                        spectate_controller.tick(
                            input_state,
                            &mut self.simulation.player_manager,
                            &mut self.connection,
                            &self.statbox,
                            &self.settings,
                        );

                        self.prize_manager.tick(
                            &mut self.simulation.player_manager,
                            &self.settings,
                            &self.map,
                            &mut self.connection,
                            &mut self.notifications,
                            &mut None,
                        );
                    }
                    MovementController::Ship(ship_controller) => {
                        let current_tick = self.connection.get_game_tick();

                        ship_controller.tick(
                            input_state,
                            &mut self.connection,
                            &mut self.simulation.player_manager,
                            &self.simulation.weapon_manager,
                            &mut self.simulation.powerball_manager,
                            &self.map,
                            &self.radar,
                            &self.settings,
                            &mut self.notifications,
                            current_tick,
                            render_state,
                        );

                        self.prize_manager.tick(
                            &mut self.simulation.player_manager,
                            &self.settings,
                            &self.map,
                            &mut self.connection,
                            &mut self.notifications,
                            &mut Some(ship_controller),
                        );

                        if input_state.is_triggered(InputAction::Attach) {
                            self.attach();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn process_simulation_events(&mut self) {
        if let MovementController::Ship(ship_controller) = &mut self.controller {
            for event in &self.simulation.events {
                match &event.kind {
                    SimulationEventKind::WeaponExplosion(explosion_event) => {
                        let took_damage = ship_controller.apply_damage(
                            &mut self.simulation.player_manager,
                            &mut self.connection,
                            &self.settings,
                            explosion_event,
                            Some(&mut self.notifications),
                        );

                        if took_damage {
                            match &explosion_event.kind {
                                WeaponKind::Bomb(_)
                                | WeaponKind::ProximityBomb(_)
                                | WeaponKind::Thor(_) => {
                                    self.camera_jitter_time = self.settings.jitter_time as u32;
                                }
                                _ => {}
                            }
                        }
                    }
                    SimulationEventKind::PowerballPickupRequest(ball_id, ball_timestamp) => {
                        let request = crate::net::packet::c2s::PowerballRequestMessage {
                            ball_id: *ball_id,
                            timestamp: *ball_timestamp,
                        };

                        if let Err(e) = self.connection.send_reliable(&request) {
                            log::error!("{e}");
                        }
                    }
                    SimulationEventKind::PowerballTimeout(ball_id) => {
                        let (position, velocity) =
                            if let Some(me) = self.simulation.player_manager.get_self() {
                                if let Some(me_position) = me.position {
                                    let speed = if me.ship_kind != ShipKind::Spectator {
                                        self.settings
                                            .get_ship_settings(me.ship_kind)
                                            .powerball_speed
                                    } else {
                                        0
                                    };

                                    let mut velocity = me.velocity;
                                    let forward = me.get_heading() * -1.0f32 * speed as f32;

                                    velocity.x.0 += forward.x as i32;
                                    velocity.y.0 += forward.y as i32;

                                    (me_position, velocity)
                                } else {
                                    (Position::empty(), Velocity::empty())
                                }
                            } else {
                                (Position::empty(), Velocity::empty())
                            };

                        let message = crate::net::packet::c2s::PowerballFireMessage {
                            ball_id: *ball_id,
                            x: (position.x.0 / 1000) as u16,
                            y: (position.y.0 / 1000) as u16,
                            x_velocity: velocity.x.0 as i16,
                            y_velocity: velocity.y.0 as i16,
                            player_id: self.simulation.player_manager.self_id,
                            timestamp: self.connection.get_game_tick(),
                        };

                        if let Err(e) = self.connection.send_reliable(&message) {
                            log::error!("{e}");
                        }
                    }
                    SimulationEventKind::PowerballGoal(ball_id, x, y) => {
                        let message = crate::net::packet::c2s::PowerballScoreMessage {
                            ball_id: *ball_id,
                            x: *x,
                            y: *y,
                        };

                        if let Err(e) = self.connection.send_reliable(&message) {
                            log::error!("{e}");
                        }
                    }
                    SimulationEventKind::DoorWarp => {
                        let player_count = self.simulation.player_manager.players.len();
                        let rng = VieRng::new(self.connection.get_game_tick().value() as i32);

                        if let Some(me) = self.simulation.player_manager.get_self_mut() {
                            let position = generate_spawn_position(
                                &self.settings,
                                &self.map,
                                me.ship_kind,
                                me.frequency,
                                rng,
                                player_count,
                            );
                            me.position = Some(position);
                            me.status |= StatusFlags::Flash;
                            ship_controller.ship.status |= StatusFlags::Flash;
                            me.velocity.clear();
                        }
                    }
                    SimulationEventKind::Repel => {
                        ship_controller.ship.repel_effect_remaining_ticks =
                            self.settings.repel_time as u32;
                    }
                    SimulationEventKind::AttachSync => {
                        if let Some(_) = ship_controller.pending_attach_target {
                            ship_controller.ship.current_energy /= 3;

                            ship_controller.ship.fake_antiwarp_remaining_ticks =
                                self.settings.antiwarp_settle_delay as u32;

                            ship_controller.pending_attach_target = None;
                        }
                    }
                }
            }
        }
    }

    fn attach(&mut self) {
        let MovementController::Ship(ship_controller) = &mut self.controller else {
            return;
        };

        let self_id = self.simulation.player_manager.self_id;
        let target_id = self.statbox.get_selected_player_id();

        match can_attach_to(
            &self.simulation.player_manager,
            ship_controller,
            &self.settings,
            target_id,
        ) {
            Ok(kind) => match kind {
                AttachKind::DetachChildren => {
                    let request = crate::net::packet::c2s::DetachAllRequestMessage {};

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }

                    self.simulation.player_manager.detach_all_children(self_id);
                }
                AttachKind::DetachSelf => {
                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: PlayerId::invalid(),
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }

                    self.simulation.player_manager.detach_player(self_id);
                }
                AttachKind::Attach(target_id) => {
                    if ship_controller.is_antiwarped(
                        &self.simulation.player_manager,
                        &self.radar,
                        &mut self.notifications,
                        self.settings.antiwarp_pixels as u32,
                    ) {
                        return;
                    }

                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: target_id,
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }

                    self.simulation
                        .player_manager
                        .attach_player(self_id, target_id);

                    ship_controller.pending_attach_target = Some(target_id);
                }
            },
            Err(e) => {
                self.notifications
                    .push_str(e.get_notification_string(), TextColor::Yellow);
            }
        }
    }

    fn get_extra_position_data(&self) -> Option<ExtraPositionData> {
        let mut flag_timer = 0;

        if let Some(me) = self.simulation.player_manager.get_self() {
            flag_timer = (me.flag_remaining_ticks / 100) as u16;
        }

        if let MovementController::Ship(ship_controller) = &self.controller {
            if self.settings.extra_position_data || self.connection.send_extra_position_info {
                let items = ItemSet {
                    shield_active: ship_controller.ship.shield_remaining_ticks > 0,
                    super_active: ship_controller.ship.super_remaining_ticks > 0,
                    bursts: ship_controller.ship.bursts,
                    repels: ship_controller.ship.repels,
                    thors: ship_controller.ship.thors,
                    bricks: ship_controller.ship.bricks,
                    decoys: ship_controller.ship.decoys,
                    rockets: ship_controller.ship.rockets,
                    portals: ship_controller.ship.portals,
                };

                if flag_timer == 0 {
                    if ship_controller.crown_remaining_ticks > 0 {
                        flag_timer = (ship_controller.crown_remaining_ticks / 100) as u16;
                    }
                }

                let data = ExtraPositionData {
                    energy: (ship_controller.ship.current_energy / 1000) as u16,
                    s2c_latency: 0,
                    flag_timer,
                    items: items,
                };

                return Some(data);
            }
        }

        None
    }

    // Generates a non-weapon position packet.
    fn generate_position_message(&self) -> Option<PositionMessage> {
        let Some(me) = self.simulation.player_manager.get_self() else {
            return None;
        };

        let (x_position, y_position) = if let Some(me_position) = me.position {
            (me_position.x.0 / 1000, me_position.y.0 / 1000)
        } else {
            (0xFFFF, 0xFFFF)
        };

        let energy = if let MovementController::Ship(ship_controller) = &self.controller {
            ship_controller.ship.current_energy / 1000
        } else {
            0
        };

        let timestamp = self.connection.get_game_tick();

        let position = PositionMessage {
            direction: me.direction,
            timestamp,
            x_position: x_position as u16,
            y_position: y_position as u16,
            x_velocity: (me.velocity.x.0) as i16,
            y_velocity: (me.velocity.y.0) as i16,
            togglables: me.status,
            bounty: me.bounty,
            energy: energy as u16,
            weapon_info: 0,
            extra_info: self.get_extra_position_data(),
        };

        Some(position)
    }

    fn send_position_message(&mut self, previous_tick_position_message: Option<PositionMessage>) {
        let (position_sync_delay, energy, status, weapon_kind) = match &mut self.controller {
            MovementController::Spectate(_) => (300, 0, 0, WeaponKind::None),
            MovementController::Ship(ship_controller) => {
                let weapon_kind = if let Some(weapon_kind) = ship_controller.ship.weapon {
                    weapon_kind
                } else {
                    WeaponKind::None
                };

                (
                    10, // This should use the settings position delay, but Continuum has issues if the delay is too short.
                    ship_controller.ship.current_energy / 1000,
                    ship_controller.ship.status,
                    weapon_kind,
                )
            }
        };

        let weapon_info = match weapon_kind {
            WeaponKind::None => 0,
            _ => weapon_kind.pack(),
        };

        let current_tick = self.connection.get_game_tick();

        let position_delay_elapsed =
            current_tick.diff(&self.last_position_tick) >= position_sync_delay;

        if position_delay_elapsed || weapon_info != 0 {
            if let Some(me) = self.simulation.player_manager.get_self() {
                if weapon_info != 0 {
                    if let Some(position) = me.position {
                        let spawn_x = position.x.0 / 1000;
                        let spawn_y = position.y.0 / 1000;

                        // Round to pixel because that's all the network supports, so other clients will spawn there as well.
                        let spawn_position =
                            Position::from_pixels(PixelUnit(spawn_x), PixelUnit(spawn_y));

                        self.simulation.weapon_manager.spawn_weapons(
                            me,
                            spawn_position,
                            me.velocity,
                            me.direction,
                            weapon_kind,
                            &self.settings,
                            self.connection.get_game_tick(),
                        );
                    }
                }

                // Generate a new position packet if we are using a weapon, otherwise use our previous tick's non-weapon packet.
                let position_message =
                    if weapon_info != 0 || previous_tick_position_message.is_none() {
                        let (x_position, y_position) = if let Some(me_position) = me.position {
                            (me_position.x.0 / 1000, me_position.y.0 / 1000)
                        } else {
                            (0xFFFF, 0xFFFF)
                        };

                        let position = PositionMessage {
                            direction: me.direction,
                            timestamp: self.connection.get_game_tick(),
                            x_position: x_position as u16,
                            y_position: y_position as u16,
                            x_velocity: (me.velocity.x.0) as i16,
                            y_velocity: (me.velocity.y.0) as i16,
                            togglables: status,
                            bounty: me.bounty,
                            energy: energy as u16,
                            weapon_info,
                            extra_info: self.get_extra_position_data(),
                        };

                        position
                    } else {
                        previous_tick_position_message.unwrap()
                    };

                if let MovementController::Ship(ship_controller) = &mut self.controller {
                    ship_controller.ship.status &= !StatusFlags::Flash;
                }

                self.outbound_position_queue.push(position_message);

                self.last_position_tick = self.connection.get_game_tick();
            }

            // Make sure our player data will be considered synchronized by the simulation.
            if let Some(player) = self
                .simulation
                .player_manager
                .get_by_id_mut(self.connection.player_id)
            {
                player.last_position_timestamp = self.last_position_tick;
            }
        }
    }

    pub fn handle_ship_request(&mut self, ship_kind: ShipKind) {
        if let Some(me) = self.simulation.player_manager.get_self() {
            if me.ship_kind == ship_kind {
                if ship_kind != ShipKind::Spectator {
                    self.notifications
                        .push_str("You are already in that type of ship.", TextColor::Yellow);
                }

                return;
            }
        }

        if let MovementController::Ship(ship_controller) = &self.controller {
            if !ship_controller.ship.is_max_energy() {
                self.notifications.push_str(
                    "Must have full energy to change ship types.",
                    TextColor::Yellow,
                );

                return;
            }
        }

        let ship_request = crate::net::packet::c2s::RequestShipMessage { ship_kind };

        if let Err(e) = self.connection.send_reliable(&ship_request) {
            log::error!("{e}");
        }
    }

    pub fn handle_chat_command(&mut self, command: ChatCommand) {
        match &command {
            ChatCommand::ChangeFrequency(frequency) => {
                if let Some(me) = self.simulation.player_manager.get_self() {
                    if me.frequency == *frequency {
                        self.notifications
                            .push_str("You are already on that frequency.", TextColor::Yellow);
                        return;
                    }
                }

                if let MovementController::Ship(ship_controller) = &self.controller {
                    if ship_controller.ship.status & StatusFlags::Safety == 0
                        && !ship_controller.ship.is_max_energy()
                    {
                        self.notifications.push_str(
                            "Not enough energy to change frequencies.",
                            TextColor::Yellow,
                        );
                        return;
                    }
                }

                let message = crate::net::packet::c2s::FrequencyChangeMessage {
                    frequency: *frequency,
                };

                if let Err(e) = self.connection.send_reliable(&message) {
                    log::error!("{e}");
                }
            }
            ChatCommand::Go(target) => {
                if let MovementController::Ship(ship_controller) = &self.controller {
                    if !ship_controller.ship.is_max_energy() {
                        self.notifications
                            .push_str("Must have full energy to change arenas.", TextColor::Yellow);
                        return;
                    }
                }

                if target.is_empty() {
                    let request = crate::net::packet::c2s::ArenaJoinMessage::new(
                        crate::ship::ShipKind::Spectator,
                        1920,
                        1080,
                        crate::net::packet::c2s::ArenaRequest::AnyPublic,
                    );

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                } else {
                    let request = if let Ok(number) = target.parse::<u16>() {
                        crate::net::packet::c2s::ArenaJoinMessage::new(
                            crate::ship::ShipKind::Spectator,
                            1920,
                            1080,
                            crate::net::packet::c2s::ArenaRequest::SpecificPublic(number),
                        )
                    } else {
                        crate::net::packet::c2s::ArenaJoinMessage::new(
                            crate::ship::ShipKind::Spectator,
                            1920,
                            1080,
                            crate::net::packet::c2s::ArenaRequest::Name(target.to_string()),
                        )
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                }
            }
        }
    }

    fn receive_messages(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
    ) -> Result<(), ConnectionError> {
        loop {
            let message = self.connection.receive_message();
            if let Err(e) = message {
                match e {
                    ConnectionError::IoError(_) => {
                        return Err(e);
                    }
                    ConnectionError::Timeout => {
                        self.chat_controller.handle_chat_message(
                            ChatKind::Arena,
                            "".to_string(),
                            "WARNING: Disconnected from server (no data)".to_string(),
                        );
                        return Err(e);
                    }
                    ConnectionError::ProxyConnect | ConnectionError::ProxyRecv => {
                        return Err(e);
                    }
                    _ => {}
                }

                continue;
            }

            let message = message.unwrap();

            if let Some(message) = message {
                self.process_message(render_state, message)?;
            } else {
                // We are done processing everything now.
                break;
            }
        }

        Ok(())
    }

    fn process_core_message(
        &mut self,
        _render_state: &mut Option<&mut RenderState>,
        message: &CoreServerMessage,
    ) -> Result<(), ConnectionError> {
        match message {
            CoreServerMessage::EncryptionResponse(_) => {
                let password = PasswordMessage::new(
                    &self.username,
                    &self.password,
                    false,
                    0x1231241,
                    240,
                    0x86,
                    123412,
                );

                self.connection.send_reliable(&password)?;

                let sync_request = ClockSyncRequestMessage::new(GameTick::now(0), 2, 2);
                self.connection.send(&sync_request)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn process_game_message(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
        message: &GameServerMessage,
    ) -> Result<(), ConnectionError> {
        match message {
            GameServerMessage::Chat(chat) => {
                let mut sender_name = String::new();

                match chat.kind {
                    ChatKind::Public | ChatKind::PublicMacro => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("{}> {}", sender.name, chat.message);
                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Team => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("T {}> {}", sender.name, chat.message);
                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Frequency => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("F {}> {}", sender.name, chat.message);

                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Arena | ChatKind::Error | ChatKind::Warning => {
                        if !chat.message.is_empty() {
                            log::debug!("A {}", chat.message);
                        }
                    }
                    ChatKind::Private => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("P {}> {}", sender.name, chat.message);

                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::RemotePrivate => {
                        log::debug!("RP {}", chat.message);
                        if !chat.message.is_empty() {
                            let check = &chat.message[1..];
                            if let Some(end_position) =
                                check.as_bytes().iter().position(|c| *c == b':')
                            {
                                sender_name = check[..end_position].to_string();
                            }
                        }
                    }
                    ChatKind::Channel => {
                        log::debug!("C {}", chat.message);
                    }
                    ChatKind::Fuchsia => {
                        log::debug!("F {}", chat.message);
                    }
                }

                self.chat_controller.handle_chat_message(
                    chat.kind,
                    sender_name,
                    chat.message.clone(),
                );
            }
            GameServerMessage::PasswordResponse(password_response) => {
                log::debug!("Got password response: {}", password_response.response);

                match &password_response.response {
                    LoginResponse::Ok => {
                        let arena_request = ArenaJoinMessage::new(
                            ShipKind::Spectator,
                            1920,
                            1080,
                            ArenaRequest::AnyPublic,
                        );

                        self.connection.send_reliable(&arena_request)?;
                    }
                    LoginResponse::Unregistered => {
                        if password_response.registration_request {
                            let mut registration_packet = vec![0; 766].into_boxed_slice();

                            log::debug!("Sending registration");

                            self.registration.serialize(&mut registration_packet);
                            self.connection.send_reliable_data(&registration_packet)?;
                        } else {
                            let password = PasswordMessage::new(
                                &self.username,
                                &self.password,
                                true,
                                0x1231241,
                                240,
                                0x86,
                                123412,
                            );

                            self.connection.send_reliable(&password)?;
                        }
                    }
                    _ => {
                        log::debug!("Failed to login: {:?}", password_response.response);
                        self.connection.state = ConnectionState::Disconnected;
                    }
                }
            }
            GameServerMessage::PlayerId(message) => {
                // We need to initialize the simulation here before we receive player enter events.
                self.simulation = Simulation::new(self.connection.get_game_tick());
                self.flag_controller.clear();
                self.last_position_tick = self.connection.get_game_tick();
                self.simulation.player_manager.self_id = message.id;

                self.notifications.clear();

                self.map.clear_bricks();
                self.camera_jitter_time = 0;

                // Stop downloading the map if we're downloading.
                // We need to clear the process queue for the new settings and map.
                self.connection.cancel_downloads();

                if let Some(render_state) = render_state {
                    render_state.animation_renderer.clear();
                    render_state
                        .banner_manager
                        .clear(&mut render_state.sprite_renderer);
                }

                self.statbox.reset();

                self.statbox.set_view(
                    &self.simulation.player_manager,
                    crate::statbox::StatboxView::Names,
                );
            }
            GameServerMessage::ArenaSettings(settings_message) => {
                log::debug!("Received arena settings");
                self.settings = settings_message.clone();

                if self.settings.door_mode >= 0 {
                    self.map.door_rng = Some(DoorRng::new(
                        self.settings.door_mode as u32,
                        self.connection.get_game_tick(),
                        self.settings.door_mode as u8,
                        self.settings.door_mode as u8,
                    ));

                    self.map.set_door_mode(self.settings.door_mode as u8);
                }
            }
            GameServerMessage::SynchronizationRequest(sync) => {
                self.map.set_door_seed(sync.door_seed, sync.timestamp);
                self.map.tick(&self.settings, sync.timestamp);
                self.prize_manager.set_seed(sync.prize_seed as i32);

                if sync.checksum_key != 0 && self.map.checksum != 0 {
                    // Send security packet
                    log::debug!("Game sync requested");

                    let settings_checksum =
                        checksum::settings_checksum(sync.checksum_key, &self.settings);
                    let exe_checksum = checksum::vie_checksum(sync.checksum_key);
                    let level_checksum = checksum::checksum_map(&self.map, sync.checksum_key);

                    let ping_average = self.connection.sync_history.get_average_ping();
                    let ping_low = self.connection.sync_history.get_low_ping();
                    let ping = self.connection.ping;
                    let ping_high = self.connection.sync_history.get_high_ping();

                    let response = SecurityMessage::new(
                        self.connection.weapons_recv,
                        settings_checksum,
                        exe_checksum,
                        level_checksum,
                        ping as u16 / 10,
                        ping_average as u16 / 10,
                        ping_low as u16 / 10,
                        ping_high as u16 / 10,
                    );
                    log::debug!("Sending game sync packet");
                    self.connection.send_reliable(&response)?;
                }
            }
            GameServerMessage::BrickDrop(message) => {
                let start = glam::Vec2::new(message.x1 as f32, message.y1 as f32);
                let end = glam::Vec2::new(message.x2 as f32, message.y2 as f32);
                let direction = (end - start).normalize();
                let distance = start.distance(end).ceil() as i32 + 1;

                let end_tick = message.timestamp + self.settings.brick_time as i32;

                let mut position = start;
                let mut me_collider = Rectangle::empty();
                let mut check_brickwarp = false;

                if let Some(me) = self.simulation.player_manager.get_self() {
                    if me.ship_kind != ShipKind::Spectator && !me.is_dead() {
                        if let Some(_) = me.position {
                            me_collider = me.get_collider(
                                self.settings.get_ship_settings(me.ship_kind).get_radius(),
                            );
                            check_brickwarp = me.frequency != message.frequency;
                        }
                    }
                }

                for _ in 0..distance {
                    self.map.insert_brick(
                        position.x as u16,
                        position.y as u16,
                        message.frequency,
                        end_tick,
                    );

                    if check_brickwarp {
                        let collider = Rectangle::new(
                            Position::from_tile(position.x as i32, position.y as i32),
                            Position::from_tile(position.x as i32 + 1, position.y as i32 + 1),
                        );

                        if collider.intersects(&me_collider) {
                            let player_count = self.simulation.player_manager.players.len();

                            if let Some(me) = self.simulation.player_manager.get_self_mut() {
                                let rng =
                                    VieRng::new(self.connection.get_game_tick().value() as i32);

                                let new_position = generate_spawn_position(
                                    &self.settings,
                                    &self.map,
                                    me.ship_kind,
                                    me.frequency,
                                    rng,
                                    player_count,
                                );
                                me.position = Some(new_position);
                                me.velocity.clear();
                                me.status |= StatusFlags::Flash;
                            }
                        }
                    }

                    position += direction;
                }
            }
            GameServerMessage::PlayerEntering(entering) => {
                for entry in &entering.players {
                    let mut player = Player::new(
                        entry.player_id,
                        &entry.name,
                        &entry.squad,
                        entry.ship_kind,
                        entry.frequency,
                        entry.flag_points,
                        entry.kill_points,
                        entry.has_koth,
                    );

                    player.wins = entry.kills;
                    player.losses = entry.deaths;
                    player.flag_count = entry.flag_count;
                    player.attach_parent = entry.attach_parent;
                    player.last_position_timestamp = self.connection.get_game_tick();

                    if entry.player_id == self.simulation.player_manager.self_id {
                        player.position = Some(Position::empty());

                        if player.ship_kind == ShipKind::Spectator {
                            self.controller =
                                MovementController::Spectate(SpectateController::new());
                        }
                    }

                    log::debug!("{} entered arena {:?}", entry.name, entry.ship_kind);

                    // If there was someone already in this place, say that they left.
                    // This can happen when joining at the same exact time as other players.
                    if let Some(old_player) = self.simulation.player_manager.add_player(player) {
                        log::debug!("{} left arena", old_player.name);
                    }
                }

                // Add children after adding all players above so their parent will exist.
                for entry in &entering.players {
                    if entry.attach_parent.valid() {
                        if let Some(parent) = self
                            .simulation
                            .player_manager
                            .get_by_id_mut(entry.attach_parent)
                        {
                            parent.children.push(entry.player_id);
                        }
                    }
                }

                self.statbox.rebuild(&self.simulation.player_manager);

                if let MovementController::Spectate(spectate_controller) = &mut self.controller {
                    spectate_controller.handle_player_entering(
                        &mut self.simulation.player_manager,
                        &mut self.connection,
                        &self.statbox,
                    );
                }
            }
            GameServerMessage::PlayerLeaving(leaving) => {
                self.simulation
                    .player_manager
                    .detach_all_children(leaving.player_id);

                self.simulation
                    .weapon_manager
                    .clear_player_weapons(leaving.player_id);

                if let Some(player) = self
                    .simulation
                    .player_manager
                    .remove_player(leaving.player_id)
                {
                    log::debug!("{} left arena", player.name);
                }

                if let Some(render_state) = render_state {
                    render_state
                        .banner_manager
                        .destroy_banner(&mut render_state.sprite_renderer, leaving.player_id);
                }

                self.statbox.rebuild(&self.simulation.player_manager);

                if let MovementController::Spectate(spectate_controller) = &mut self.controller {
                    spectate_controller.handle_player_leave(
                        leaving,
                        &mut self.simulation.player_manager,
                        &mut self.connection,
                        &self.statbox,
                    );
                }
            }
            GameServerMessage::PlayerBannerChanged(message) => {
                if let Some(render_state) = render_state {
                    render_state.banner_manager.set_banner(
                        &render_state.device,
                        &render_state.queue,
                        &mut render_state.sprite_renderer,
                        message.player_id,
                        &message.banner_data,
                    );
                }
            }
            GameServerMessage::TurretLinkCreate(message) => {
                message.requester_id;

                if message.destination_id.is_none() {
                    // If there was no destination id provided in the packet, we must detach ourself and send the packet.
                    self.simulation
                        .player_manager
                        .detach_player(self.simulation.player_manager.self_id);

                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: PlayerId::invalid(),
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                } else {
                    self.simulation
                        .player_manager
                        .attach_player(message.requester_id, message.destination_id.unwrap())
                }
            }
            GameServerMessage::TurretLinkDestroy(message) => {
                if self
                    .simulation
                    .player_manager
                    .detach_all_children(message.player_id)
                {
                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: PlayerId::invalid(),
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                }
            }
            GameServerMessage::SmallPosition(message) => {
                let self_id = self.simulation.player_manager.self_id;

                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    let message_timestamp =
                        GameTick::from_mini(self.connection.get_game_tick(), message.timestamp)
                            - message.ping as i32;

                    if message.status & StatusFlags::Flash != 0 {
                        // Always override new flashes if we get one, even if the message timestamp is older.
                        if player.flash_remaining_ticks == 0 {
                            player.status |= StatusFlags::Flash;
                        }

                        if let Some(current_position) = player.position {
                            let (x_pixels, y_pixels) = current_position.to_pixels();

                            if let Some(render_state) = render_state {
                                let (cols, rows) =
                                    GAME_SPRITE_SHEET_DEFINITIONS[GameSpriteKind::Flash as usize];
                                let frame_count = cols * rows;

                                render_state.animation_renderer.add(
                                    GameSpriteKind::Flash,
                                    message_timestamp,
                                    0,
                                    frame_count as usize,
                                    PLAYER_FLASH_DURATION,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Explosions,
                                );
                            }
                        }
                    }

                    if player.last_position_timestamp < message_timestamp {
                        let position = Position::new(
                            PixelUnit(message.x as i32).into(),
                            PixelUnit(message.y as i32).into(),
                        );

                        player.velocity = Velocity::new(
                            PositionUnit(message.x_velocity as i32),
                            PositionUnit(message.y_velocity as i32),
                        );

                        if player.id != self_id {
                            let sim_ticks =
                                self.connection.get_game_tick().diff(&message_timestamp);

                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );
                        } else {
                            player.position = Some(position);
                        }

                        player.direction = message.direction;
                        player.bounty = message.bounty as u16;
                        player.status = message.status;
                        player.ping = message.ping;
                        player.last_position_timestamp = message_timestamp;

                        if let Some(extra) = &message.extra {
                            player.extra_position_data = Some(*extra);
                            player.last_extra_data_timestamp = Some(message_timestamp);
                        }
                    } else {
                        Self::validate_packet_timestamp(
                            self.connection.get_game_tick(),
                            message_timestamp,
                            "small",
                        );
                    }
                }
            }
            GameServerMessage::LargePosition(message) => {
                let self_id = self.simulation.player_manager.self_id;

                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    let message_timestamp =
                        GameTick::from_mini(self.connection.get_game_tick(), message.timestamp)
                            - message.ping as i32;

                    let position = Position::new(
                        PixelUnit(message.x as i32).into(),
                        PixelUnit(message.y as i32).into(),
                    );

                    let velocity = Velocity::new(
                        PositionUnit(message.x_velocity as i32),
                        PositionUnit(message.y_velocity as i32),
                    );

                    let direction = message.direction;

                    if message.status & StatusFlags::Flash != 0 {
                        // Always override new flashes if we get one, even if the message timestamp is older.
                        if player.flash_remaining_ticks == 0 {
                            player.status |= StatusFlags::Flash;
                        }

                        if let Some(current_position) = player.position {
                            let (x_pixels, y_pixels) = current_position.to_pixels();

                            if let Some(render_state) = render_state {
                                let (cols, rows) =
                                    GAME_SPRITE_SHEET_DEFINITIONS[GameSpriteKind::Flash as usize];
                                let frame_count = cols * rows;

                                render_state.animation_renderer.add(
                                    GameSpriteKind::Flash,
                                    message_timestamp,
                                    0,
                                    frame_count as usize,
                                    PLAYER_FLASH_DURATION,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Explosions,
                                );
                            }
                        }
                    }

                    if player.last_position_timestamp < message_timestamp {
                        player.velocity = velocity;

                        if player.id != self_id {
                            let sim_ticks =
                                self.connection.get_game_tick().diff(&message_timestamp);

                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );
                        } else {
                            player.position = Some(position);
                        }

                        player.direction = message.direction;
                        player.bounty = message.bounty;
                        player.status = message.status;
                        player.ping = message.ping;
                        player.last_position_timestamp = message_timestamp;

                        if let Some(extra) = &message.extra {
                            player.extra_position_data = Some(*extra);
                            player.last_extra_data_timestamp = Some(message_timestamp);
                        }
                    } else {
                        if Self::validate_packet_timestamp(
                            self.connection.get_game_tick(),
                            message_timestamp,
                            "large",
                        ) {
                            return Ok(());
                        }
                    }

                    let weapon_kind =
                        WeaponKind::new(message.weapon, position, velocity, player, &self.settings);

                    if let Some(weapon_kind) = weapon_kind {
                        let spawn_count = self.simulation.weapon_manager.spawn_weapons(
                            player,
                            position,
                            velocity,
                            direction,
                            weapon_kind,
                            &self.settings,
                            message_timestamp,
                        );

                        log::trace!("Spawn count for {}: {}", player.name, spawn_count);
                    }
                }
            }
            GameServerMessage::BatchedSmallPosition(message) => {
                for message in &message.positions {
                    if let Some(player) = self
                        .simulation
                        .player_manager
                        .get_by_id_mut(message.player_id)
                    {
                        let message_timestamp = GameTick::from_batched(
                            self.connection.get_game_tick(),
                            message.timestamp,
                        );

                        if player.last_position_timestamp < message_timestamp {
                            let position = Position::new(
                                PixelUnit(message.x as i32).into(),
                                PixelUnit(message.y as i32).into(),
                            );

                            player.velocity = Velocity::new(
                                PositionUnit(message.x_velocity as i32),
                                PositionUnit(message.y_velocity as i32),
                            );

                            let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );

                            player.direction = message.direction;
                            player.last_position_timestamp = message_timestamp;

                            log::trace!(
                                "[BatchedSmall] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
                            );
                        } else {
                            Self::validate_packet_timestamp(
                                self.connection.get_game_tick(),
                                message_timestamp,
                                "small batched",
                            );
                        }
                    } else {
                        log::warn!(
                            "got small batched position packet from bad player id {}",
                            message.player_id.value
                        );
                    }
                }
            }
            GameServerMessage::BatchedLargePosition(message) => {
                for message in &message.positions {
                    if let Some(player) = self
                        .simulation
                        .player_manager
                        .get_by_id_mut(message.player_id)
                    {
                        let message_timestamp = GameTick::from_batched(
                            self.connection.get_game_tick(),
                            message.timestamp,
                        );

                        if player.last_position_timestamp < message_timestamp {
                            let position = Position::new(
                                PixelUnit(message.x as i32).into(),
                                PixelUnit(message.y as i32).into(),
                            );

                            player.velocity = Velocity::new(
                                PositionUnit(message.x_velocity as i32),
                                PositionUnit(message.y_velocity as i32),
                            );

                            let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );

                            player.direction = message.direction;
                            player.last_position_timestamp = message_timestamp;
                            if let Some(status) = message.status {
                                player.status = status;
                            }

                            log::trace!(
                                "[BatchedLarge] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
                            );
                        } else {
                            Self::validate_packet_timestamp(
                                self.connection.get_game_tick(),
                                message_timestamp,
                                "large batched",
                            );
                        }
                    } else {
                        log::warn!(
                            "got large batched position packet from bad player id {}",
                            message.player_id.value
                        );
                    }
                }
            }
            GameServerMessage::PlayerDeath(message) => {
                if let Some(killer) = self.simulation.player_manager.get_by_id(message.killer_id) {
                    if let Some(killed) =
                        self.simulation.player_manager.get_by_id(message.killed_id)
                    {
                        log::debug!("{} killed by {}", killed.name, killer.name);

                        let self_involved = killer.id == self.simulation.player_manager.self_id
                            || killed.id == self.simulation.player_manager.self_id;

                        let color = if self_involved {
                            TextColor::Yellow
                        } else {
                            TextColor::Green
                        };

                        // Only display notifications involving us for now.
                        // TODO: Add setting for toggling this.
                        if self_involved {
                            let bounty = if killer.frequency == killed.frequency {
                                0
                            } else {
                                killed.bounty
                            };

                            self.notifications.push(
                                format!("{}({}) killed by: {}", killed.name, bounty, killer.name),
                                color,
                            );
                        }
                    }
                }

                if let Some(killer) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.killer_id)
                {
                    if (killer.flag_count > 0
                        && message.bounty > self.settings.flag_drop_reset_reward as u16)
                        || message.flag_transfer > 0
                    {
                        killer.flag_remaining_ticks = self.settings.flag_drop_delay as u32;
                    }

                    killer.flag_count += message.flag_transfer;
                    killer.wins = killer.wins.wrapping_add(1);
                    killer.kill_points = killer.kill_points.wrapping_add(message.bounty as i32);
                    killer.bounty = killer
                        .bounty
                        .wrapping_add_signed(self.settings.bounty_increase_for_kill);
                }

                if let Some(killed) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.killed_id)
                {
                    killed.enter_delay =
                        self.settings.enter_delay as u16 + PLAYER_EXPLOSION_DURATION as u16;
                    killed.explosion_remaining_ticks = PLAYER_EXPLOSION_DURATION;
                    killed.losses = killed.losses.wrapping_add(1);
                    killed.flag_count = 0;
                    killed.flag_remaining_ticks = 0;

                    if let Some(killed_position) = killed.position {
                        let was_moving = killed.velocity.x.0 != 0 || killed.velocity.y.0 != 0;

                        if message.prize_id > 0 && was_moving {
                            let x = (killed_position.x.0 / 16000) as u16;
                            let y = (killed_position.y.0 / 16000) as u16;

                            self.prize_manager.spawn_green(
                                x,
                                y,
                                message.prize_id as i32,
                                self.settings.death_prize_time as u32,
                            );
                        }
                    }

                    self.simulation
                        .player_manager
                        .detach_all_children(message.killed_id);
                }

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::ScoreUpdate(message) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    player.kill_points = message.kill_points;
                    player.flag_points = message.flag_points;
                    player.wins = message.kills;
                    player.losses = message.deaths;
                    self.statbox.rebuild(&self.simulation.player_manager);
                }
            }
            GameServerMessage::PlayerFrequencyChange(change) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(change.player_id)
                {
                    player.frequency = change.frequency;
                }

                if self
                    .simulation
                    .player_manager
                    .detach_all_children(change.player_id)
                {
                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: PlayerId::invalid(),
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                }

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::PlayerTeamAndShipChange(change) => {
                let player_count = self.simulation.player_manager.players.len();

                self.simulation
                    .weapon_manager
                    .clear_player_weapons(change.player_id);

                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(change.player_id)
                {
                    let previous_position = if let Some(position) = player.position {
                        position
                    } else {
                        Position::from_tile(0, 0)
                    };

                    player.ship_kind = change.ship_kind;
                    player.frequency = change.frequency;

                    if player.status & StatusFlags::Safety == 0 {
                        player.position = None;
                        player.velocity.clear();
                    }

                    if player.id == self.connection.player_id {
                        if player.ship_kind == ShipKind::Spectator {
                            player.position = Some(previous_position);
                            self.controller =
                                MovementController::Spectate(SpectateController::new());
                        } else {
                            // Clear our spectate target before we get in a ship.
                            if let MovementController::Spectate(_) = &self.controller {
                                let spectate_request = SpectateMessage {
                                    player_id: PlayerId::invalid(),
                                };

                                if let Err(e) = self.connection.send_reliable(&spectate_request) {
                                    log::error!("{e}");
                                }
                            }

                            let mut perform_warp = true;

                            if let MovementController::Ship(_) = &self.controller {
                                if player.status & StatusFlags::Safety != 0 {
                                    perform_warp = false;
                                }
                            }

                            if perform_warp {
                                let rng = VieRng::new(GameTick::now(0).value() as i32);

                                let position = generate_spawn_position(
                                    &self.settings,
                                    &self.map,
                                    player.ship_kind,
                                    change.frequency,
                                    rng,
                                    player_count,
                                );

                                player.position = Some(position);

                                let mut ship_controller = ShipController::new();

                                ship_controller.reset_ship(
                                    &self.settings,
                                    self.connection.get_game_tick(),
                                    player.ship_kind,
                                );

                                self.controller = MovementController::Ship(ship_controller);
                            }
                        }
                    }
                }

                if self
                    .simulation
                    .player_manager
                    .detach_all_children(change.player_id)
                {
                    let request = crate::net::packet::c2s::AttachRequestMessage {
                        player_id: PlayerId::invalid(),
                    };

                    if let Err(e) = self.connection.send_reliable(&request) {
                        log::error!("{e}");
                    }
                }

                self.statbox.rebuild(&self.simulation.player_manager);

                if let MovementController::Spectate(spectate_controller) = &mut self.controller {
                    spectate_controller.handle_ship_change(
                        change,
                        &mut self.simulation.player_manager,
                        &mut self.connection,
                        &self.statbox,
                    );
                }
            }
            GameServerMessage::PrizePickup(message) => {
                self.prize_manager.on_prize_collected(message.x, message.y);

                let frequency = if let Some(player) =
                    self.simulation.player_manager.get_by_id(message.player_id)
                {
                    player.frequency
                } else {
                    0xFFFF
                };

                if message.prize_id != Prize::Warp as i16 {
                    if let Some(me) = self.simulation.player_manager.get_self() {
                        if let MovementController::Ship(ship_controller) = &mut self.controller {
                            let share_limit = self
                                .settings
                                .get_ship_settings(me.ship_kind)
                                .prize_share_limit;

                            if me.bounty < share_limit && me.frequency == frequency {
                                if let Err(e) = apply_prize_id(
                                    &self.settings,
                                    &mut ship_controller.ship,
                                    self.connection.get_game_tick(),
                                    message.prize_id as i32,
                                    Some(&mut self.notifications),
                                    false,
                                ) {
                                    log::error!("{e}");
                                }
                            }
                        }
                    }
                }
            }
            GameServerMessage::CollectedPrize(message) => {
                if let MovementController::Ship(ship_controller) = &mut self.controller {
                    for _ in 0..message.count {
                        if let Err(e) = apply_prize_id(
                            &self.settings,
                            &mut ship_controller.ship,
                            self.connection.get_game_tick(),
                            message.prize_id as i32,
                            Some(&mut self.notifications),
                            false,
                        ) {
                            log::error!("{e}");
                        }

                        ship_controller.ship.bounty = ship_controller.ship.bounty.wrapping_add(1);
                    }

                    if message.prize_id == Prize::Warp as i16 {
                        let player_count = self.simulation.player_manager.players.len();

                        if let Some(me) = self.simulation.player_manager.get_self_mut() {
                            ship_controller.warp(
                                me,
                                &self.settings,
                                &self.map,
                                self.connection.get_game_tick(),
                                player_count,
                                None,
                            );
                        }
                    }
                }
            }
            GameServerMessage::PowerballPosition(message) => {
                let new_pickup = self.simulation.powerball_manager.on_ball_position_message(
                    &mut self.simulation.player_manager,
                    &self.settings,
                    message,
                );

                if new_pickup {
                    let current_tick = self.connection.get_game_tick();

                    if let MovementController::Ship(ship_controller) = &mut self.controller {
                        let bomb_delay = self
                            .settings
                            .get_ship_settings(ship_controller.ship.kind)
                            .bomb_fire_delay;

                        ship_controller.ship.next_bomb_tick = current_tick + bomb_delay as i32;
                        ship_controller.ship.next_bullet_tick = current_tick + bomb_delay as i32;
                    }
                }
            }
            GameServerMessage::MapInformation(info) => {
                log::debug!("Map name: {}", info.filename);

                self.connection.state = ConnectionState::MapDownload;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let map_path = get_zone_path(&self.zone, &info.filename);
                    let map_data = std::fs::read(map_path);

                    if let Ok(map_data) = map_data {
                        let checksum = checksum::crc32(&map_data);

                        if checksum == info.checksum {
                            if let Ok(new_map) =
                                Map::new(&info.filename, &map_data, self.map.door_rng)
                            {
                                if let Some(render_state) = render_state {
                                    render_state.on_map_change(&new_map, &map_data);
                                }

                                self.handle_map_load(new_map, info.checksum);
                            } else {
                                log::debug!("Map read error: failed to load tiles");
                                self.connection.state = ConnectionState::Disconnected;
                            }
                        }
                    }
                }

                if matches!(self.connection.state, ConnectionState::MapDownload) {
                    // Request
                    let map_request = MapRequestMessage {};
                    self.connection.send_reliable(&map_request)?;

                    self.connection.state = ConnectionState::MapDownload;

                    self.map = Map::empty(&info.filename);
                    self.map.checksum = info.checksum;
                }
            }
            GameServerMessage::CompressedMap(compressed) => {
                if compressed.filename == self.map.filename {
                    let inflated = decompress_to_vec_zlib(compressed.data.as_slice());

                    match inflated {
                        Ok(inflated) => {
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let map_path = get_zone_path(&self.zone, &compressed.filename);

                                if let Err(e) = build_zone_directory(&self.zone) {
                                    log::debug!("Error creating zone directory: {}", e);
                                }

                                if let Err(e) = std::fs::write(map_path, inflated.as_slice()) {
                                    log::debug!("Error writing map: {}", e);
                                }
                            }

                            if let Ok(new_map) =
                                Map::new(&self.map.filename, &inflated, self.map.door_rng)
                            {
                                if let Some(render_state) = render_state {
                                    render_state.on_map_change(&new_map, &inflated);
                                }

                                self.handle_map_load(new_map, checksum::crc32(&inflated));
                            } else {
                                log::debug!("Map read error: failed to load tiles");
                            }
                        }
                        Err(e) => {
                            log::debug!("Error: {}", e);
                        }
                    }
                }
            }
            GameServerMessage::ArenaDirectory(directory) => {
                self.statbox
                    .display_select_box(Box::new(SelectBox::new_directory(&directory.entries)));
            }
            GameServerMessage::FlagPosition(message) => {
                self.flag_controller.handle_flag_position_message(message);
            }
            GameServerMessage::FlagClaim(message) => {
                self.flag_controller.handle_flag_claim_message(
                    message,
                    &mut self.simulation.player_manager,
                    &self.map,
                    &self.settings,
                );
            }
            GameServerMessage::FlagDrop(message) => {
                self.flag_controller
                    .handle_flag_drop_message(message, &mut self.simulation.player_manager);
            }
            GameServerMessage::TurfFlagUpdate(message) => {
                self.flag_controller
                    .handle_turf_update_message(message, &self.map);
            }
            GameServerMessage::FlagReward(message) => {
                for reward in &message.rewards {
                    for player in &mut self.simulation.player_manager.players {
                        if player.frequency == reward.frequency {
                            player.flag_points =
                                player.flag_points.wrapping_add(reward.points as i32);
                        }
                    }
                }

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::SetShipCoordinates(message) => {
                let position = Position::from_tile(message.x as i32, message.y as i32);

                if let Some(me) = self.simulation.player_manager.get_self_mut() {
                    if me.ship_kind != ShipKind::Spectator {
                        me.position = Some(position);
                        me.velocity.clear();
                        me.status |= StatusFlags::Flash;
                    }
                }
            }
            GameServerMessage::KothAddTime(message) => {
                if let Some(me) = self.simulation.player_manager.get_self_mut() {
                    me.has_crown = true;
                }

                if let MovementController::Ship(ship_controller) = &mut self.controller {
                    ship_controller.crown_remaining_ticks += message.added_time;
                }
            }
            GameServerMessage::KothSetTimer(message) => {
                if let Some(me) = self.simulation.player_manager.get_self_mut() {
                    me.has_crown = true;
                }

                if let MovementController::Ship(ship_controller) = &mut self.controller {
                    ship_controller.crown_remaining_ticks = message.timer;
                }
            }
            GameServerMessage::KothReset(message) => {
                if !message.player_id.valid() {
                    for player in &mut self.simulation.player_manager.players {
                        player.has_crown = message.add_crown;
                    }

                    if let MovementController::Ship(ship_controller) = &mut self.controller {
                        ship_controller.crown_remaining_ticks = message.timer;
                    }
                } else {
                    if let Some(player) = self
                        .simulation
                        .player_manager
                        .get_by_id_mut(message.player_id)
                    {
                        player.has_crown = message.add_crown;
                    }

                    if message.player_id == self.simulation.player_manager.self_id {
                        if let MovementController::Ship(ship_controller) = &mut self.controller {
                            ship_controller.crown_remaining_ticks = message.timer;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn validate_packet_timestamp(current_tick: GameTick, timestamp: GameTick, ctx: &str) -> bool {
        if current_tick.diff(&timestamp) > 100 {
            log::warn!(
                "Received {} packet timestamp that was far out of range of normal Recv: {} Now: {}",
                ctx,
                timestamp.value(),
                current_tick.value()
            );

            true
        } else {
            false
        }
    }

    fn handle_map_load(&mut self, map: Map, checksum: u32) {
        self.map = map;
        self.map.checksum = checksum;
        self.connection.state = ConnectionState::Playing;

        self.radar.invalidate();
        self.simulation.powerball_paused = false;
    }

    fn process_message(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
        message: ServerMessage,
    ) -> Result<(), ConnectionError> {
        match message {
            ServerMessage::Core(core_message) => {
                self.process_core_message(render_state, &core_message)
            }
            ServerMessage::Game(game_message) => {
                self.process_game_message(render_state, &game_message)
            }
        }
    }

    pub fn get_view_self(&self) -> Option<&Player> {
        let id = if let MovementController::Spectate(spectate_controller) = &self.controller {
            if let Some(spectate_player_id) = spectate_controller.spectate_player_id {
                spectate_player_id
            } else {
                self.connection.player_id
            }
        } else {
            self.connection.player_id
        };

        self.simulation.player_manager.get_by_id(id)
    }

    pub fn get_freq(&self) -> u16 {
        let player_id = if let MovementController::Spectate(spectate_controller) = &self.controller
        {
            let Some(spectate_player_id) = spectate_controller.spectate_player_id else {
                return spectate_controller.last_spectate_freq;
            };

            spectate_player_id
        } else {
            self.connection.player_id
        };

        if let Some(player) = self.simulation.player_manager.get_by_id(player_id) {
            player.frequency
        } else {
            0xFFFF
        }
    }
}
