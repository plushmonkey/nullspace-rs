use smol_str::{SmolStr, format_smolstr};

use crate::{
    client::{Client, MovementController},
    clock::GameTick,
    game_settings::{GameSettings, RenderNameMode},
    map::{ANIMATED_TILE_KIND_COUNT, AnimatedTileKind, TILE_ID_FIRST_DOOR, TILE_ID_FLAG},
    math::{PixelUnit, Position, PositionUnit, Rectangle},
    net::connection::ConnectionState,
    player::{Player, PlayerId, PlayerManager, StatusFlags},
    powerball::{PowerballState, is_team_goal},
    radar::IndicatorFlag,
    render::{
        animation_renderer::get_animation_index,
        colors::ColorRenderableKind,
        game_sprites::{GAME_SPRITE_SHEET_DEFINITIONS, GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        text_renderer::{FontKind, TextAlignment, TextColor},
    },
    ship::ShipKind,
    simulation::{
        game_simulation::SimulationEventKind,
        player_simulation::{PLAYER_EXPLOSION_DURATION, PLAYER_FLASH_DURATION},
    },
    weapon::WeaponKind,
};

pub fn render_game(
    client: &mut Client,
    game_settings: &GameSettings,
    render_state: &mut RenderState,
    sprites: &mut GameSprites,
    menu_open: bool,
) {
    client
        .chat_controller
        .render(render_state, sprites, game_settings);

    client.statbox.render(
        &client.simulation.player_manager,
        render_state,
        sprites,
        game_settings,
    );

    match client.connection.state {
        ConnectionState::Playing | ConnectionState::Disconnected => {
            if let Some(me) = client.simulation.player_manager.get_self() {
                if let Some(me_position) = me.position {
                    render_state.camera.position = me_position.into();
                }
            }

            if client.camera_jitter_time > 0 {
                let strength =
                    client.camera_jitter_time as f32 / client.settings.jitter_time as f32;
                let max_jitter_tiles = (client.settings.jitter_time as f32 / 100.0f32).min(2.0f32);

                let t = (client.camera_jitter_time % 100) as f32 / 100.0f32;

                let x_jitter = (t * 80.75f32).sin() * strength * max_jitter_tiles;
                let y_jitter = (t * 45.63f32).sin() * strength * max_jitter_tiles;

                render_state.camera.position.x += x_jitter;
                render_state.camera.position.y += y_jitter;
            }

            match &client.controller {
                MovementController::Spectate(spectate_controller) => {
                    spectate_controller.render(
                        render_state,
                        sprites,
                        &client.simulation.player_manager,
                        &client.settings,
                        client.connection.get_game_tick(),
                    );
                }
                MovementController::Ship(ship_controller) => {
                    ship_controller.render(
                        &client.simulation.player_manager,
                        render_state,
                        sprites,
                        &client.settings,
                        game_settings,
                        client.connection.get_game_tick(),
                    );

                    if let Some(portal_position) = ship_controller.ship.portal_position {
                        let remaining_ticks = ship_controller.ship.portal_remaining_ticks;
                        let t = (remaining_ticks as f32 * 1.5f32) as u32 % 100;

                        let mut position = portal_position;

                        if t < 25 {
                            position.y.0 += 16000;
                        } else if t < 50 {
                            position.x.0 += 16000;
                            position.y.0 += 16000;
                        } else if t < 75 {
                            position.x.0 += 16000;
                        }

                        client.radar.add_indicator(
                            ColorRenderableKind::RadarPortal,
                            position,
                            client.connection.get_game_tick(),
                            IndicatorFlag::SmallMap,
                        );
                    }
                }
            }

            render_state.render_map = true;
            let view_freq = client.get_freq();

            render_players(client, render_state, sprites, game_settings);
            render_weapons(client, render_state, sprites, game_settings);
            render_powerballs(client, render_state, sprites);

            client.simulation.powerball_manager.render_radar(
                &mut client.radar,
                &client.simulation.player_manager,
                view_freq,
                client.connection.get_game_tick(),
                client.settings.powerball_global_position,
            );

            render_map_animations(client, render_state, sprites);
            client.prize_manager.render(
                render_state,
                sprites,
                &mut client.radar,
                client.connection.get_game_tick(),
            );

            if !menu_open {
                client.notifications.render(render_state);
            }

            let client_flag_ticks = if let Some(me) = client.simulation.player_manager.get_self() {
                me.flag_remaining_ticks
            } else {
                0
            };

            client.flag_controller.render(
                render_state,
                sprites,
                &mut client.radar,
                client.connection.get_game_tick(),
                view_freq,
                client_flag_ticks,
            );

            client.radar.render(
                render_state,
                sprites,
                &client.map,
                client.settings.map_zoom_factor as u16,
                client.get_freq(),
                client.settings.powerball_mode,
                game_settings,
            );

            if client.connection.state == ConnectionState::Disconnected {
                let x = (render_state.width() as f32 * 0.2f32) as i32;
                let y = (render_state.height() as f32 * 0.4f32) as i32;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "WARNING: No data coming from server",
                    x,
                    y,
                    Layer::TopMost,
                    TextColor::Yellow,
                    TextAlignment::Left,
                );

                client.notifications.clear();
            }

            client.lvz_controller.render(render_state, sprites);
        }
        _ => {
            render_state.render_map = false;

            let (x_pixels, y_pixels) = (
                render_state.width() as i32 / 2,
                render_state.height() as i32 - (render_state.height() as i32 / 4),
            );

            let text = if let ConnectionState::MapDownload = client.connection.state {
                "Downloading"
            } else {
                "Entering arena"
            };

            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                text,
                x_pixels,
                y_pixels,
                Layer::TopMost,
                TextColor::Blue,
                TextAlignment::Center,
            );
        }
    }
}

