struct UniformState {
  mvp: mat4x4<f32>,
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

  // z is 8.9 to match Layer::AfterShips, but at the top of it.
  out.position = uniform_state.mvp * vec4<f32>(position, 8.9, 1.0);
  out.world_position = position;

  return out;
}

@group(0)
@binding(1)
var t_diffuse: texture_2d_array<f32>;

@group(0)
@binding(2)
var s_diffuse: sampler;

@group(0)
@binding(3)
var t_tiledata: texture_2d<u32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let x: f32 = in.world_position.x;
  let y: f32 = in.world_position.y;

  var tile_id: u32 = 0;

  if x < 0.0 || y < 0.0 || x > 1024.0 || y > 1024.0 {
    discard;
  } else {
    let tile_x = u32(in.world_position.x);
    let tile_y = u32(in.world_position.y);

    tile_id = textureLoad(t_tiledata, vec2<u32>(tile_x, tile_y), 0).r;
  }

  if tile_id < 176 || tile_id > 190 {
    discard;
  }

  let uv: vec2<f32> = modf(in.world_position + vec2<f32>(2.0, 2.0)).fract;
  let sample: vec4<f32> = textureSample(t_diffuse, s_diffuse, uv, tile_id - 1);

  if sample.r == 0.0 && sample.g == 0.0 && sample.b == 0.0 {
    discard;
  }
  
  return sample;
}
