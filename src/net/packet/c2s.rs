use std::collections::HashMap;

use crate::checksum::weapon_checksum;
use crate::clock::GameTick;
use crate::net::packet::s2c::ChatKind;
use crate::net::packet::{Packet, Serialize};
use crate::player::PlayerId;
use crate::ship::ShipKind;

// Core packets

pub enum EncryptionClientVersion {
    Subspace,
    ContinuumClassic,
    Continuum,
}

pub struct EncryptionRequestMessage {
    pub key: u32,
    pub version: EncryptionClientVersion,
}

impl EncryptionRequestMessage {
    pub fn new(key: u32) -> Self {
        Self {
            key,
            version: EncryptionClientVersion::Subspace,
        }
    }
}

impl Serialize for EncryptionRequestMessage {
    fn serialize(&self) -> Packet {
        let version = match &self.version {
            EncryptionClientVersion::Subspace => 0x01,
            EncryptionClientVersion::ContinuumClassic => 0x10,
            EncryptionClientVersion::Continuum => 0x11,
        } as u16;

        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x01)
            .concat_u32(self.key)
            .concat_u16(version)
    }
}

// Game packets
pub enum ArenaRequest {
    AnyPublic,
    SpecificPublic(u16),
    Name([u8; 16]),
}

// 0x01
pub struct ArenaJoinMessage {
    pub ship_kind: ShipKind,
    pub resolution_x: u16,
    pub resolution_y: u16,
    pub arena_request: ArenaRequest,
}

impl ArenaJoinMessage {
    pub fn new(
        ship_kind: ShipKind,
        resolution_x: u16,
        resolution_y: u16,
        arena_request: ArenaRequest,
    ) -> Self {
        Self {
            ship_kind,
            resolution_x,
            resolution_y,
            arena_request,
        }
    }
}

impl Serialize for ArenaJoinMessage {
    fn serialize(&self) -> Packet {
        let mut arena_number = 0xFFFF;
        let mut arena_name = [0; 16];

        match self.arena_request {
            ArenaRequest::AnyPublic => {}
            ArenaRequest::SpecificPublic(number) => {
                arena_number = number;
            }
            ArenaRequest::Name(name) => {
                if name.len() <= 16 {
                    arena_name[..name.len()].copy_from_slice(&name);
                    arena_number = 0xFFFD;
                }
            }
        }

        let ship = self.ship_kind.network_value();

        Packet::empty()
            .concat_u8(0x01)
            .concat_u8(ship)
            .concat_u16(0x01) // Audio
            .concat_u16(self.resolution_x)
            .concat_u16(self.resolution_y)
            .concat_u16(arena_number)
            .concat_bytes(&arena_name)
    }
}

// 0x02
pub struct LeaveArenaMessage {}

impl Serialize for LeaveArenaMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x02)
    }
}

// 0x03
pub struct PositionMessage {
    pub direction: u8,
    pub timestamp: GameTick,
    pub x_position: u16,
    pub y_position: u16,
    pub x_velocity: i16,
    pub y_velocity: i16,
    pub togglables: u8,
    pub bounty: u16,
    pub energy: u16,
    pub weapon_info: u16,
}

impl Serialize for PositionMessage {
    fn serialize(&self) -> Packet {
        let mut packet = Packet::empty()
            .concat_u8(0x03)
            .concat_u8(self.direction)
            .concat_u32(self.timestamp.value())
            .concat_i16(self.x_velocity)
            .concat_u16(self.y_position)
            .concat_u8(0x00) // Checksum
            .concat_u8(self.togglables)
            .concat_u16(self.x_position)
            .concat_i16(self.y_velocity)
            .concat_u16(self.bounty)
            .concat_u16(self.energy)
            .concat_u16(self.weapon_info);

        packet.data[10] = weapon_checksum(&packet.data[..packet.size]);

        packet
    }
}

// 0x05
pub struct DeathMessage {
    pub killer_id: PlayerId,
    pub bounty: u16,
}

impl Serialize for DeathMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x05)
            .concat_u16(self.killer_id.value)
            .concat_u16(self.bounty)
    }
}

// 0x06
pub struct SendChatMessage<'a> {
    pub kind: ChatKind,
    pub sound: u8,
    pub target_id: PlayerId,
    pub text: &'a str,
}

