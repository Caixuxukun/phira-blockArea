//! GPU smoke test without audio, assets or an interactive game session.
//! cargo run -p prpr --no-default-features --example block_area_gpu -- OUTPUT_DIR
use macroquad::prelude::*;
use prpr::core::{block_area, copy_fbo, internal_id, MSRenderTarget};
use std::path::PathBuf;

#[path = "../src/core/block_render.rs"]
mod block_render;

struct Config(bool);
impl Config {
    fn flip_x(&self) -> bool {
        self.0
    }
}

// Exercise the production renderer against a minimal resource holder. The
// production Resource and all its call sites are also checked by cargo check.
struct Resource {
    chart_target: Option<MSRenderTarget>,
    config: Config,
    aspect_ratio: f32,
    time: f64,
    blocked_touch_positions: Vec<block_area::BlockPoint>,
}

fn config() -> Conf {
    Conf {
        window_title: "blockArea GPU validation".into(),
        window_width: 640,
        window_height: 360,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    let output = std::env::args().nth(1).map(PathBuf::from).expect("output directory");
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = block_render::BlockRenderer::new().unwrap();
    let mut areas: Vec<block_area::BlockArea> = serde_json::from_str(
        r#"[
      {"topRightPercentage":{"x":0.8,"y":0.8},"bottomLeftPercentage":{"x":0.1,"y":0.2},
       "appearTime":1,"enableTime":2,"disableTime":4,"disappearTime":5,
       "rotateEvents":[{"time":0,"rotation":12,"anchor":{"x":0.5,"y":0.5},"easeType":0}]},
      {"topRightPercentage":{"x":0.58,"y":0.65},"bottomLeftPercentage":{"x":0.36,"y":0.35},
       "appearTime":1,"enableTime":2,"disableTime":4,"disappearTime":5,"isSubtract":true}
    ]"#,
    )
    .unwrap();
    for a in &mut areas {
        a.validate().unwrap();
    }
    let mut res = Resource {
        chart_target: Some(MSRenderTarget::new((640, 360), 1)),
        config: Config(false),
        aspect_ratio: 16. / 9.,
        time: 0.,
        blocked_touch_positions: Vec::new(),
    };
    for (name, time, flip, touch, viewport) in [
        ("hidden", 0., false, false, (0, 0, 640, 360)),
        ("disabled", 1.25, false, false, (0, 0, 640, 360)),
        ("ready", 1.75, false, false, (0, 0, 640, 360)),
        ("active", 3., false, false, (0, 0, 640, 360)),
        ("touch", 3., false, true, (0, 0, 640, 360)),
        ("flipped", 3., true, false, (0, 0, 640, 360)),
        ("letterbox", 3., false, false, (80, 45, 480, 270)),
        ("hidden_after", 5., false, false, (0, 0, 640, 360)),
    ] {
        res.time = time;
        res.config.0 = flip;
        res.blocked_touch_positions = if touch {
            vec![block_area::BlockPoint { x: -0.5, y: 0. }]
        } else {
            Vec::new()
        };
        set_camera(&Camera2D {
            zoom: vec2(1., 1.),
            render_target: Some(res.chart_target.as_ref().unwrap().output()),
            ..Default::default()
        });
        clear_background(Color::new(0.08, 0.12, 0.18, 1.));
        for i in 0..20 {
            let x = -1. + i as f32 * 0.1;
            draw_line(x, -1., x, 1., 0.005, GRAY);
            draw_line(-1., x, 1., x, 0.005, GRAY);
        }
        unsafe { get_internal_gl() }.flush();
        unsafe { get_internal_gl() }.quad_gl.viewport(Some(viewport));
        renderer.render(&areas, &mut res);
        // MSRenderTarget is RGB8; macroquad's readback helper assumes RGBA8.
        let texture = res.chart_target.as_ref().unwrap().output().texture;
        let mut rgb = vec![0_u8; 640 * 360 * 3];
        texture.raw_miniquad_texture_handle().read_pixels(&mut rgb);
        let image = Image {
            width: 640,
            height: 360,
            bytes: rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        };
        image.export_png(output.join(format!("{name}.png")).to_str().unwrap());
        assert_eq!(unsafe { miniquad::gl::glGetError() }, 0, "GL error in {name}");
        next_frame().await;
    }
    std::fs::write(output.join("passed.txt"), "All blockArea shaders compiled, rendered and returned GL_NO_ERROR.\n").unwrap();
}
