//! GPU smoke test without audio, assets or an interactive game session.
//! cargo run -p prpr --no-default-features --example block_area_gpu -- OUTPUT_DIR
use macroquad::prelude::*;
use prpr::core::{block_area, MSRenderTarget};
use std::path::PathBuf;

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 900;

fn read_scene(res: &Resource) -> Vec<u8> {
    let texture = res.chart_target.as_ref().unwrap().output().texture;
    let mut rgb = vec![0_u8; (WIDTH * HEIGHT * 3) as usize];
    texture.raw_miniquad_texture_handle().read_pixels(&mut rgb);
    rgb
}

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
        window_width: WIDTH as i32,
        window_height: HEIGHT as i32,
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
        chart_target: Some(MSRenderTarget::new((WIDTH, HEIGHT), 1)),
        config: Config(false),
        aspect_ratio: 16. / 9.,
        time: 0.,
        blocked_touch_positions: Vec::new(),
    };
    let full = (0, 0, WIDTH as i32, HEIGHT as i32);
    let mut active_frame = Vec::new();
    for (name, time, flip, touch, viewport) in [
        ("hidden", 0., false, false, full),
        ("disabled", 1.25, false, false, full),
        ("ready", 1.75, false, false, full),
        ("active", 3., false, false, full),
        ("active_later", 3.4, false, false, full),
        ("active_seek_back", 3., false, false, full),
        ("touch", 3., false, true, full),
        ("flipped", 3., true, false, full),
        ("letterbox", 3., false, false, (200, 112, 1200, 675)),
        ("hidden_after", 5., false, false, full),
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
        let baseline = read_scene(&res);
        unsafe { get_internal_gl() }.quad_gl.viewport(Some(viewport));
        renderer.render(&areas, &mut res);
        // MSRenderTarget is RGB8; macroquad's readback helper assumes RGBA8.
        let rgb = read_scene(&res);
        if name == "active" {
            // This strip is beyond the field and its glow. Check normal scene
            // pixels inside the viewport too, not just letterboxing.
            for y in 0..HEIGHT {
                for x in WIDTH - 8..WIDTH {
                    let p = ((y * WIDTH + x) * 3) as usize;
                    assert_eq!(&rgb[p..p + 3], &baseline[p..p + 3], "scene changed outside noise field");
                }
            }
            active_frame = rgb.clone();
        } else if name == "active_later" {
            assert!(rgb.iter().zip(&active_frame).filter(|(a, b)| a != b).count() > 1000, "noise must animate");
        } else if name == "active_seek_back" {
            assert_eq!(rgb, active_frame, "chart time must control noise, including pause/seek");
        } else if name == "letterbox" {
            for y in 0..HEIGHT as i32 {
                for x in 0..WIDTH as i32 {
                    if x < viewport.0 || x >= viewport.0 + viewport.2 || y < viewport.1 || y >= viewport.1 + viewport.3 {
                        let p = ((y * WIDTH as i32 + x) * 3) as usize;
                        assert_eq!(&rgb[p..p + 3], &baseline[p..p + 3], "scene changed outside viewport at {x},{y}");
                    }
                }
            }
        }
        let image = Image {
            width: WIDTH as u16,
            height: HEIGHT as u16,
            bytes: rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        };
        image.export_png(output.join(format!("{name}.png")).to_str().unwrap());
        assert_eq!(unsafe { miniquad::gl::glGetError() }, 0, "GL error in {name}");
        next_frame().await;
    }
    std::fs::write(
        output.join("passed.txt"),
        "Shaders: GL_NO_ERROR. Animation, seek determinism, and unchanged pixels outside viewport passed at 1600x900.\n",
    )
    .unwrap();
}