impl<'a> SendChatMessage<'a> {
    pub fn public(text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::Public,
            sound: 0,
            target_id: PlayerId::new(0),
            text,
        }
    }

    // target must be in the same arena
    pub fn private(target_id: PlayerId, text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::Private,
            sound: 0,
            target_id,
            text,
        }
    }

    // text must be formatted as :target:message
    pub fn remote_private(text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::RemotePrivate,
            sound: 0,
            target_id: PlayerId::new(0),
            text,
        }
    }

    pub fn team(text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::Team,
            sound: 0,
            target_id: PlayerId::new(0),
            text,
        }
    }

    pub fn frequency(frequency: u16, text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::Team,
            sound: 0,
            target_id: PlayerId::new(frequency),
            text,
        }
    }

    // text must be formatted as 1;message
    pub fn channel(text: &'a str) -> SendChatMessage<'a> {
        SendChatMessage {
            kind: ChatKind::Channel,
            sound: 0,
            target_id: PlayerId::new(0),
            text,
        }
    }
}

impl<'a> Serialize for SendChatMessage<'a> {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x06)
            .concat_u8(self.kind as u8)
            .concat_u8(self.sound)
            .concat_player_id(self.target_id)
            .concat_str(self.text)
    }
}

// 0x07
pub struct TakePrizeMessage {
    pub timestamp: GameTick,
    pub x: u16,
    pub y: u16,
    pub prize: i16,
}

impl Serialize for TakePrizeMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x07)
            .concat_u32(self.timestamp.value())
            .concat_u16(self.x)
            .concat_u16(self.y)
            .concat_i16(self.prize)
    }
}

// 0x08
pub struct SpectateMessage {
    pub player_id: PlayerId,
}

impl Serialize for SpectateMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x08)
            .concat_u16(self.player_id.value)
    }
}

// 0x09
pub struct PasswordMessage {
    pub new_user: bool,
    pub name: [u8; 32],
    pub password: [u8; 32],
    pub machine_id: u32,
    pub timezone: u16,
    pub version: u16,
    pub permission_id: u32,
}

// Client features that extend beyond the VIE client so the server knows we support them.
// Works with SubspaceServer.NET:
// https://github.com/gigamon-dev/SubspaceServer/blob/master/src/Packets/Game/LoginPacket.cs
#[allow(nonstandard_style)]
pub mod ClientFeatures {
    pub const WatchDamage: u16 = 1 << 0;
    pub const BatchPositions: u16 = 1 << 1;
    pub const WarpTo: u16 = 1 << 2;
    pub const Lvz: u16 = 1 << 3;
    pub const Redirect: u16 = 1 << 4;
    pub const SelectBox: u16 = 1 << 5;
    pub const Continuum: u16 = WatchDamage | BatchPositions | WarpTo | Lvz | Redirect | SelectBox;
}

impl PasswordMessage {
    pub fn new(
        name: &str,
        password: &str,
        new_user: bool,
        machine_id: u32,
        timezone: u16,
        version: u16,
        permission_id: u32,
    ) -> Self {
        let mut new_name = [0; 32];
        new_name[..name.len()].copy_from_slice(name.as_bytes());

        let mut new_password = [0; 32];
        new_password[..password.len()].copy_from_slice(password.as_bytes());

        Self {
            new_user,
            name: new_name,
            password: new_password,
            machine_id,
            timezone,
            version,
            permission_id,
        }
    }
}

impl Serialize for PasswordMessage {
    fn serialize(&self) -> Packet {
        let new_user = if self.new_user { 1 } else { 0 };

        let mut client_features: u16 = 0;

        //client_features |= ClientFeatures::BatchPositions; // Temporarily disable until we are stable enough to process them.
        client_features |= ClientFeatures::WarpTo;
        client_features |= ClientFeatures::Lvz;

        Packet::empty()
            .concat_u8(0x09)
            .concat_u8(new_user)
            .concat_bytes(&self.name)
            .concat_bytes(&self.password)
            .concat_u32(self.machine_id)
            .concat_u8(0x04) // ConnectType
            .concat_u16(self.timezone)
            .concat_u16(0x00)
            .concat_u16(self.version)
            .concat_u16(444)
            .concat_u16(client_features)
            .concat_u32(555)
            .concat_u32(self.permission_id)
            .concat_u32(0x00)
            .concat_u32(0x00)
            .concat_u32(0x00)
    }
}

// 0x0B
pub struct SubspaceExeRequestMessage {}

impl Serialize for SubspaceExeRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x0B)
    }
}

// 0x0C
pub struct MapRequestMessage {}

impl Serialize for MapRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x0C)
    }
}

// 0x0D
pub struct NewsRequestMessage {}

impl Serialize for NewsRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x0D)
    }
}

// 0x0E
pub struct SendVoiceMessage {
    pub index: u8,
    pub player_id: PlayerId,
    pub data: Vec<u8>,
}

impl Serialize for SendVoiceMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x0E)
            .concat_u8(self.index)
            .concat_u16(self.player_id.value)
            .concat_bytes(&self.data[..])
    }
}

// 0x0F
pub struct FrequencyChangeMessage {
    pub frequency: u16,
}

impl Serialize for FrequencyChangeMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x0F).concat_u16(self.frequency)
    }
}

