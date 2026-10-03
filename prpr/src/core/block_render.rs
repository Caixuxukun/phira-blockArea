use super::{
    block_area::{BlockArea, BlockPhase, BlockPoint},
    Resource,
};
use anyhow::Result;
use macroquad::prelude::*;
use miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams, UniformType};

const VERTEX: &str = include_str!("shaders/block_area/vertex.glsl");
const ACTIVE: &str = include_str!("shaders/block_area/active.glsl");
const DISABLED: &str = include_str!("shaders/block_area/disabled.glsl");

/// Owns all GPU objects; charts without noise fields allocate none of them.
pub struct BlockRenderer {
    union: Material,
    xor: Material,
    compose: Material,
    edge: Material,
    active: Material,
    disabled: Material,
    targets: Vec<RenderTarget>,
    textures: [Texture2D; 3],
    size: (u32, u32),
}

fn material(fragment: &str, blend: Option<BlendState>) -> Result<Material> {
    // The adapted APK programs contain only simple, scalar uniform declarations.
    let mut uniforms = Vec::new();
    let mut textures = Vec::new();
    for line in fragment.lines() {
        let words: Vec<_> = line.trim().trim_end_matches(';').split_whitespace().collect();
        if words.first() != Some(&"uniform") {
            continue;
        }
        let kind = words[words.len() - 2];
        let name = words[words.len() - 1].to_owned();
        if kind == "sampler2D" {
            textures.push(name);
            continue;
        }
        let kind = match kind {
            "float" => UniformType::Float1,
            "int" => UniformType::Int1,
            "vec2" => UniformType::Float2,
            "vec3" => UniformType::Float3,
            "vec4" => UniformType::Float4,
            _ => anyhow::bail!("Unsupported block shader uniform {kind}"),
        };
        uniforms.push((name, kind));
    }
    Ok(load_material(
        VERTEX,
        fragment,
        MaterialParams {
            uniforms,
            textures,
            pipeline_params: PipelineParams {
                color_blend: blend,
                ..Default::default()
            },
        },
    )?)
}

impl BlockRenderer {
    pub fn new() -> Result<Self> {
        use BlendFactor::{One, OneMinusValue};
        use BlendValue::{DestinationColor, SourceAlpha, SourceColor};
        // Allocate fallible materials into an owning object first, so an error
        // on a later shader cannot leak the earlier GPU pipelines.
        let mut materials = Vec::new();
        let result = (|| -> Result<()> {
            for (fragment, blend) in [
                (include_str!("shaders/block_area/mask.glsl"), Some(BlendState::new(Equation::Add, One, OneMinusValue(SourceColor)))),
                (
                    include_str!("shaders/block_area/mask.glsl"),
                    Some(BlendState::new(Equation::Add, OneMinusValue(DestinationColor), OneMinusValue(SourceColor))),
                ),
                (include_str!("shaders/block_area/compose.glsl"), None),
                (include_str!("shaders/block_area/edge_glow.glsl"), None),
                (ACTIVE, Some(BlendState::new(Equation::Add, One, OneMinusValue(SourceAlpha)))),
                (DISABLED, Some(BlendState::new(Equation::Add, One, One))),
            ] {
                materials.push(material(fragment, blend)?);
            }
            Ok(())
        })();
        if let Err(err) = result {
            for mut material in materials {
                material.delete();
            }
            return Err(err);
        }
        let textures = [
            Texture2D::from_file_with_format(include_bytes!("shaders/block_area/displace.png"), Some(ImageFormat::Png)),
            Texture2D::from_file_with_format(include_bytes!("shaders/block_area/spark.png"), Some(ImageFormat::Png)),
            Texture2D::from_file_with_format(include_bytes!("shaders/block_area/noise.png"), Some(ImageFormat::Png)),
        ];
        for texture in textures {
            texture.set_filter(FilterMode::Linear);
            texture
                .raw_miniquad_texture_handle()
                .set_wrap(unsafe { get_internal_gl() }.quad_context, miniquad::TextureWrap::Repeat);
        }
        Ok(Self {
            union: materials[0],
            xor: materials[1],
            compose: materials[2],
            edge: materials[3],
            active: materials[4],
            disabled: materials[5],
            targets: Vec::new(),
            textures,
            size: (0, 0),
        })
    }

