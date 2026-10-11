//! Renders the REAL editor (`vss::editor::ui`) over the default params in an eframe window,
//! saves docs/design/preview-vss-editor.png on the first stable frame and exits.
//!   cargo run --example editor_preview

use eframe::egui::{self, vec2, ViewportCommand};
use std::sync::Arc;
use vss::editor::{self, MapCtl, WINDOW};
use vss::params::VssParams;

struct App {
    ctl: MapCtl,
    frame: u32,
    asked: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let (w, h) = (WINDOW.0 as f32, WINDOW.1 as f32);
        if self.frame == 0 {
            // 1 px per point so the PNG is 1:1 whatever the OS scaling.
            let native = ctx.native_pixels_per_point().unwrap_or(1.0);
            ctx.set_pixels_per_point(1.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(w, h) / native));
        }
        self.frame += 1;
        editor::ui(ctx, &self.ctl, Some(5));

        let size = ctx.screen_rect().size();
        let ready = (size.x - w).abs() < 1.0 && (size.y - h).abs() < 1.0 && self.frame >= 3;
        if (ready || self.frame > 120) && !self.asked {
            self.asked = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/design/preview-vss-editor.png");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create png"));
            let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(img.as_raw()).unwrap();
            println!("wrote {} ({}x{})", path.display(), img.size[0], img.size[1]);
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW.0 as f32, WINDOW.1 as f32])
            .with_resizable(false)
            .with_title("VSS editor preview"),
        ..Default::default()
    };
    eframe::run_native(
        "VSS editor preview",
        opts,
        Box::new(|cc| {
            // Fonts apply on the next pass, so install them at creation, as the plugin does.
            rr_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(App { ctl: MapCtl::new(Arc::new(VssParams::default())), frame: 0, asked: false }))
        }),
    )
}
