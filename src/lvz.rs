use std::{collections::HashMap, ffi::CStr};

use image::EncodableLayout;
use miniz_oxide::inflate::decompress_to_vec_zlib;
use thiserror::Error;

use crate::render::{
    animation_renderer::get_animation_index,
    game_sprites::GameSprites,
    layer::Layer,
    render_state::{ReferencePoint, RenderState},
    sprite_renderer::{SheetIndex, SpriteRenderable},
    texture::Texture,
};

#[derive(Error, Debug)]
pub enum LvzError {
    #[error("Unexpected end of file")]
    Eof,

    #[error("Invalid header")]
    InvalidHeader,

    #[error("Invalid section header")]
    InvalidSectionHeader,

    #[error("Invalid object header")]
    InvalidObjectHeader,

    #[error("Invalid compression")]
    InvalidCompression,
}

#[derive(Copy, Clone, PartialEq)]
pub enum DisplayMode {
    ShowAlways,
    EnterZone,
    EnterArena,
    Kill,
    Death,
    ServerControlled,
}

impl DisplayMode {
    pub fn from_value(value: u16) -> Self {
        match value {
            0 => Self::ShowAlways,
            1 => Self::EnterZone,
            2 => Self::EnterArena,
            3 => Self::Kill,
            4 => Self::Death,
            _ => Self::ServerControlled,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct LvzHeader {
    pub magic: u32,
    pub section_count: u32,
}

impl LvzHeader {
    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 8 {
            return None;
        }

        let header = Self {
            magic: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            section_count: u32::from_le_bytes(data[4..8].try_into().unwrap()),
        };

        Some((header, &data[8..]))
    }
}

pub struct LvzSectionHeader {
    pub magic: u32,
    pub decompressed_size: u32,
    pub timestamp: u32,
    pub compressed_size: u32,
    pub filename: String,
}

impl LvzSectionHeader {
    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 16 {
            log::warn!("LvzSectionHeader: Not enough data in section");
            return None;
        }

        let mut header = Self {
            magic: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            decompressed_size: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            timestamp: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            compressed_size: u32::from_le_bytes(data[12..16].try_into().unwrap()),
            filename: String::new(),
        };

        let filename = match CStr::from_bytes_until_nul(&data[16..]) {
            Ok(filename) => filename,
            Err(_) => return None,
        };

        header.filename = match filename.to_str() {
            Ok(filename) => filename.to_string(),
            Err(_) => return None,
        };

        let filename_size = header.filename.len() + 1;
        let data_consumed = 16 + filename_size;

        Some((header, &data[data_consumed..]))
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ObjectSectionHeader {
    pub magic: u32,
    pub object_count: u32,
    pub image_count: u32,
}

impl ObjectSectionHeader {
    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 12 {
            return None;
        }

        let header = Self {
            magic: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            object_count: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            image_count: u32::from_le_bytes(data[8..12].try_into().unwrap()),
        };

        Some((header, &data[12..]))
    }
}

#[derive(Copy, Clone)]
pub struct ScreenObjectDefinition {
    pub x_reference_point: ReferencePoint,
    pub y_reference_point: ReferencePoint,
}

#[derive(Copy, Clone)]
pub enum DefinitionKind {
    Map,
    Screen(ScreenObjectDefinition),
}

pub struct ObjectDefinition {
    pub kind: DefinitionKind,

    pub id: u16,

    pub x: i16,
    pub y: i16,

    pub image_index: u8,
    pub layer: Layer,

    pub display_ticks: u16,
    pub display_mode: DisplayMode,
}

impl ObjectDefinition {
    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 10 {
            return None;
        }

        let object = u16::from_le_bytes(data[0..2].try_into().unwrap());
        let is_map_object = (object & 1) != 0;
        let id = object >> 1;

        let (x, y, kind) = if is_map_object {
            let x = i16::from_le_bytes(data[2..4].try_into().unwrap());
            let y = i16::from_le_bytes(data[4..6].try_into().unwrap());

            (x, y, DefinitionKind::Map)
        } else {
            let x_packed = u16::from_le_bytes(data[2..4].try_into().unwrap());
            let y_packed = u16::from_le_bytes(data[4..6].try_into().unwrap());

            let x_reference_point = ReferencePoint::from_value(x_packed & 0x0F);
            let x = x_packed.cast_signed() >> 4;
            let y_reference_point = ReferencePoint::from_value(y_packed & 0x0F);
            let y = y_packed.cast_signed() >> 4;

            (
                x,
                y,
                DefinitionKind::Screen(ScreenObjectDefinition {
                    x_reference_point,
                    y_reference_point,
                }),
            )
        };

        let image_index = data[6];
        let layer = Self::get_layer(data[7]);

        let display_packed = u16::from_le_bytes(data[8..10].try_into().unwrap());
        let display_ticks = display_packed & 0xFFF;
        let display_mode = DisplayMode::from_value(display_packed >> 12);

        let object = ObjectDefinition {
            kind,
            id,
            x,
            y,
            image_index,
            layer,
            display_ticks,
            display_mode,
        };

        Some((object, &data[10..]))
    }

    fn get_layer(value: u8) -> Layer {
        match value {
            0 => Layer::BelowAll,
            1 => Layer::AfterBackground,
            2 => Layer::AfterTiles,
            3 => Layer::AfterWeapons,
            4 => Layer::AfterShips,
            5 => Layer::AfterGauges,
            6 => Layer::AfterChat,
            7 => Layer::TopMost,
            _ => Layer::TopMost,
        }
    }
}

pub struct ImageDefinition {
    pub columns: u16,
    pub rows: u16,
    pub duration: u16,
    pub filename: String,
}

impl ImageDefinition {
    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 7 {
            return None;
        }

