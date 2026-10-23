use std::{hint::black_box, time::Instant};
use xingjian::rendering::{RenderConfig, Renderer};

fn main() {
    // --layers measures the preview worker; default measures full-size export.
    let layers = std::env::args().any(|arg| arg == "--layers");
    let mut report = serde_json::Map::new();
    for (name, bg, width, height) in [
        ("letter", "builtin:bgs/letter.png", 560, 850),
        ("large", "solid:#fffdf5:2480:3508", 2000, 3000),
    ] {
        let mut renderer = Renderer::default();
        let mut config = RenderConfig {
            text: "把一颗星，藏进字里行间。\n愿每一句未曾说出口的话，都能在纸上找到温柔的回响。\n"
                .repeat(12),
            bg_path: bg.into(),
            rect_width: width,
            rect_height: height,
            ..Default::default()
        };
        black_box(renderer.render(&config).unwrap());
        for (scenario, changing_spacing) in [("move", false), ("spacing", true)] {
            let mut times = Vec::new();
            for i in 0..30 {
                if changing_spacing {
                    config.font_spacing = 0.1 + (i % 10) as f32 * 0.01;
                } else {
                    config.x = 60 + i;
                    config.y = 60 + i;
                }
                let started = Instant::now();
                if layers {
                    black_box(renderer.render_layers(&config).unwrap());
                } else {
                    black_box(renderer.render(&config).unwrap());
                }
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            report.insert(
                format!("{name}_{scenario}_median_ms"),
                serde_json::json!(times[times.len() / 2]),
            );
        }
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
