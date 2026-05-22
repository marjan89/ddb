use clap::Args;
use image::{ImageReader, RgbaImage};
use std::io::Cursor;
use std::path::Path;

use crate::adb;
use crate::registry::{Device, Registry};

#[derive(Args)]
pub struct ScrollCaptureArgs {
    /// Output composite PNG path
    #[arg(short, long, default_value = "/tmp/android-scroll-composite.png")]
    pub output: String,

    /// Scroll percentage per step (smaller = more overlap = better correlation)
    #[arg(long, default_value_t = 20)]
    pub scroll_pct: u32,

    /// Maximum scroll steps before giving up
    #[arg(long, default_value_t = 30)]
    pub max_steps: u32,

    /// Save individual frames
    #[arg(long)]
    pub keep_steps: bool,
}

pub fn run(dev_name: Option<&str>, args: ScrollCaptureArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    let frames = capture_frames(dev.as_ref(), args.scroll_pct, args.max_steps)?;

    if frames.len() < 2 {
        eprintln!("only {} frame(s), need at least 2", frames.len());
        if let Some(img) = frames.first() {
            img.save(&args.output).map_err(|e| format!("save failed: {e}"))?;
            println!("{} ({}x{})", args.output, img.width(), img.height());
        }
        return Ok(());
    }

    eprintln!("{} frames captured", frames.len());

    if args.keep_steps {
        let dir = Path::new(&args.output).parent().unwrap_or(Path::new("/tmp"));
        for (i, img) in frames.iter().enumerate() {
            let path = dir.join(format!("scroll-step-{i}.png"));
            let _ = img.save(&path);
        }
    }

    let screen_h = frames[0].height() as usize;
    let w = frames[0].width();

    let sticky_top = detect_sticky_top_simple(&frames[0], &frames[1]);
    let scroll_delta_01 = find_scroll_delta(&frames[0], &frames[1], 0, screen_h);
    let sticky_bottom = detect_vanishing_bottom(&frames[0], &frames[1], scroll_delta_01);
    eprintln!("sticky_top={sticky_top}px sticky_bottom={sticky_bottom}px screen={screen_h}px");

    let content_top = sticky_top;
    let content_bottom = screen_h - sticky_bottom;
    let content_h = content_bottom - content_top;
    let margin = content_h / 5; // 20% crop from each edge

    // Exclude the final at-rest frame from stitching
    let stitch_count = frames.len() - 1;

    // Build strips: frame 0 full content, middle frames cropped 20% each side, last frame crop top only
    let mut strips: Vec<RgbaImage> = Vec::new();

    if sticky_top > 0 {
        strips.push(crop_strip(&frames[0], 0, sticky_top));
    }

    strips.push(crop_strip(&frames[0], content_top, content_bottom));

    for i in 1..stitch_count {
        let is_last_stitch = i == stitch_count - 1;
        let strip_top = content_top + margin;
        let strip_bottom = if is_last_stitch { content_bottom } else { content_bottom - margin };
        if strip_top >= strip_bottom { continue; }

        let strip = crop_strip(&frames[i], strip_top, strip_bottom);
        strips.push(strip);
    }

    if sticky_bottom > 0 {
        strips.push(crop_strip(&frames[0], content_bottom, screen_h));
    }

    // Place strips: first strip at y=0, each subsequent via overlap correlation
    let mut composite_h = strips[0].height();
    let mut placements: Vec<u32> = vec![0];

    for i in 1..strips.len() {
        let placement = find_strip_placement(&strips, &placements, i, w);
        let bottom = placement + strips[i].height();
        if bottom > composite_h {
            composite_h = bottom;
        }
        placements.push(placement);
    }

    let mut composite = RgbaImage::new(w, composite_h);
    for (i, strip) in strips.iter().enumerate() {
        image::imageops::overlay(&mut composite, strip, 0, placements[i] as i64);
    }

    composite.save(&args.output).map_err(|e| format!("save failed: {e}"))?;
    println!("{} ({}x{})", args.output, w, composite_h);
    Ok(())
}