        let columns = u16::from_le_bytes(data[0..2].try_into().unwrap());
        let rows = u16::from_le_bytes(data[2..4].try_into().unwrap());
        let duration = u16::from_le_bytes(data[4..6].try_into().unwrap());

        let Ok(filename) = CStr::from_bytes_until_nul(&data[6..]) else {
            return None;
        };

        let Ok(filename) = filename.to_str() else {
            return None;
        };

        let data = &data[filename.len() + 6 + 1..];

        Some((
            Self {
                columns,
                rows,
                duration,
                filename: filename.to_string(),
            },
            data,
        ))
    }
}

pub struct LvzFileData {
    pub data: Vec<u8>,
    pub filename: String,
}

pub struct LvzContainer {
    pub objects: Vec<ObjectDefinition>,
    pub images: Vec<ImageDefinition>,
    pub files: Vec<LvzFileData>,
}

impl LvzContainer {
    pub fn new() -> Self {
        Self {
            objects: vec![],
            images: vec![],
            files: vec![],
        }
    }

    fn parse_section(&mut self, header: &LvzSectionHeader, data: &[u8]) {
        let mut data = data;

        if !header.filename.is_empty() || header.timestamp != 0 {
            self.files.push(LvzFileData {
                data: data.to_vec(),
                filename: header.filename.clone(),
            });
        } else {
            const CLV1_MAGIC: u32 = 0x31564C43;
            const CLV2_MAGIC: u32 = 0x32564C43;

            let Some((header, remaining_data)) = ObjectSectionHeader::parse(data) else {
                log::warn!("Invalid LvzSection definition");
                return;
            };

            data = remaining_data;

            if header.magic != CLV1_MAGIC && header.magic != CLV2_MAGIC {
                log::warn!("Invalid LvzSection definition");
                return;
            }

            for _ in 0..header.object_count {
                let Some((object, remaining_data)) = ObjectDefinition::parse(data) else {
                    log::warn!("Invalid LvzSection ObjectDefinition");
                    return;
                };

                self.objects.push(object);

                data = remaining_data;
            }

            for _ in 0..header.image_count {
                let Some((image_definition, remaining_data)) = ImageDefinition::parse(data) else {
                    log::warn!("Invalid LvzSection ImageDefinition");
                    return;
                };

                self.images.push(image_definition);

                data = remaining_data;
            }
        }
    }
}

pub struct LvzObject {
    pub kind: DefinitionKind,

    pub id: u16,

    pub x: i16,
    pub y: i16,

    pub layer: Layer,