fn get_radar_player_color(
    player_manager: &PlayerManager,
    player_id: PlayerId,
    view_freq: u16,
    is_decoy: bool,
    target_bounty: u16,
) -> ColorRenderableKind {
    let Some(player) = player_manager.get_by_id(player_id) else {
        return ColorRenderableKind::RadarEnemyTarget;
    };

    if player.frequency == view_freq {
        if is_decoy {
            ColorRenderableKind::RadarDecoy
        } else {
            if player.flag_count > 0 || player.carrying_ball {
                ColorRenderableKind::RadarTeammateFlagCarry
            } else {
                ColorRenderableKind::RadarTeammate
            }
        }
    } else {
        if player.flag_count > 0 || player.carrying_ball {
            ColorRenderableKind::RadarEnemyFlagCarry
        } else {
            let mut color = ColorRenderableKind::RadarEnemy;

            if player.bounty >= target_bounty {
                color = ColorRenderableKind::RadarEnemyTarget;
            }

            if player.has_crown {
                color = ColorRenderableKind::RadarEnemyCrown;
            }

            if !is_decoy {
                for child_id in &player.children {
                    if let Some(child) = player_manager.get_by_id(*child_id) {
                        if child.flag_count > 0 || child.carrying_ball {
                            color = ColorRenderableKind::RadarEnemyFlagCarry;
                            break;
                        }
                    }
                }
            }

            color
        }
    }
}

fn get_highest_points_player_id(client: &Client) -> PlayerId {
    let mut highest_points_player = PlayerId::invalid();
    let mut highest_points = 0;

    for player in &client.simulation.player_manager.players {
        let points = player.get_points();
        if points > highest_points {
            highest_points = points;
            highest_points_player = player.id;
        }
    }

    highest_points_player
}