fn capture_frames(dev: Option<&Device>, scroll_pct: u32, max_steps: u32) -> Result<Vec<RgbaImage>, String> {
    let first = take_screenshot(dev)?;
    let h = first.height() as i32;
    let w = first.width() as i32;
    let mid_x = w / 2;

    // Scroll to top
    eprintln!("scrolling to top...");
    for _ in 0..8 {
        adb::shell(dev, &[
            "input", "swipe",
            &mid_x.to_string(), &(h / 5).to_string(),
            &mid_x.to_string(), &(h * 4 / 5).to_string(),
            "100",
        ])?;
    }
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Scroll distance per step
    let scroll_dist = (h as f64 * scroll_pct as f64 / 100.0) as i32;
    let start_y = h / 2 + scroll_dist / 2;
    let end_y = h / 2 - scroll_dist / 2;

    let mut frames = Vec::new();
    let mut identical_count = 0u32;

    // Capture first frame at top
    let frame0 = take_screenshot(dev)?;
    frames.push(frame0);

    for step in 0..max_steps {
        adb::shell(dev, &[
            "input", "swipe",
            &mid_x.to_string(), &start_y.to_string(),
            &mid_x.to_string(), &end_y.to_string(),
            "300",
        ])?;

        std::thread::sleep(std::time::Duration::from_millis(400));

        let frame = take_screenshot(dev)?;

        if is_near_identical(&frames[frames.len() - 1], &frame) {
            identical_count += 1;
            if identical_count >= 2 {
                eprintln!("bottom reached at step {step}");
                break;
            }
        } else {
            identical_count = 0;
            frames.push(frame);
        }
    }

    // Wait for animations to settle (nav bar reappear), capture final at-rest frame
    std::thread::sleep(std::time::Duration::from_millis(800));
    let at_rest = take_screenshot(dev)?;
    frames.push(at_rest);

    // Scroll back to top
    eprintln!("scrolling back to top...");
    for _ in 0..8 {
        adb::shell(dev, &[
            "input", "swipe",
            &mid_x.to_string(), &(h / 5).to_string(),
            &mid_x.to_string(), &(h * 4 / 5).to_string(),
            "100",
        ])?;
    }

    Ok(frames)
}

fn take_screenshot(dev: Option<&Device>) -> Result<RgbaImage, String> {
    let png_bytes = adb::adb_raw(dev, &["exec-out", "screencap", "-p"])?;
    let img = ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("image format error: {e}"))?
        .decode()
        .map_err(|e| format!("image decode error: {e}"))?
        .to_rgba8();
    Ok(img)
}

fn find_scroll_delta(prev: &RgbaImage, curr: &RgbaImage, content_top: usize, content_bottom: usize) -> usize {
    let w = prev.width() as usize;
    let content_h = content_bottom - content_top;
    let max_delta = content_h / 2;

    let sample_rows = 20;
    let mut best_delta = 0usize;
    let mut best_score = u64::MAX;

    for delta in 0..max_delta {
        let overlap_h = content_h - delta;
        if overlap_h < 20 { break; }
        let row_step = overlap_h / sample_rows;
        if row_step == 0 { continue; }

        let mut diff: u64 = 0;
        for r in 0..sample_rows {
            let prev_y = content_top + delta + r * row_step;
            let curr_y = content_top + r * row_step;
            for x in (0..w).step_by(4) {
                let pa = prev.get_pixel(x as u32, prev_y as u32);
                let pb = curr.get_pixel(x as u32, curr_y as u32);
                diff += (pa[0] as i32 - pb[0] as i32).unsigned_abs() as u64;
                diff += (pa[1] as i32 - pb[1] as i32).unsigned_abs() as u64;
                diff += (pa[2] as i32 - pb[2] as i32).unsigned_abs() as u64;
            }
            if diff > best_score { break; }
        }

        if diff < best_score {
            best_score = diff;
            best_delta = delta;
        }
    }

    best_delta
}

fn find_strip_placement(strips: &[RgbaImage], placements: &[u32], idx: usize, w: u32) -> u32 {
    let strip = &strips[idx];
    let prev_idx = idx - 1;
    let prev = &strips[prev_idx];
    let prev_y0 = placements[prev_idx];
    let prev_bottom = prev_y0 + prev.height();

    let strip_h = strip.height() as usize;
    let prev_h = prev.height() as usize;
    let max_overlap = strip_h.min(prev_h);
    let sample_rows = 10;

    let mut best_overlap = 0usize;
    let mut best_score = u64::MAX;

    for overlap in 1..max_overlap {
        let row_step = overlap / sample_rows;
        if row_step == 0 { continue; }

        let mut diff: u64 = 0;
        for r in 0..sample_rows {
            let strip_y = r * row_step;
            let prev_y = prev_h - overlap + r * row_step;
            if strip_y >= strip_h || prev_y >= prev_h { break; }
            for x in (0..w as usize).step_by(4) {
                let pa = prev.get_pixel(x as u32, prev_y as u32);
                let pb = strip.get_pixel(x as u32, strip_y as u32);
                diff += (pa[0] as i32 - pb[0] as i32).unsigned_abs() as u64;
                diff += (pa[1] as i32 - pb[1] as i32).unsigned_abs() as u64;
                diff += (pa[2] as i32 - pb[2] as i32).unsigned_abs() as u64;
            }
            if diff > best_score { break; }
        }

        if diff < best_score {
            best_score = diff;
            best_overlap = overlap;
        }
    }

    prev_bottom - best_overlap as u32
}

