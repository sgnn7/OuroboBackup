mod app;

use eframe::egui;

fn load_icon() -> Option<egui::IconData> {
    let png_bytes = include_bytes!("../../../assets/icon_source.png");
    let src = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (src_w, src_h) = src.dimensions();

    // macOS Tahoe icons: content fills ~80% of canvas, centered, with a
    // soft continuous rounded rectangle (squircle) mask.
    let canvas_size = src_w; // output is same size as source
    let content_scale = 0.80;
    let content_size = (canvas_size as f64 * content_scale) as u32;
    let offset = (canvas_size - content_size) / 2;

    // Resize source to content area
    let resized = image::imageops::resize(
        &src,
        content_size,
        content_size,
        image::imageops::FilterType::Lanczos3,
    );

    // Create transparent canvas and paste resized content centered
    let mut canvas = image::RgbaImage::new(canvas_size, canvas_size);
    image::imageops::overlay(&mut canvas, &resized, offset as i64, offset as i64);

    // Apply squircle mask over the content area
    // Radius ~19% of content size, matching Apple's icon corner radius
    let radius = content_size as f64 * 0.42;
    let cx = canvas_size as f64 / 2.0;
    let cy = canvas_size as f64 / 2.0;
    let half = content_size as f64 / 2.0;

    for y in 0..canvas_size {
        for x in 0..canvas_size {
            let alpha = squircle_alpha(
                x as f64, y as f64, cx, cy, half, radius,
            );
            if alpha < 1.0 {
                let pixel = canvas.get_pixel_mut(x, y);
                pixel.0[3] = (pixel.0[3] as f64 * alpha) as u8;
            }
        }
    }

    let (width, height) = canvas.dimensions();
    Some(egui::IconData {
        rgba: canvas.into_raw(),
        width,
        height,
    })
}

/// Returns 0.0-1.0 alpha for a macOS-style continuous rounded rect (squircle).
fn squircle_alpha(x: f64, y: f64, cx: f64, cy: f64, half: f64, radius: f64) -> f64 {
    // Distance from edge of content rect
    let dx = (x - cx).abs() - (half - radius);
    let dy = (y - cy).abs() - (half - radius);

    if dx <= 0.0 || dy <= 0.0 {
        return 1.0;
    }

    // Superellipse (squircle): (dx/r)^n + (dy/r)^n <= 1, n=4 for Apple's shape
    let nx = dx / radius;
    let ny = dy / radius;
    let dist = (nx.powi(4) + ny.powi(4)).powf(0.25);

    if dist <= 1.0 {
        1.0
    } else {
        // Smooth anti-alias over ~2px
        (1.0 - (dist - 1.0) * radius / 2.0).clamp(0.0, 1.0)
    }
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([600.0, 400.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "OuroboBackup",
        options,
        Box::new(|_cc| Ok(Box::new(app::OuroboApp::new()))),
    )
}