// 0x10
pub struct AttachRequestMessage {
    pub player_id: PlayerId,
}

impl Serialize for AttachRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x10)
            .concat_u16(self.player_id.value)
    }
}

// 0x13
pub struct FlagRequestMessage {
    pub flag_id: u16,
}

impl Serialize for FlagRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x13).concat_u16(self.flag_id)
    }
}

// 0x14
pub struct DetachAllRequestMessage {}

impl Serialize for DetachAllRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x14)
    }
}

// 0x15
pub struct DropFlagsMessage {}

impl Serialize for DropFlagsMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x15)
    }
}

// 0x16
pub struct SendFileMessage<'a> {
    pub filename: String,
    pub data: &'a [u8],
}

impl<'a> SendFileMessage<'a> {
    pub fn serialize(&self, out: &mut [u8]) {
        out[0] = 0x16;
        let mut name_len = self.filename.len();
        if name_len > 16 {
            name_len = 16;
        }

        for i in 0..name_len {
            out[i + 1] = self.filename.as_bytes()[i];
        }

        out[17..self.data.len() + 17].copy_from_slice(self.data);
    }
}

pub enum RegistrationSex {
    Male,
    Female,
}

impl RegistrationSex {
    pub fn value(&self) -> u8 {
        match self {
            RegistrationSex::Male => 'M' as u8,
            RegistrationSex::Female => 'F' as u8,
        }
    }
}

// 0x17
pub struct RegistrationFormMessage {
    pub real_name: String,
    pub email: String,
    pub city: String,
    pub state: String,
    pub sex: RegistrationSex,
    pub age: u8,
    pub connecting_from_home: bool,
    pub connecting_from_work: bool,
    pub connecting_from_school: bool,
}

impl RegistrationFormMessage {
    pub fn new(
        real_name: &str,
        email: &str,
        city: &str,
        state: &str,
        sex: RegistrationSex,
        age: u8,
    ) -> Self {
        Self {
            real_name: real_name.to_owned(),
            email: email.to_owned(),
            city: city.to_owned(),
            state: state.to_owned(),
            sex,
            age,
            connecting_from_home: true,
            connecting_from_work: false,
            connecting_from_school: false,
        }
    }

    pub fn serialize(&self, out: &mut [u8]) {
        let mut packet = Packet::empty().concat_u8(0x17);

        packet.write_fixed_str(&self.real_name, 32);
        packet.write_fixed_str(&self.email, 64);
        packet.write_fixed_str(&self.city, 32);
        packet.write_fixed_str(&self.state, 24);

        packet.write_u8(self.sex.value());
        packet.write_u8(self.connecting_from_home as u8);
        packet.write_u8(self.connecting_from_work as u8);
        packet.write_u8(self.connecting_from_school as u8);

        packet.write_u32(0); // Processor type
        packet.write_u32(0);

        out[..packet.size].copy_from_slice(&packet.data[..packet.size]);
    }
}

// 0x18
pub struct RequestShipMessage {
    pub ship_kind: ShipKind,
}

impl Serialize for RequestShipMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x18)
            .concat_u8(self.ship_kind.network_value())
    }
}

// 0x19
pub struct SetBannerMessage<'a> {
    pub data: &'a [u8; 96],
}

impl<'a> Serialize for SetBannerMessage<'a> {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x19).concat_bytes(self.data)
    }
}

// 0x1A
pub struct SecurityMessage {
    pub weapon_count: u32,
    pub settings_checksum: u32,
    pub exe_checksum: u32,
    pub level_checksum: u32,
    pub s2c_slow_total: u32,
    pub s2c_fast_total: u32,
    pub s2c_slow_current: u16,
    pub s2c_fast_current: u16,
    pub s2c_reliable_out: u16,
    pub ping: u16,
    pub ping_average: u16,
    pub ping_low: u16,
    pub ping_high: u16,
    pub slow_frame: bool,
}

impl SecurityMessage {
    pub fn new(
        weapon_count: u32,
        settings_checksum: u32,
        exe_checksum: u32,
        level_checksum: u32,
        ping: u16,
        ping_average: u16,
        ping_low: u16,
        ping_high: u16,
    ) -> SecurityMessage {
        SecurityMessage {
            weapon_count,
            settings_checksum,
            exe_checksum,
            level_checksum,
            s2c_slow_total: 0,
            s2c_fast_total: 0,
            s2c_slow_current: 0,
            s2c_fast_current: 0,
            s2c_reliable_out: 0,
            ping,
            ping_average,
            ping_low,
            ping_high,
            slow_frame: false,
        }
    }
}