fn detect_sticky_top_simple(a: &RgbaImage, b: &RgbaImage) -> usize {
    let h = a.height() as usize;
    let w = a.width() as usize;
    let mut sticky = 0;
    for y in 0..h / 3 {
        if rows_match(a, b, y, w) {
            sticky = y + 1;
        } else {
            break;
        }
    }
    sticky
}

fn detect_vanishing_bottom(first: &RgbaImage, second: &RgbaImage, scroll_delta: usize) -> usize {
    let h = first.height() as usize;
    let w = first.width() as usize;
    if scroll_delta == 0 || scroll_delta >= h { return 0; }

    // Walk from the bottom of frame[0] upward. Check if the row in frame[0]
    // matches the corresponding row in frame[1] (shifted by scroll_delta).
    // Non-matching rows at the bottom = nav bar that vanished on scroll.
    for offset in 0..h / 3 {
        let y_first = h - 1 - offset;
        if y_first < scroll_delta { break; }
        let y_second = y_first - scroll_delta;
        if rows_match_at(first, y_first, second, y_second, w) {
            return offset;
        }
    }
    0
}

fn rows_match(a: &RgbaImage, b: &RgbaImage, y: usize, w: usize) -> bool {
    let mut mismatches = 0;
    let threshold = w / 10;
    for x in (0..w).step_by(2) {
        let pa = a.get_pixel(x as u32, y as u32);
        let pb = b.get_pixel(x as u32, y as u32);
        let dr = (pa[0] as i32 - pb[0] as i32).abs();
        let dg = (pa[1] as i32 - pb[1] as i32).abs();
        let db = (pa[2] as i32 - pb[2] as i32).abs();
        if dr > 5 || dg > 5 || db > 5 {
            mismatches += 1;
            if mismatches > threshold { return false; }
        }
    }
    true
}

fn rows_match_at(a: &RgbaImage, ya: usize, b: &RgbaImage, yb: usize, w: usize) -> bool {
    let mut mismatches = 0;
    let threshold = w / 10;
    for x in (0..w).step_by(2) {
        let pa = a.get_pixel(x as u32, ya as u32);
        let pb = b.get_pixel(x as u32, yb as u32);
        let dr = (pa[0] as i32 - pb[0] as i32).abs();
        let dg = (pa[1] as i32 - pb[1] as i32).abs();
        let db = (pa[2] as i32 - pb[2] as i32).abs();
        if dr > 5 || dg > 5 || db > 5 {
            mismatches += 1;
            if mismatches > threshold { return false; }
        }
    }
    true
}


fn crop_strip(img: &RgbaImage, top: usize, bottom: usize) -> RgbaImage {
    let w = img.width();
    let h = (bottom - top) as u32;
    image::imageops::crop_imm(img, 0, top as u32, w, h).to_image()
}

fn is_near_identical(a: &RgbaImage, b: &RgbaImage) -> bool {
    if a.dimensions() != b.dimensions() { return false; }
    let total_pixels = a.width() as u64 * a.height() as u64;
    let mut matching = 0u64;
    let sample_step = 7;

    for y in (0..a.height()).step_by(sample_step) {
        for x in (0..a.width()).step_by(sample_step) {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            let dr = (pa[0] as i32 - pb[0] as i32).abs();
            let dg = (pa[1] as i32 - pb[1] as i32).abs();
            let db = (pa[2] as i32 - pb[2] as i32).abs();
            if dr + dg + db < 15 {
                matching += 1;
            }
        }
    }

    let sampled = total_pixels / (sample_step as u64 * sample_step as u64);
    matching * 100 / sampled > 99
}