    pub display_ticks: u32,
    pub display_mode: DisplayMode,

    pub sheet: SheetIndex,
    pub columns: u16,
    pub rows: u16,
    pub duration: u16,
    pub image_width: u32,
    pub image_height: u32,

    pub remaining_ticks: u32,
}

impl LvzObject {
    pub fn get_renderable(&self) -> SpriteRenderable {
        let total_frames = self.columns as usize * self.rows as usize;
        let elapsed_ticks = self.display_ticks.saturating_sub(self.remaining_ticks) as usize;

        let frame = get_animation_index(elapsed_ticks as u32, total_frames, self.duration as usize);

        let renderable_width = self.image_width / self.columns.max(1) as u32;
        let renderable_height = self.image_height / self.rows.max(1) as u32;

        let start_x = (frame as i32 % self.columns as i32) * renderable_width as i32;
        let start_y = (frame as i32 / self.columns as i32) * renderable_height as i32;
        let end_x = start_x + renderable_width as i32;
        let end_y = start_y + renderable_height as i32;

        let uv_start_x = (start_x as f32) / (self.image_width as f32);
        let uv_start_y = (start_y as f32) / (self.image_height as f32);
        let uv_end_x = (end_x as f32) / (self.image_width as f32);
        let uv_end_y = (end_y as f32) / (self.image_height as f32);

        SpriteRenderable {
            uv_start: [uv_start_x, uv_start_y],
            uv_size: [uv_end_x - uv_start_x, uv_end_y - uv_start_y],
            size: [renderable_width, renderable_height],
            sheet_index: self.sheet,
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum State {
    Downloading,
    Initialization(bool),
    Ready,
}

pub struct LvzController {
    pub containers: Vec<LvzContainer>,
    pub sheets: HashMap<String, SheetIndex>,
    pub objects: Vec<LvzObject>,

    pub active_map_objects: Vec<usize>,
    pub active_screen_objects: Vec<usize>,

    pub enter_zone_objects: Vec<usize>,
    pub enter_arena_objects: Vec<usize>,
    pub kill_objects: Vec<usize>,
    pub death_objects: Vec<usize>,
    pub server_objects: Vec<usize>,

    state: State,
}

impl LvzController {
    pub fn new() -> Self {
        Self {
            containers: vec![],
            sheets: HashMap::new(),
            objects: vec![],
            active_map_objects: vec![],
            active_screen_objects: vec![],

            enter_zone_objects: vec![],
            enter_arena_objects: vec![],
            kill_objects: vec![],
            death_objects: vec![],
            server_objects: vec![],

            state: State::Downloading,
        }
    }

    pub fn on_download_complete(&mut self, zone_activation: bool) {
        self.state = State::Initialization(zone_activation);
    }

    pub fn render(&mut self, render_state: &mut RenderState, sprites: &mut GameSprites) {
        if let State::Initialization(zone_activation) = self.state {
            self.initialize(render_state, sprites, zone_activation);
        }

        for index in &self.active_map_objects {
            let object = &self.objects[*index];
            let renderable = object.get_renderable();

            render_state.sprite_renderer.draw(
                &render_state.camera,
                &renderable,
                object.x as i32,
                object.y as i32,
                object.layer,
            );
        }

        for index in &self.active_screen_objects {
            let object = &self.objects[*index];
            let renderable = object.get_renderable();

            let (x_reference, y_reference) = match &object.kind {
                DefinitionKind::Map => continue,
                DefinitionKind::Screen(screen_object_definition) => (
                    screen_object_definition.x_reference_point,
                    screen_object_definition.y_reference_point,
                ),
            };

            let base_x = render_state.get_screen_reference_point(x_reference).0;
            let base_y = render_state.get_screen_reference_point(y_reference).1;

            render_state.sprite_renderer.draw(
                &render_state.ui_camera,
                &renderable,
                object.x as i32 + base_x,
                object.y as i32 + base_y,
                object.layer,
            );
        }
    }

    pub fn tick(&mut self) {
        Self::tick_set(&mut self.objects, &mut self.active_map_objects);
        Self::tick_set(&mut self.objects, &mut self.active_screen_objects);
    }

    fn tick_set(objects: &mut Vec<LvzObject>, set: &mut Vec<usize>) {
        let mut set_index = 0;

        while set_index < set.len() {
            let object_index = set[set_index];

            if objects[object_index].remaining_ticks > 0 {
                objects[object_index].remaining_ticks -= 1;

                if objects[object_index].remaining_ticks == 0 {
                    set.swap_remove(set_index);
                    continue;
                }
            }

            set_index += 1;
        }
    }

    pub fn activate_mode(&mut self, mode: DisplayMode) {
        let set = match &mode {
            DisplayMode::EnterZone => &self.enter_zone_objects,
            DisplayMode::EnterArena => &self.enter_arena_objects,
            DisplayMode::Kill => &self.kill_objects,
            DisplayMode::Death => &self.death_objects,
            DisplayMode::ServerControlled => &self.server_objects,

            DisplayMode::ShowAlways => {
                return;
            }
        };

        for index in set {
            Self::deactivate_index(
                &mut self.objects,
                &mut self.active_map_objects,
                &mut self.active_screen_objects,
                *index,
            );

            Self::activate_index(
                &mut self.objects,
                &mut self.active_map_objects,
                &mut self.active_screen_objects,
                *index,
            );
        }
    }

    pub fn activate(&mut self, id: u16) {
        self.deactivate(id);

        if let Some(index) = self.get_object_index_from_id(id) {
            Self::activate_index(
                &mut self.objects,
                &mut self.active_map_objects,
                &mut self.active_screen_objects,
                index,
            );
        }
    }

    fn activate_index(
        objects: &mut Vec<LvzObject>,
        active_map_objects: &mut Vec<usize>,
        active_screen_objects: &mut Vec<usize>,
        index: usize,
    ) {
        if index >= objects.len() {
            return;
        }

        let object = &mut objects[index];

        object.remaining_ticks = object.display_ticks;

        match &object.kind {
            DefinitionKind::Map => active_map_objects.push(index),
            DefinitionKind::Screen(_) => active_screen_objects.push(index),
        }
    }

    pub fn deactivate(&mut self, id: u16) {
        if let Some(index) = self.get_object_index_from_id(id) {
            Self::deactivate_index(
                &mut self.objects,
                &mut self.active_map_objects,
                &mut self.active_screen_objects,
                index,
            );
        }
    }

    fn deactivate_index(
        objects: &mut Vec<LvzObject>,
        active_map_objects: &mut Vec<usize>,
        active_screen_objects: &mut Vec<usize>,
        index: usize,
    ) {
        if index >= objects.len() {
            return;
        }

        let object = &objects[index];

        let set = match &object.kind {
            DefinitionKind::Map => active_map_objects,
            DefinitionKind::Screen(_) => active_screen_objects,
        };

        let mut set_index = 0;
        while set_index < set.len() {
            if set[set_index] == index {
                set.swap_remove(set_index);
                continue;
            }

            set_index += 1;
        }
    }

    pub fn get_object_index_from_id(&self, id: u16) -> Option<usize> {
        for i in 0..self.objects.len() {
            let object = &self.objects[i];

            if object.id == id {
                return Some(i);
            }
        }

        None
    }

    fn initialize(
        &mut self,
        render_state: &mut RenderState,
        sprites: &mut GameSprites,
        zone_activation: bool,
    ) {
        for container in &self.containers {
            for file in &container.files {
                let img = match image::load_from_memory(&file.data) {
                    Ok(img) => img.to_rgba8(),
                    Err(_) => {
                        log::warn!("Lvz file not image");
                        continue;
                    }
                };

                // TODO: Determine if this file should override GameSprite
                let _ = sprites;

                let texture = Texture::new_2d(
                    &render_state.device,
                    img.width(),
                    img.height(),
                    render_state.get_texture_format(),
                );

                RenderState::buffer_texture(&render_state.queue, &texture, &img.as_bytes());

                let sheet_index = render_state.sprite_renderer.create_sprite_sheet(
                    &render_state.device,
                    &texture,
                    false,
                );

                self.sheets.insert(file.filename.clone(), sheet_index);
            }
        }

        for container in &self.containers {
            for object_defn in &container.objects {
                let index = object_defn.image_index as usize;

                if index >= container.images.len() {
                    log::warn!("Invalid LvzContainer image index");
                    continue;
                }

                let image_defn = &container.images[index];

                let Some(sheet_index) = self.sheets.get(&image_defn.filename) else {
                    log::warn!(
                        "LvzObject definition requested file '{}', but it wasn't provided.",
                        image_defn.filename
                    );
                    continue;
                };

                let Some(sheet) = render_state.sprite_renderer.get_sheet(*sheet_index) else {
                    continue;
                };

                let object = LvzObject {
                    kind: object_defn.kind,
                    id: object_defn.id,
                    x: object_defn.x,
                    y: object_defn.y,
                    layer: object_defn.layer,
                    display_ticks: object_defn.display_ticks as u32 * 10,
                    display_mode: object_defn.display_mode,
                    sheet: *sheet_index,
                    columns: image_defn.columns,
                    rows: image_defn.rows,
                    duration: image_defn.duration,
                    remaining_ticks: 0,
                    image_width: sheet.width,
                    image_height: sheet.height,
                };

                let object_index = self.objects.len();
                self.objects.push(object);

                match &object_defn.display_mode {
                    DisplayMode::ShowAlways => match &object_defn.kind {
                        DefinitionKind::Map => self.active_map_objects.push(object_index),
                        DefinitionKind::Screen(_) => self.active_screen_objects.push(object_index),
                    },
                    DisplayMode::EnterZone => self.enter_zone_objects.push(object_index),
                    DisplayMode::EnterArena => self.enter_arena_objects.push(object_index),
                    DisplayMode::Kill => self.kill_objects.push(object_index),
                    DisplayMode::Death => self.death_objects.push(object_index),
                    DisplayMode::ServerControlled => self.server_objects.push(object_index),
                }
            }
        }

        self.state = State::Ready;

        if zone_activation {
            self.activate_mode(DisplayMode::EnterZone);
        }

        self.activate_mode(DisplayMode::EnterArena);
    }

    pub fn clear(&mut self, render_state: Option<&mut RenderState>) {
        let mut render_state = render_state;

        if let Some(render_state) = &mut render_state {
            for (_, sheet_index) in &self.sheets {
                render_state.sprite_renderer.destroy_sheet(*sheet_index);
            }
        }

        self.sheets.clear();
        self.objects.clear();
        self.containers.clear();

        self.active_map_objects.clear();
        self.active_screen_objects.clear();
        self.enter_zone_objects.clear();
        self.enter_arena_objects.clear();
        self.kill_objects.clear();
        self.death_objects.clear();
        self.server_objects.clear();

        self.state = State::Downloading;
    }

    pub fn handle_download(&mut self, data: &[u8]) -> Result<(), LvzError> {
        const CONT_MAGIC: u32 = 0x544E4F43;

        let Some((header, mut data)) = LvzHeader::parse(data) else {
            return Err(LvzError::Eof);
        };

        if header.magic != CONT_MAGIC {
            return Err(LvzError::InvalidHeader);
        }

        let mut container = LvzContainer::new();

        for _ in 0..header.section_count {
            let Some((section_header, remaining_data)) = LvzSectionHeader::parse(data) else {
                return Err(LvzError::Eof);
            };

            data = remaining_data;

            if section_header.magic != CONT_MAGIC {
                return Err(LvzError::InvalidSectionHeader);
            }

            if section_header.compressed_size != section_header.decompressed_size {
                let decompressed_data = match decompress_to_vec_zlib(
                    &data[..section_header.compressed_size as usize],
                ) {
                    Ok(decompressed_data) => decompressed_data,
                    Err(_) => return Err(LvzError::InvalidCompression),
                };

                container.parse_section(&section_header, &decompressed_data);
            } else {
                if data.len() < section_header.decompressed_size as usize {
                    return Err(LvzError::Eof);
                }

                container.parse_section(
                    &section_header,
                    &data[..section_header.decompressed_size as usize],
                );
            }

            data = &data[section_header.compressed_size as usize..];
        }

        self.containers.push(container);

        Ok(())
    }
}