fn render_players(
    client: &mut Client,
    render_state: &mut RenderState,
    sprites: &GameSprites,
    game_settings: &GameSettings,
) {
    let client_position = Position::new(
        PositionUnit(render_state.camera.position.x as i32 * 16000),
        PositionUnit(render_state.camera.position.y as i32 * 16000),
    );

    let client_view_id = if let Some(player) = client.get_view_self() {
        player.id
    } else {
        client.connection.player_id
    };

    let self_id = client.simulation.player_manager.self_id;

    let highest_points_player_id = get_highest_points_player_id(client);

    if let Some(player) = client.simulation.player_manager.get_by_id(client_view_id) {
        let color_kind = if player.flag_count > 0 {
            ColorRenderableKind::RadarSelfFlagCarry
        } else {
            ColorRenderableKind::RadarSelf
        };

        let indicator_flag = if player.ship_kind == ShipKind::Spectator {
            IndicatorFlag::FullMap
        } else {
            IndicatorFlag::SmallMap | IndicatorFlag::FullMap
        };

        client.radar.add_indicator(
            color_kind,
            client_position,
            client.connection.get_game_tick(),
            indicator_flag,
        );
    }

    let using_xradar = match &client.controller {
        MovementController::Ship(ship_controller) => {
            ship_controller.ship.status & StatusFlags::XRadar != 0
        }
        MovementController::Spectate(spectate_controller) => spectate_controller.xradar,
    };

    let view_freq = client.get_freq();
    let current_tick = client.connection.get_game_tick();

    for player in &client.simulation.player_manager.players {
        if player.ship_kind == ShipKind::Spectator {
            continue;
        }

        if player.attach_parent.valid() {
            continue;
        }

        let Some(player_position) = player.position else {
            continue;
        };

        let x_pixels = player_position.x.0 / 1000;
        let y_pixels = player_position.y.0 / 1000;

        if player.explosion_remaining_ticks > 0 {
            if let Some(explosion_renderables) = sprites.get_set(GameSpriteKind::PlayerExplosion) {
                let tick_count = PLAYER_EXPLOSION_DURATION - player.explosion_remaining_ticks;

                let frame_count = explosion_renderables.renderables.len();
                let ticks_per_frame = PLAYER_EXPLOSION_DURATION as usize / frame_count;
                let index = (tick_count as usize / ticks_per_frame).min(frame_count - 1);

                let renderable = &explosion_renderables.renderables[index];

                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::AfterShips,
                );
            }
        } else if player.flash_remaining_ticks > 0 {
            if let Some(flash_renderables) = sprites.get_set(GameSpriteKind::Flash) {
                let tick_count = PLAYER_FLASH_DURATION - player.flash_remaining_ticks;

                let frame_count = flash_renderables.renderables.len();
                let ticks_per_frame = PLAYER_FLASH_DURATION as usize / frame_count;
                let index = (tick_count as usize / ticks_per_frame).min(frame_count - 1);

                let renderable = &flash_renderables.renderables[index];

                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::AfterShips,
                );
            }
        }

        let visible = using_xradar
            || (player.status & StatusFlags::Cloak == 0)
            || player.frequency == view_freq;
        let visible_radar = using_xradar
            || is_radar_visible(&client.simulation.player_manager, player.id, view_freq);

        // Player indicator continues to be on radar even while they are exploding, so add it before the enter delay check.
        if player.id != client_view_id {
            let color_kind = get_radar_player_color(
                &client.simulation.player_manager,
                player.id,
                client.get_freq(),
                false,
                game_settings.radar_target_bounty,
            );

            if visible_radar {
                client.radar.add_indicator(
                    color_kind,
                    player_position,
                    client.connection.get_game_tick(),
                    IndicatorFlag::SmallMap,
                );
            }
        }

        if player.enter_delay > 0 {
            continue;
        }

        if let Some(ship_renderables) = sprites.get_set(GameSpriteKind::Ships) {
            let ship_kind_index = player.ship_kind.network_value() as usize * 40;
            let ship_index = ship_kind_index + player.direction as usize;

            let renderable = &ship_renderables.renderables[ship_index];

            if visible {
                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::Ships,
                );
            }

            let name_x = x_pixels + (renderable.size[0] as i32) / 2;
            let mut name_y = y_pixels + (renderable.size[1] as i32) / 2;

            if player.id == client.simulation.player_manager.self_id && player.carrying_ball {
                if let Some(ball_ticks) = client
                    .simulation
                    .powerball_manager
                    .get_carry_remaining_ticks()
                {
                    let seconds = ball_ticks as f32 / 100.0f32;

                    render_state.draw_world_text(
                        &format_smolstr!("{:.1}", seconds),
                        name_x,
                        name_y,
                        Layer::Ships,
                        TextColor::Red,
                        TextAlignment::Left,
                    );

                    name_y += render_state
                        .text_renderer
                        .character_height(FontKind::Normal);
                }
            }

            if visible {
                if let Some(extra_data) = &player.extra_position_data {
                    let energy = extra_data.energy as u32;
                    let energy_x = x_pixels - (renderable.size[0] as i32) / 2;
                    let energy_y = y_pixels + (renderable.size[1] as i32) / 2;

                    let initial_energy = (client
                        .settings
                        .get_ship_settings(player.ship_kind)
                        .initial_energy) as u32;

                    let energy_color = if energy <= initial_energy / 4 {
                        TextColor::DarkRed
                    } else if energy <= initial_energy / 2 {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    render_state.draw_world_text(
                        &format_smolstr!("{}", energy),
                        energy_x,
                        energy_y,
                        Layer::Ships,
                        energy_color,
                        TextAlignment::Right,
                    );
                } else if player.id == client.simulation.player_manager.self_id {
                    if let MovementController::Ship(ship_controller) = &client.controller {
                        let half_energy = ship_controller.ship.max_energy / 2;

                        if ship_controller.ship.current_energy <= half_energy {
                            let quarter_energy = ship_controller.ship.max_energy / 4;
                            let energy = ship_controller.ship.current_energy / 1000;
                            let energy_color =
                                if ship_controller.ship.current_energy <= quarter_energy {
                                    TextColor::DarkRed
                                } else {
                                    TextColor::Yellow
                                };

                            render_state.draw_world_text(
                                &format_smolstr!("{}", energy),
                                name_x,
                                name_y,
                                Layer::Ships,
                                energy_color,
                                TextAlignment::Left,
                            );

                            name_y += render_state
                                .text_renderer
                                .character_height(FontKind::Normal);
                        }
                    }
                }
            }

            if visible {
                let render_name = match &game_settings.render_name_mode {
                    RenderNameMode::All => true,
                    RenderNameMode::Others => player.id != self_id,
                    RenderNameMode::Off => false,
                };

                if render_name {
                    render_player_name(
                        render_state,
                        sprites,
                        player,
                        name_x,
                        name_y,
                        client.get_freq(),
                        current_tick,
                        player.id == highest_points_player_id,
                        game_settings.render_player_ping,
                    );
                }
            }

            let mut child_y = name_y
                + render_state
                    .text_renderer
                    .character_height(FontKind::Normal);

            for child_id in &player.children {
                if let Some(child) = client.simulation.player_manager.get_by_id(*child_id) {
                    let visible = using_xradar
                        || (child.status & StatusFlags::Cloak == 0)
                        || child.frequency == view_freq;

                    if !visible {
                        continue;
                    }

                    if child.id == client.simulation.player_manager.self_id {
                        if let MovementController::Ship(ship_controller) = &client.controller {
                            let half_energy = ship_controller.ship.max_energy / 2;

                            if ship_controller.ship.current_energy <= half_energy {
                                let quarter_energy = ship_controller.ship.max_energy / 4;
                                let energy = ship_controller.ship.current_energy / 1000;
                                let energy_color =
                                    if ship_controller.ship.current_energy <= quarter_energy {
                                        TextColor::DarkRed
                                    } else {
                                        TextColor::Yellow
                                    };

                                render_state.draw_world_text(
                                    &format_smolstr!("{}", energy),
                                    name_x,
                                    child_y,
                                    Layer::Ships,
                                    energy_color,
                                    TextAlignment::Left,
                                );

                                child_y += render_state
                                    .text_renderer
                                    .character_height(FontKind::Normal);
                            }
                        }
                    } else {
                        if let Some(turret_sprites) = sprites.get_set(GameSpriteKind::TeamTurret) {
                            let renderable =
                                &turret_sprites.renderables[child.direction as usize % 40];

                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::AfterShips,
                            );
                        }
                    }

                    let render_name = match &game_settings.render_name_mode {
                        RenderNameMode::All => true,
                        RenderNameMode::Others => player.id != self_id,
                        RenderNameMode::Off => false,
                    };

                    if render_name {
                        render_player_name(
                            render_state,
                            sprites,
                            child,
                            name_x,
                            child_y,
                            client.get_freq(),
                            current_tick,
                            child.id == highest_points_player_id,
                            game_settings.render_player_ping,
                        );

                        child_y += render_state
                            .text_renderer
                            .character_height(FontKind::Normal);
                    }
                }
            }
        }
    }
}

