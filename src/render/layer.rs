#[derive(Copy, Clone, Debug)]
pub enum Layer {
    BelowAll,
    Background,
    AfterBackground,
    Tiles,
    AfterTiles,
    Weapons,
    AfterWeapons,
    Ships,
    AfterShips,
    Explosions,
    Gauges,
    AfterGauges,
    Chat,
    AfterChat,
    TopMost,
}

impl Layer {
    pub fn z(&self) -> f32 {
        match self {
            Layer::BelowAll => 0.0f32,
            Layer::Background => 1.0f32,
            Layer::AfterBackground => 2.0f32,
            Layer::Tiles => 3.0f32,
            Layer::AfterTiles => 4.0f32,
            Layer::Weapons => 5.0f32,
            Layer::AfterWeapons => 6.0f32,
            Layer::Ships => 7.0f32,
            Layer::AfterShips => 8.0f32,
            Layer::Explosions => 9.0f32,
            Layer::Gauges => 10.0f32,
            Layer::AfterGauges => 11.0f32,
            Layer::Chat => 12.0f32,
            Layer::AfterChat => 13.0f32,
            Layer::TopMost => 14.0f32,
        }
    }

    pub fn get_max_z() -> f32 {
        15.0f32
    }
}