impl Serialize for SecurityMessage {
    fn serialize(&self) -> Packet {
        let slow_frame = if self.slow_frame { 1 } else { 0 };

        Packet::empty()
            .concat_u8(0x1A)
            .concat_u32(self.weapon_count)
            .concat_u32(self.settings_checksum)
            .concat_u32(self.exe_checksum)
            .concat_u32(self.level_checksum)
            .concat_u32(self.s2c_slow_total)
            .concat_u32(self.s2c_fast_total)
            .concat_u16(self.s2c_slow_current)
            .concat_u16(self.s2c_fast_current)
            .concat_u16(self.s2c_reliable_out)
            .concat_u16(self.ping)
            .concat_u16(self.ping_average)
            .concat_u16(self.ping_low)
            .concat_u16(self.ping_high)
            .concat_u8(slow_frame)
    }
}

#[derive(Copy, Clone)]
pub enum SecurityViolation {
    Ok = 0,
    SlowFramerate,
    CurrentEnergyOverflow,
    TopEnergyOverflow,
    UnprizedMaxEnergy,
    TopRechargeOverflow,
    UnprizedMaxRecharge,
    BurstOveruse,
    RepelOveruse,
    DecoyOveruse,
    ThorOveruse,
    BrickOveruse,
    UnprizedStealth,
    UnprizedCloak,
    UnprizedXRadar,
    UnprizedAntiwarp,
    UnprizedProximity,
    UnprizedBouncingBullets,
    UnprizedMaxGuns,
    UnprizedMaxBombs,
    SuperShieldOveruse,
    SavedShipItems,
    SavedShipWeapons,
    LoginChecksum,
    Unknown,
    SavedShipChecksum,
    Softice,
    DataChecksum,
    ParameterMismatch,
    UnknownIntegrity,
    HighLatency = 0x3C,
}

// 0x1B
pub struct SecurityViolationMessage {
    pub violation: SecurityViolation,
}

impl Serialize for SecurityViolationMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x1B)
            .concat_u8(self.violation as u8)
    }
}

// 0x1C
pub struct DropBrickMessage {
    pub x: u16,
    pub y: u16,
}

impl Serialize for DropBrickMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x1C)
            .concat_u16(self.x)
            .concat_u16(self.y)
    }
}

// 0x1D
pub struct ChangeArenaSettingsMessage<'a> {
    // Key is 'Category:Key', value is any value stored as a string.
    pub changes: &'a HashMap<String, String>,
}

impl<'a> ChangeArenaSettingsMessage<'a> {
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![];

        // Type byte plus 2 bytes for ending null bytes indicating packet end
        let mut out_size = 1 + 2;

        for (key, value) in self.changes {
            out_size += key.len() + value.len() + 2;
        }

        out.resize(out_size, 0);

        out[0] = 0x1D;
        let mut current = &mut out[1..];

        for (key, value) in self.changes {
            let self_size = key.len() + value.len() + 2;

            current[..key.len()].copy_from_slice(key.as_bytes());
            current[key.len()] = b':';
            current[key.len() + 1..key.len() + 1 + value.len()].copy_from_slice(value.as_bytes());

            current[self_size] = 0;

            current = &mut current[self_size..];
        }

        out
    }
}

// 0x1E
pub struct KothEndMessage {}

impl Serialize for KothEndMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x1E)
    }
}

// 0x1F
pub struct PowerballFireMessage {
    pub ball_id: u8,
    pub x: u16,
    pub y: u16,
    pub x_velocity: i16,
    pub y_velocity: i16,
    pub player_id: PlayerId,
    pub timestamp: GameTick,
}

impl Serialize for PowerballFireMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x1F)
            .concat_u8(self.ball_id)
            .concat_u16(self.x)
            .concat_u16(self.y)
            .concat_i16(self.x_velocity)
            .concat_i16(self.y_velocity)
            .concat_u16(self.player_id.value)
            .concat_u32(self.timestamp.value())
    }
}

// 0x20
pub struct PowerballRequestMessage {
    pub ball_id: u8,
    pub timestamp: GameTick,
}

impl Serialize for PowerballRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x20)
            .concat_u8(self.ball_id)
            .concat_u32(self.timestamp.value())
    }
}

// 0x21
pub struct PowerballScoreMessage {
    pub ball_id: u8,
    pub timestamp: GameTick,
}

impl Serialize for PowerballScoreMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x21)
            .concat_u8(self.ball_id)
            .concat_u32(self.timestamp.value())
    }
}

// 0x22
pub struct SecurityViolationExtMessage {
    pub unknown: u32,
    pub settings_checksum: u32,
    pub code_checksum1: u32,
    pub code_checksum2: u32,
    pub violation: SecurityViolation,
}

impl Serialize for SecurityViolationExtMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x22)
            .concat_u32(self.unknown)
            .concat_u32(self.settings_checksum)
            .concat_u32(self.code_checksum1)
            .concat_u32(self.code_checksum2)
            .concat_u8(self.violation as u8)
    }
}