fn render_player_name(
    render_state: &mut RenderState,
    sprites: &GameSprites,
    player: &Player,
    name_x: i32,
    name_y: i32,
    view_freq: u16,
    current_tick: GameTick,
    highest_score: bool,
    render_ping: bool,
) {
    let ping = if render_ping {
        Some(player.ping as u16)
    } else {
        None
    };

    let (player_name_view, name_color) = get_player_name_view(player, view_freq, ping);

    let mut text_x = name_x;

    if let Some(banner_renderable) = render_state.banner_manager.get_banner(player.id) {
        render_state.sprite_renderer.draw(
            &render_state.camera,
            &banner_renderable,
            name_x,
            name_y + 1,
            Layer::AfterShips,
        );
        text_x += 16;
    }

    let name_width = player_name_view.len() as i32
        * render_state.text_renderer.character_width(FontKind::Normal);

    render_state.draw_world_text(
        &player_name_view,
        text_x,
        name_y,
        Layer::AfterShips,
        name_color,
        TextAlignment::Left,
    );

    let mut x = text_x + name_width + 3;

    if player.has_crown {
        if let Some(crown_sprites) = sprites.get_set(GameSpriteKind::Crown) {
            let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
            let renderable = &crown_sprites.renderables[animation_index];

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x,
                name_y - 5,
                Layer::AfterShips,
            );

            x += renderable.size[0] as i32 + 3;
        }
    }

    if highest_score {
        if let Some(points_shield_sprites) = sprites.get_set(GameSpriteKind::PointsShield) {
            let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
            let renderable = &points_shield_sprites.renderables[animation_index];

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x,
                name_y - 2,
                Layer::AfterShips,
            );
        }
    }
}

fn is_radar_visible(player_manager: &PlayerManager, player_id: PlayerId, view_freq: u16) -> bool {
    if let Some(player) = player_manager.get_by_id(player_id) {
        if player.frequency == view_freq {
            return true;
        }

        if player.status & StatusFlags::Stealth == 0 {
            return true;
        }

        for child_id in &player.children {
            if let Some(child) = player_manager.get_by_id(*child_id) {
                if child.status & StatusFlags::Stealth == 0 {
                    return true;
                }
            }
        }
    }

    false
}

fn get_player_name_view(
    player: &Player,
    view_freq: u16,
    ping: Option<u16>,
) -> (SmolStr, TextColor) {
    let color = if player.frequency == view_freq {
        TextColor::Yellow
    } else {
        if player.flag_count > 0 || player.carrying_ball {
            TextColor::DarkRed
        } else {
            TextColor::Blue
        }
    };

    let text = if player.flag_count > 0 {
        if player.carrying_ball {
            if let Some(ping) = ping {
                format_smolstr!(
                    "{}({}:{})[{}] (Ball)",
                    player.name,
                    player.bounty,
                    player.flag_count,
                    ping * 10
                )
            } else {
                format_smolstr!(
                    "{}({}:{}) (Ball)",
                    player.name,
                    player.bounty,
                    player.flag_count
                )
            }
        } else {
            if let Some(ping) = ping {
                format_smolstr!(
                    "{}({}:{})[{}]",
                    player.name,
                    player.bounty,
                    player.flag_count,
                    ping * 10
                )
            } else {
                format_smolstr!("{}({}:{})", player.name, player.bounty, player.flag_count)
            }
        }
    } else {
        if player.carrying_ball {
            if let Some(ping) = ping {
                format_smolstr!("{}({})[{}] (Ball)", player.name, player.bounty, ping * 10)
            } else {
                format_smolstr!("{}({}) (Ball)", player.name, player.bounty)
            }
        } else {
            if let Some(ping) = ping {
                format_smolstr!("{}({})[{}]", player.name, player.bounty, ping * 10)
            } else {
                format_smolstr!("{}({})", player.name, player.bounty)
            }
        }
    };

    (text, color)
}

