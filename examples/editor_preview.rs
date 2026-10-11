//! Renders the REAL editor (`vss::editor::ui`) over the default params in an eframe window,
//! saves docs/design/preview-vss-editor.png on the first stable frame and exits.
//!   cargo run --example editor_preview

use eframe::egui::{self, vec2, ViewportCommand};
use std::sync::Arc;
use rr_bus::{Bus, Kind};
use vss::bus::{Link, Modulator};
use vss::editor::{self, Ctl, MapCtl, PatchView, WINDOW};
use vss::params::{CvTarget, VssParams};

struct App {
    ctl: MapCtl,
    link: Arc<Link>,
    view: PatchView,
    _keep: Box<dyn std::any::Any>,
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
        editor::ui(ctx, &self.ctl, &self.link, &mut self.view, Some(5));

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
            let (ctl, link, keep) = demo();
            Ok(Box::new(App { ctl, link, view: PatchView::default(), _keep: keep, frame: 0, asked: false }))
        }),
    )
}

/// The same patch-bay demo data as the rr-ui kit preview: row 1 follows this device's own Step
/// Gate, row 2 a live source from another device, row 3 a lost one.
fn demo() -> (MapCtl, Arc<Link>, Box<dyn std::any::Any>) {
    let params = Arc::new(VssParams::default());
    let ctl = MapCtl::new(params.clone());
    let link = Link::new(params.clone());
    let bus = Bus::open_named(&format!("Local\\VssEditorPreview.{}", std::process::id()));
    // Two other devices publishing, so the source combo has entries.
    let gts1 = bus.register("GTS");
    let mod_a = gts1.publish("Mod A", Kind::Bipolar);
    let gts2 = bus.register("GTS");
    let mod_b = gts2.publish("Mod B", Kind::Bipolar);
    mod_a.set(0.35);
    mod_b.set(-0.5);

    let mut modulator = Modulator::new(link.clone());
    modulator.initialize(bus.clone());
    link.set_label("Lead bass");
    *params.cv_sources.write().unwrap() =
        ["Lead bass \u{b7} Step Gate".into(), "GTS 1 \u{b7} Mod A".into(), "GTS 3 \u{b7} Mod A".into()];
    link.bind_persisted();
    for (i, (t, a)) in [(CvTarget::FormantShift, 0.78), (CvTarget::Resonance, 0.35), (CvTarget::Drive, 0.5)].into_iter().enumerate() {
        ctl.set_enum(vss::editor::CV_TARGET_IDS[i], t as usize);
        ctl.set01(vss::editor::CV_AMOUNT_IDS[i], a);
    }
    // One block so the activity LEDs have something to show.
    modulator.publish((1.0, 0.25, 0.4));
    modulator.begin_block([CvTarget::FormantShift, CvTarget::Resonance, CvTarget::Drive], [1.0; 3], 48_000.0);
    ctl.log.borrow_mut().clear();
    (ctl, link, Box::new((bus, gts1, gts2, mod_a, mod_b, modulator)))
}
