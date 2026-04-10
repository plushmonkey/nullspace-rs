pub struct Camera {
    pub projection: glam::Mat4,
    pub position: glam::Vec2,
    pub surface_dim: glam::Vec2,
    pub scale: f32,
}

impl Camera {
    pub fn new(surface_width: f32, surface_height: f32, position: glam::Vec2, scale: f32) -> Self {
        let projection = Self::build_projection(surface_width, surface_height, scale);

        Camera {
            projection,
            position,
            surface_dim: glam::Vec2::new(surface_width, surface_height),
            scale,
        }
    }

    pub fn projection(&self) -> &glam::Mat4 {
        &self.projection
    }

    pub fn view(&self) -> glam::Mat4 {
        glam::Mat4::from_translation(glam::Vec3::new(-self.position.x, -self.position.y, 0.0))
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn set_surface_dimensions(&mut self, surface_width: f32, surface_height: f32) {
        self.surface_dim = glam::Vec2::new(surface_width, surface_height);
        self.projection = Self::build_projection(surface_width, surface_height, self.scale);
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.projection =
            Self::build_projection(self.surface_dim.x, self.surface_dim.y, self.scale);
    }

    pub fn unproject(&self, screen_position: glam::Vec2) -> glam::Vec2 {
        let screen_center = self.surface_dim * 0.5;
        let screen_offset = screen_position - screen_center;

        self.position + (screen_offset * self.scale)
    }

    fn build_projection(surface_width: f32, surface_height: f32, scale: f32) -> glam::Mat4 {
        let width = ((surface_width as u32 + 1) & !1) as f32;
        let height = ((surface_height as u32 + 1) & !1) as f32;

        // Scale is halved because the orthographic setup is halved.
        let scale = scale * 0.5;

        let left = -width * scale;
        let right = width * scale;
        let bottom = height * scale;
        let top = -height * scale;

        glam::Mat4::orthographic_rh(left, right, bottom, top, 0.0f32, 1.0f32)
    }
}