fn render_weapons(
    client: &mut Client,
    render_state: &mut RenderState,
    sprites: &GameSprites,
    game_settings: &GameSettings,
) {
    let current_tick = client.connection.get_game_tick();
    let tick_value = current_tick.value();

    let highest_points_player_id = get_highest_points_player_id(client);

    for weapon in &client.simulation.weapon_manager.weapons {
        const WEAPON_DESYNC_TICKS: i32 = 5;

        if weapon.spawn_timestamp == weapon.last_update_tick
            && current_tick.diff(&weapon.last_update_tick) > WEAPON_DESYNC_TICKS
        {
            // We don't want to spawn weapons that haven't been updated to be near the current tick.
            // This fixes the flicker that could occur with large delays of weapon packets.
            continue;
        }

        let x_pixels = weapon.position.x.0 / 1000;
        let y_pixels = weapon.position.y.0 / 1000;

        match weapon.kind {
            WeaponKind::Bullet(bullet) => {
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                    let animation_index = get_animation_index(tick_value, 4, 20);
                    let renderable_index = (bullet.level * 4) as usize + animation_index;
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );
                }
            }
            WeaponKind::BouncingBullet(bouncing) => {
                let animation_index = get_animation_index(tick_value, 4, 20);
                let renderable_index = (bouncing.level * 4) as usize + 5 * 4 + animation_index;
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );
                }
            }
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                let animation_index = get_animation_index(tick_value, 10, 100);

                if bomb.mine {
                    let renderable_index = {
                        if bomb.emp {
                            (bomb.level * 10) as usize + 40
                        } else {
                            (bomb.level * 10) as usize
                        }
                    } + animation_index;

                    if let Some(renderables) = sprites.get_set(GameSpriteKind::Mines) {
                        let renderable = &renderables.renderables[renderable_index];
                        render_state.sprite_renderer.draw_centered(
                            &render_state.camera,
                            renderable,
                            x_pixels,
                            y_pixels,
                            Layer::Weapons,
                        );
                    }
                } else {
                    let renderable_index = {
                        if bomb.emp {
                            (bomb.level * 10) as usize + 40
                        } else {
                            if bomb.remaining_bounces > 0 {
                                (bomb.level * 10) as usize + 80
                            } else {
                                (bomb.level * 10) as usize
                            }
                        }
                    } + animation_index;

                    if let Some(renderables) = sprites.get_set(GameSpriteKind::Bombs) {
                        let renderable = &renderables.renderables[renderable_index];
                        render_state.sprite_renderer.draw_centered(
                            &render_state.camera,
                            renderable,
                            x_pixels,
                            y_pixels,
                            Layer::Weapons,
                        );
                    }
                }
            }
            WeaponKind::Thor(_) => {
                let animation_index = get_animation_index(tick_value, 10, 100);
                let renderable_index = 120 + animation_index;
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Bombs) {
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );
                }
            }
            WeaponKind::Shrapnel(shrapnel) => {
                let animation_index = get_animation_index(tick_value, 10, 60);
                let renderable_index = (shrapnel.level as usize * 10)
                    + (shrapnel.bouncing as usize) * 30
                    + animation_index;
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Shrapnel) {
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );
                }
            }
            WeaponKind::Repel => {
                let ticks_per_frame = 60 / 10;
                let ticks = (weapon.last_update_tick - weapon.spawn_timestamp).value() as usize;
                let animation_index = (ticks / ticks_per_frame) % 10;

                let renderable_index = animation_index;
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Repel) {
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Explosions,
                    );
                }
            }
            WeaponKind::Burst(burst) => {
                let animation_index = get_animation_index(tick_value, 4, 20);
                let renderable_index =
                    (4 * 4) + (burst.active as usize) * (5 * 4) + animation_index;
                if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                    let renderable = &renderables.renderables[renderable_index];
                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );
                }
            }
            WeaponKind::Decoy(decoy) => {
                if let Some(player) = client.simulation.player_manager.get_by_id(weapon.player_id) {
                    let orientation = ((decoy.initial_rotation + 40)
                        - (((player.direction + 40) - decoy.initial_rotation) % 40))
                        % 40;

                    let ship_kind_index = player.ship_kind.network_value() as usize * 40;
                    let ship_index = ship_kind_index + orientation as usize;

                    if let Some(renderables) = sprites.get_set(GameSpriteKind::Ships) {
                        let renderable = &renderables.renderables[ship_index];

                        render_state.sprite_renderer.draw_centered(
                            &render_state.camera,
                            renderable,
                            x_pixels,
                            y_pixels,
                            Layer::Ships,
                        );

                        if player.id != client.simulation.player_manager.self_id {
                            let name_x = x_pixels + (renderable.size[0] as i32) / 2;
                            let name_y = y_pixels + (renderable.size[1] as i32) / 2;

                            if game_settings.render_name_mode != RenderNameMode::Off {
                                render_player_name(
                                    render_state,
                                    sprites,
                                    player,
                                    name_x,
                                    name_y,
                                    client.get_freq(),
                                    client.connection.get_game_tick(),
                                    player.id == highest_points_player_id,
                                    game_settings.render_player_ping,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        match &weapon.kind {
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                if let Some(player) = client.get_view_self() {
                    if player.ship_kind != ShipKind::Spectator {
                        let mut visbility_level = client
                            .settings
                            .get_ship_settings(player.ship_kind)
                            .see_bomb_level;

                        if bomb.mine
                            && !client
                                .settings
                                .get_ship_settings(player.ship_kind)
                                .see_mines
                        {
                            visbility_level = 0;
                        }

                        if visbility_level > 0 && visbility_level <= 1 + bomb.level as u16 {
                            client.radar.add_indicator(
                                ColorRenderableKind::RadarBomb,
                                weapon.position,
                                client.connection.get_game_tick(),
                                IndicatorFlag::SmallMap,
                            );
                        }
                    }
                }
            }
            WeaponKind::Decoy(_) => {
                let color_kind = get_radar_player_color(
                    &client.simulation.player_manager,
                    weapon.player_id,
                    client.get_freq(),
                    true,
                    game_settings.radar_target_bounty,
                );

                client.radar.add_indicator(
                    color_kind,
                    weapon.position,
                    client.connection.get_game_tick(),
                    IndicatorFlag::SmallMap,
                );
            }
            _ => {}
        }
    }
}

pub fn render_powerballs(client: &Client, render_state: &mut RenderState, sprites: &GameSprites) {
    let Some(ball_sprites) = sprites.get_set(GameSpriteKind::Powerball) else {
        return;
    };

    let render_duration = 100;

    let tick_value = client.connection.get_game_tick().value();

    for ball in &client.simulation.powerball_manager.balls {
        match &ball.state {
            PowerballState::World => {
                if ball.remaining_pickup_ticks > 80 {
                    continue;
                }

                let phasing = ball.is_phasing(
                    client.connection.get_game_tick(),
                    client.settings.powerball_pass_delay as i32,
                );

                let x_pixels = ball.position.x.0 / 1000;
                let y_pixels = ball.position.y.0 / 1000;
                let index =
                    get_animation_index(tick_value, 10, render_duration) + phasing as usize * 10;

                let renderable = &ball_sprites.renderables[index];

                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::AfterWeapons,
                );
            }
            PowerballState::Carried => {
                if let Some(carrier) = client.simulation.player_manager.get_by_id(ball.carrier_id) {
                    if carrier.ship_kind == ShipKind::Spectator {
                        continue;
                    }

                    if let Some(position) = carrier.position {
                        let index = get_animation_index(tick_value, 10, render_duration);
                        let heading = carrier.get_heading();
                        let offset = heading
                            * client
                                .settings
                                .get_ship_settings(carrier.ship_kind)
                                .get_radius() as f32;

                        let renderable = &ball_sprites.renderables[index];

                        let x_pixels = position.x.0 / 1000 + offset.x as i32;
                        let y_pixels = position.y.0 / 1000 + offset.y as i32;

                        render_state.sprite_renderer.draw_centered(
                            &render_state.camera,
                            renderable,
                            x_pixels,
                            y_pixels,
                            Layer::AfterWeapons,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn render_trails(
    client: &mut Client,
    render_state: &mut RenderState,
    game_settings: &GameSettings,
) {
    const BULLET_TRAIL_DURATION: u32 = 14;
    const BOMB_TRAIL_DURATION: u32 = 30;

    let current_tick = client.connection.current_tick;

    for weapon in &mut client.simulation.weapon_manager.weapons {
        match &weapon.kind {
            WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) => {
                if !game_settings.render_gun_trails {
                    continue;
                }

                let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                if trail_diff < 2 {
                    continue;
                }

                let start_index = (bullet.level as usize * 14) + 3 * 14;
                let (x_pixels, y_pixels) = weapon.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Gradient,
                    current_tick,
                    start_index,
                    start_index + 14,
                    BULLET_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                weapon.last_trail_tick = current_tick;
            }
            WeaponKind::Shrapnel(shrapnel) => {
                if !game_settings.render_gun_trails {
                    continue;
                }

                let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                if trail_diff < 2 {
                    continue;
                }

                let start_index = (shrapnel.level as usize * 14) + 3 * 14;
                let (x_pixels, y_pixels) = weapon.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Gradient,
                    current_tick,
                    start_index,
                    start_index + 14,
                    BULLET_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                weapon.last_trail_tick = current_tick;
            }
            WeaponKind::Burst(_) => {
                if !game_settings.render_gun_trails {
                    continue;
                }

                let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                if trail_diff < 2 {
                    continue;
                }

                let start_index = 5 * 14;
                let (x_pixels, y_pixels) = weapon.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Gradient,
                    current_tick,
                    start_index,
                    start_index + 14,
                    BULLET_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                weapon.last_trail_tick = current_tick;
            }
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                if !game_settings.render_bomb_trails {
                    continue;
                }

                let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                if trail_diff < 5 {
                    continue;
                }

                let start_index = bomb.level as usize * 10;
                let (x_pixels, y_pixels) = weapon.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Trail,
                    current_tick,
                    start_index,
                    start_index + 10,
                    BOMB_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                weapon.last_trail_tick = current_tick;
            }
            WeaponKind::Thor(_) => {
                if !game_settings.render_bomb_trails {
                    continue;
                }

                let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                if trail_diff < 5 {
                    continue;
                }

                let start_index = 4 * 10;
                let (x_pixels, y_pixels) = weapon.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Trail,
                    current_tick,
                    start_index,
                    start_index + 10,
                    BOMB_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                weapon.last_trail_tick = current_tick;
            }
            _ => {}
        }
    }

    for ball in &mut client.simulation.powerball_manager.balls {
        if !game_settings.render_ball_trails {
            continue;
        }

        let trail_diff = current_tick.diff(&ball.last_trail_tick);

        if trail_diff < 3 {
            continue;
        }

        if ball.velocity.x.0 != 0 || ball.velocity.y.0 != 0 {
            let start_index = 20;
            let (x_pixels, y_pixels) = ball.position.to_pixels();

            render_state.animation_renderer.add(
                GameSpriteKind::Powerball,
                current_tick,
                start_index,
                start_index + 10,
                BOMB_TRAIL_DURATION,
                x_pixels,
                y_pixels,
                Layer::Weapons,
            );

            ball.last_trail_tick = current_tick;
        }
    }
}

pub fn render_map_animations(
    client: &Client,
    render_state: &mut RenderState,
    sprites: &GameSprites,
) {
    const OFFSCREEN_PIXELS: i32 = 8 * 16;
    let (screen_width, screen_height) = (render_state.width() as i32, render_state.height() as i32);
    let half_width = (screen_width / 2) + OFFSCREEN_PIXELS;
    let half_height = (screen_height / 2) + OFFSCREEN_PIXELS;

    let center_x = (render_state.camera.position.x * 16.0f32) as i32;
    let center_y = (render_state.camera.position.y * 16.0f32) as i32;

    let view_min = Position::from_pixels(
        PixelUnit(center_x - half_width),
        PixelUnit(center_y - half_height),
    );
    let view_max = Position::from_pixels(
        PixelUnit(center_x + half_width),
        PixelUnit(center_y + half_height),
    );

    let view_rect = Rectangle::new(view_min, view_max);
    const ANIMATED_TILE_MAPPING: [(GameSpriteKind, usize); ANIMATED_TILE_KIND_COUNT - 2] = [
        (GameSpriteKind::Goal, 50),
        (GameSpriteKind::AsteroidSmall, 150),
        (GameSpriteKind::AsteroidSmall2, 150),
        (GameSpriteKind::AsteroidLarge, 150),
        (GameSpriteKind::SpaceStation, 100),
        (GameSpriteKind::Wormhole, 250),
        (GameSpriteKind::Flag, 100),
    ];

    let tick_value = client.connection.get_game_tick().value();

    // Loop over the animated tiles except for flags. Flags require extra game state to determine how they should be rendered.
    // Skip the last two because bricks are handled differently.
    for i in 0..ANIMATED_TILE_KIND_COUNT - 3 {
        let tiles = &client.map.animated_tiles[i];

        if tiles.is_empty() {
            continue;
        }

        let (game_sprite_kind, duration) = ANIMATED_TILE_MAPPING[i];

        let Some(sprite_set) = sprites.get_set(game_sprite_kind) else {
            continue;
        };

        let frames = GAME_SPRITE_SHEET_DEFINITIONS[game_sprite_kind as usize];
        let frames = frames.0 * frames.1;

        for tile in tiles {
            let x_pixels = tile.x() as i32 * 16;
            let y_pixels = tile.y() as i32 * 16;
            let position = Position::from_pixels(PixelUnit(x_pixels), PixelUnit(y_pixels));

            if !view_rect.contains(position) {
                continue;
            }

            let renderable = match &game_sprite_kind {
                GameSpriteKind::Goal => {
                    const GOAL_FRAMES: usize = 9;

                    let enemy_goal =
                        !is_team_goal(client.settings.powerball_mode, position, client.get_freq());

                    // First half of goal frames are team goals, second half are enemy.
                    // This increments the animation index to point into the appropriate set.
                    let animation_index = get_animation_index(tick_value, GOAL_FRAMES, duration)
                        + enemy_goal as usize * GOAL_FRAMES;

                    &sprite_set.renderables[animation_index]
                }
                _ => {
                    let animation_index =
                        get_animation_index(tick_value, frames as usize, duration);
                    &sprite_set.renderables[animation_index]
                }
            };

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x_pixels,
                y_pixels,
                Layer::AfterTiles,
            );
        }
    }

    let map_enemy_bricks = &client.map.animated_tiles[AnimatedTileKind::EnemyBrick as usize];
    let map_team_bricks = &client.map.animated_tiles[AnimatedTileKind::TeamBrick as usize];

    // Render the bricks that are part of the map.
    if !map_enemy_bricks.is_empty() || !map_team_bricks.is_empty() {
        if let Some(brick_sprites) = sprites.get_set(GameSpriteKind::Brick) {
            let sets = [map_enemy_bricks, map_team_bricks];

            for i in 0..sets.len() {
                let set = sets[i];

                for tile in set {
                    let x_pixels = tile.x() as i32 * 16;
                    let y_pixels = tile.y() as i32 * 16;
                    let position = Position::from_pixels(PixelUnit(x_pixels), PixelUnit(y_pixels));

                    if !view_rect.contains(position) {
                        continue;
                    }

                    let animation_index = get_animation_index(tick_value, 10, 10 * 10);

                    let renderable = &brick_sprites.renderables[i * 10 + animation_index];

                    render_state.sprite_renderer.draw(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::AfterTiles,
                    );
                }
            }
        }
    }

    let client_freq = client.get_freq();

    if let Some(brick_sprites) = sprites.get_set(GameSpriteKind::Brick) {
        for brick in &client.map.bricks {
            let index = get_animation_index(tick_value, 10, 10 * 10)
                + (client_freq == brick.frequency) as usize * 10;

            let renderable = &brick_sprites.renderables[index];
            let x_pixels = brick.tile.x() as i32 * 16;
            let y_pixels = brick.tile.y() as i32 * 16;

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x_pixels,
                y_pixels,
                Layer::Tiles,
            );
        }
    }

    if render_state
        .map_renderer
        .door_spriteset
        .renderables
        .is_empty()
    {
        return;
    }

    for door_tile in &client.map.doors {
        let current_id = client.map.get_tile(door_tile.x(), door_tile.y());

        // The map mutates its door tiles into a flag tile if it's considered open, so skip rendering it.
        if current_id == TILE_ID_FLAG {
            continue;
        }

        let x_pixels = door_tile.x() as i32 * 16;
        let y_pixels = door_tile.y() as i32 * 16;
        let position = Position::from_pixels(PixelUnit(x_pixels), PixelUnit(y_pixels));

        if !view_rect.contains(position) {
            continue;
        }

        // There are two door sets and each one is 4 frames. Dividing by 4 will give us the first or second half depending on tile id.
        let set = (door_tile.id() - TILE_ID_FIRST_DOOR) as usize / 4;
        let frame = get_animation_index(tick_value, 4, 40);

        let index = (set * 4) + frame;

        let renderable = &render_state.map_renderer.door_spriteset.renderables[index];

        render_state.sprite_renderer.draw(
            &render_state.camera,
            renderable,
            x_pixels,
            y_pixels,
            Layer::Tiles,
        );
    }
}

pub fn render_explosions(client: &mut Client, render_state: &mut RenderState) {
    for event in &client.simulation.events {
        match &event.kind {
            SimulationEventKind::WeaponExplosion(explosion) => {
                let x_pixels = explosion.position.x.0 / 1000;
                let y_pixels = explosion.position.y.0 / 1000;

                match &explosion.kind {
                    WeaponKind::Bullet(_)
                    | WeaponKind::BouncingBullet(_)
                    | WeaponKind::Shrapnel(_) => {
                        render_state.animation_renderer.add(
                            GameSpriteKind::BulletExplosion,
                            event.tick,
                            0,
                            7,
                            7 * 6,
                            x_pixels,
                            y_pixels,
                            Layer::AfterShips,
                        );
                    }
                    WeaponKind::Bomb(bomb)
                    | WeaponKind::ProximityBomb(bomb)
                    | WeaponKind::Thor(bomb) => {
                        let (kind, frames, duration) = if bomb.emp {
                            (GameSpriteKind::EmpExplosion, 10, 40)
                        } else {
                            (GameSpriteKind::BombExplosion, 44, 44 * 3)
                        };

                        render_state.animation_renderer.add(
                            kind,
                            event.tick,
                            0,
                            frames,
                            duration,
                            x_pixels,
                            y_pixels,
                            Layer::Explosions,
                        );

                        const RADAR_EXPLOSION_DURATION: i32 = 132;

                        if explosion.frequency == client.get_freq() {
                            client.radar.add_indicator(
                                ColorRenderableKind::RadarExplosion,
                                explosion.position,
                                client.connection.get_game_tick() + RADAR_EXPLOSION_DURATION,
                                IndicatorFlag::SmallMap,
                            );
                        } else {
                            // We render the RadarBomb color if have visibility of bombs because we terminate weapons differently than Continuum.
                            // Continuum keeps the weapon around with its RadarBomb still animating during the explosion, but we remove the weapon
                            // and have to do it manually here.
                            if let Some(player) = client.get_view_self() {
                                if player.ship_kind != ShipKind::Spectator {
                                    let visbility_level = client
                                        .settings
                                        .get_ship_settings(player.ship_kind)
                                        .see_bomb_level;

                                    if visbility_level > 0
                                        && visbility_level <= 1 + bomb.level as u16
                                    {
                                        client.radar.add_indicator(
                                            ColorRenderableKind::RadarBomb,
                                            explosion.position,
                                            client.connection.get_game_tick()
                                                + RADAR_EXPLOSION_DURATION,
                                            IndicatorFlag::SmallMap,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