    fn target(&self, index: usize) {
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        gl.quad_gl.render_pass(Some(self.targets[index].render_pass));
        gl.quad_gl.viewport(None);
        clear_background(Color::new(0., 0., 0., 0.));
    }

    fn fullscreen(material: Material) {
        gl_use_material(material);
        draw_rectangle(-1., -1., 2., 2., WHITE);
        gl_use_default_material();
    }

    pub fn render(&mut self, areas: &[BlockArea], res: &mut Resource) {
        let Some(target) = res.chart_target.as_ref() else {
            return;
        };
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let viewport = gl.quad_gl.get_viewport();
        let output = target.output();
        let vp = viewport.unwrap_or((0, 0, output.texture.width() as i32, output.texture.height() as i32));
        let scale = (1280. / vp.2.max(vp.3).max(1) as f32).min(1.);
        let size = ((vp.2 as f32 * scale).max(1.) as u32, (vp.3 as f32 * scale).max(1.) as u32);
        // All masks use chart-local UVs, including when the game is letterboxed.
        if self.size != size {
            for target in self.targets.drain(..) {
                target.delete();
            }
            self.targets = (0..11).map(|_| render_target(size.0, size.1)).collect();
            for target in &self.targets {
                target.texture.set_filter(FilterMode::Linear);
            }
            self.size = size;
        }
        let time = res.time;
        let flip = if res.config.flip_x() { -1. } else { 1. };
        let visible: Vec<_> = areas
            .iter()
            .filter_map(|area| {
                let phase = area.phase(time);
                (phase != BlockPhase::Hidden).then(|| (area, phase, area.geometry(time, res.aspect_ratio)))
            })
            .collect();
        if visible.is_empty() && res.blocked_touch_positions.is_empty() {
            return;
        }
        // 0/1 active normal/subtract, 2/3 disabled, 4/5 ready.
        for index in 0..6 {
            self.target(index);
            gl_use_material(if index % 2 == 0 { self.union } else { self.xor });
            for (area, phase, geometry) in &visible {
                let stage = match phase {
                    BlockPhase::Active => 0,
                    BlockPhase::Disabled => 2,
                    BlockPhase::Ready => 4,
                    BlockPhase::Hidden => continue,
                };
                if stage + usize::from(area.is_subtract) != index {
                    continue;
                }
                let point = |x, y| {
                    let p = geometry.corner(x, y);
                    vec2(p.x * flip, p.y * res.aspect_ratio)
                };
                let a = area.opacity(time);
                let color = Color::new(a, a, a, a);
                let [p0, p1, p2, p3] = [point(-0.5, -0.5), point(0.5, -0.5), point(0.5, 0.5), point(-0.5, 0.5)];
                draw_triangle(p0, p1, p2, color);
                draw_triangle(p0, p2, p3, color);
            }
            gl_use_default_material();
        }
        for index in 0..3 {
            self.target(6 + index);
            self.compose.set_texture("normalMask", self.targets[index * 2].texture);
            self.compose.set_texture("subtractMask", self.targets[index * 2 + 1].texture);
            self.compose.set_texture("displaceMap", self.textures[0]);
            self.compose.set_uniform("time", time as f32);
            self.compose.set_uniform("isActive", if index == 0 { 1_f32 } else { 0_f32 });
            Self::fullscreen(self.compose);
        }
        self.target(9);
        self.edge.set_texture("mask", self.targets[6].texture);
        self.edge.set_uniform("texel", vec2(1. / size.0 as f32, 1. / size.1 as f32));
        Self::fullscreen(self.edge);
        self.target(10);
        gl_use_material(self.union);
        for BlockPoint { x, y } in res.blocked_touch_positions.iter().take(10) {
            let radius = 0.12;
            let center = vec2(*x * flip, *y * res.aspect_ratio);
            for i in 0..32 {
                let point = |i: i32| {
                    let (s, c) = (i as f32 * std::f32::consts::TAU / 32.).sin_cos();
                    center + vec2(c * radius, s * radius * res.aspect_ratio)
                };
                draw_triangle(center, point(i), point(i + 1), WHITE);
            }
        }
        gl_use_default_material();
        gl.flush();

        // Preserve the scene sampled by ActiveBlock: never sample the FBO being
        // written. Copy the full target so areas outside the chart viewport survive.
        let target = res.chart_target.as_mut().unwrap();
        target.swap();
        let source = target.old();
        let destination = target.output();
        super::copy_fbo(super::internal_id(source), super::internal_id(destination), (source.texture.width() as u32, source.texture.height() as u32));
        gl.quad_gl.render_pass(Some(destination.render_pass));
        gl.quad_gl.viewport(viewport);
        let time = time as f32;
        for material in [self.active, self.disabled] {
            material.set_uniform("_Time", [time / 20., time, time * 2., time * 3.]);
            material.set_texture("_DisplaceMap", self.textures[0]);
            material.set_texture("_SparkMap", self.textures[1]);
        }
        self.disabled.set_texture("_ComposeRT", self.targets[7].texture);
        Self::fullscreen(self.disabled);
        self.active.set_texture("_ComposeRT", self.targets[6].texture);
        self.active.set_texture("_EffectRT", self.targets[9].texture);
        self.active.set_texture("_DisabledNormalBlockRT", self.targets[4].texture);
        self.active.set_texture("_DisabledSubtractBlockRT", self.targets[5].texture);
        self.active.set_texture("_ReadyComposeRT", self.targets[8].texture);
        self.active.set_texture("_TouchHoverRT", self.targets[10].texture);
        self.active.set_texture("_SceneColor", source.texture);
        self.active.set_texture("_NoiseMap", self.textures[2]);
        let (w, h) = (vp.2 as f32, vp.3 as f32);
        self.active.set_uniform("_ScreenParams", [w, h, 1. + 1. / w, 1. + 1. / h]);
        self.active
            .set_uniform("_EffectRT_TexelSize", [1. / size.0 as f32, 1. / size.1 as f32, size.0 as f32, size.1 as f32]);
        self.active
            .set_uniform("sceneScale", vec2(w / source.texture.width(), h / source.texture.height()));
        self.active
            .set_uniform("sceneOffset", vec2(vp.0 as f32 / source.texture.width(), vp.1 as f32 / source.texture.height()));
        self.active
            .set_uniform("_TouchPosCount", res.blocked_touch_positions.len().min(10) as i32);
        self.active.set_uniform("_TouchPosShine", (0.5 + 0.5 * (time * 43.).sin()).max(0.63) * 2.);
        for (index, point) in res.blocked_touch_positions.iter().take(10).enumerate() {
            self.active
                .set_uniform(&format!("touch{index}"), vec2((point.x * flip + 1.) * 0.5 * res.aspect_ratio, (point.y * res.aspect_ratio + 1.) * 0.5));
        }
        Self::fullscreen(self.active);
        gl.flush();
    }
}

impl Drop for BlockRenderer {
    fn drop(&mut self) {
        for material in [
            &mut self.union,
            &mut self.xor,
            &mut self.compose,
            &mut self.edge,
            &mut self.active,
            &mut self.disabled,
        ] {
            material.delete();
        }
        for target in &self.targets {
            target.delete();
        }
        for texture in self.textures {
            texture.delete();
        }
    }
}
