struct UniformState {
  mvp: mat4x4<f32>,
  camera_position: vec2<f32>,
  seed: u32,
  color: f32,
  speed: f32,
};

struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) world_position: vec2<f32>,
};

@group(0)
@binding(0)
var<uniform> uniform_state: UniformState;

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
  var out: VertexOutput;

  // z is 1.0 to match Layer::Background.
  out.position = uniform_state.mvp * vec4<f32>(position, 1.0, 1.0);

  let offset = vec2<f32>(uniform_state.camera_position.x, uniform_state.camera_position.y) * uniform_state.speed;
  out.world_position = position - offset;

  return out;
}

fn pcg_hash(input: u32) -> u32 {
    let state: u32 = input * 747796405u + 2891336453u;
    let word: u32 = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand(seed: u32, pos: vec2<f32>, repeat_offset: f32) -> f32 {
    let input: u32 = u32(pos.x) + u32(pos.y) * u32(repeat_offset);
    let r = pcg_hash(input ^ seed);

    return f32(r) / 4294967295.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let x: f32 = in.world_position.x;
  let y: f32 = in.world_position.y;

  // Shift everything to the right so the negative world positions aren't flipped.
  let offset = 1024.0 * 16.0 * 10.0;

  let x_pixels: f32 = floor(x * 16.0) + offset;
  let y_pixels: f32 = floor(y * 16.0) + offset;

  let rng = rand(uniform_state.seed, vec2<f32>(x_pixels, y_pixels), offset);

  if rng > 0.0001f {
    discard;
  }

  let color = uniform_state.color;

  return vec4<f32>(color, color, color, 1.0);
}
