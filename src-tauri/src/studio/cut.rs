//! Turns generated key-pose sheets into a Baa character pack: clean the alpha
//! (Sprite Studio's defringe + orphan-pixel pass), find the six drawings, stand
//! each one on a shared ground line at one shared scale, and write the sheets +
//! character.json. Hops keep their height above the ground; sitting stays low.

use image::{imageops, Rgba, RgbaImage};
use serde_json::json;
use std::path::Path;

const VISIBLE: u8 = 64;
const KEY_THRESHOLD_SQ: f64 = 72.0 * 72.0;
/// How tall a standing frame is in the output, px (shown at ~75 px, so 3x for sharp scaling)
pub const STAND_H: f64 = 230.0;

/// The standing height to cut at: STAND_H, or BAA_STAND_H for bigger exports (the website).
fn stand_h() -> f64 {
    std::env::var("BAA_STAND_H").ok().and_then(|v| v.parse().ok()).unwrap_or(STAND_H)
}
/// Pixel-art characters: a standing pose is this many real pixels tall...
pub const STAND_PX: f64 = 48.0;
/// ...and each pixel is stored as a crisp block this big (the app draws them pixelated).
pub const PIXEL_BLOCK: u32 = 4;
const PAD: u32 = 8;

// ---------- alpha cleanup (ported from Sprite Studio's pixel_normalize, MIT) ----------

fn lum(p: Rgba<u8>) -> f64 {
    0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64
}

fn touches_clear(im: &RgbaImage, x: u32, y: u32) -> bool {
    let (w, h) = im.dimensions();
    (x > 0 && im.get_pixel(x - 1, y)[3] == 0)
        || (y > 0 && im.get_pixel(x, y - 1)[3] == 0)
        || (x + 1 < w && im.get_pixel(x + 1, y)[3] == 0)
        || (y + 1 < h && im.get_pixel(x, y + 1)[3] == 0)
}

/// Strips the light halo an image model leaves around a cut-out.
fn defringe(im: &mut RgbaImage) {
    for _ in 0..2 {
        let mut clear = vec![];
        for (x, y, p) in im.enumerate_pixels() {
            if p[3] > 0 && touches_clear(im, x, y) && (p[3] < VISIBLE || (lum(*p) > 190.0 && p[3] < 255)) {
                clear.push((x, y));
            }
        }
        for (x, y) in clear {
            im.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }
    }
}

/// Real alpha is kept. Without it, the background is removed by flooding in from the
/// edges: a flat colour (keyed from the corners) or a painted "transparency"
/// checkerboard (light, colourless squares, which image models often draw instead of
/// real transparency). Only what the flood reaches goes, so white eyes, teeth or wool
/// inside the character's outline stay.
pub fn clean(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let corners = [src.get_pixel(0, 0), src.get_pixel(w - 1, 0), src.get_pixel(0, h - 1), src.get_pixel(w - 1, h - 1)];
    let native = corners.iter().map(|p| p[3] as u32).sum::<u32>() <= 64;
    let mut out = src.clone();
    if native {
        for p in out.pixels_mut() {
            if p[3] < VISIBLE {
                *p = Rgba([0, 0, 0, 0]);
            }
        }
    } else {
        let key = [0, 1, 2].map(|c| (corners.iter().map(|p| p[c] as u32).sum::<u32>() / 4) as f64);
        let is_bg = |p: &Rgba<u8>| {
            let d = (0..3).map(|c| (p[c] as f64 - key[c]).powi(2)).sum::<f64>();
            let (mx, mn) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
            d <= KEY_THRESHOLD_SQ || (mx - mn <= 14 && lum(*p) >= 175.0) // checkerboard squares
        };
        let mut gone = vec![false; (w * h) as usize];
        let mut stack: Vec<(u32, u32)> = vec![];
        for x in 0..w {
            stack.push((x, 0));
            stack.push((x, h - 1));
        }
        for y in 0..h {
            stack.push((0, y));
            stack.push((w - 1, y));
        }
        while let Some((x, y)) = stack.pop() {
            let i = (y * w + x) as usize;
            if gone[i] || !is_bg(src.get_pixel(x, y)) {
                continue;
            }
            gone[i] = true;
            if x > 0 { stack.push((x - 1, y)); }
            if y > 0 { stack.push((x, y - 1)); }
            if x + 1 < w { stack.push((x + 1, y)); }
            if y + 1 < h { stack.push((x, y + 1)); }
        }
        for (i, p) in out.pixels_mut().enumerate() {
            *p = if gone[i] { Rgba([0, 0, 0, 0]) } else { Rgba([p[0], p[1], p[2], 255]) };
        }
    }
    defringe(&mut out);
    out
}

// ---------- finding the frames ----------

#[derive(Clone, Copy, Debug)]
pub struct Box4 {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// Mask cells are this many pixels square.
const S: u32 = 4;

/// One drawing on a sheet: its box (sheet px) and its label in the cell map.
#[derive(Clone, Copy, Debug)]
pub struct Found {
    pub b: Box4,
    id: u32,
}

/// The drawings on a sheet, plus which drawing each 4x4 cell belongs to, so a frame
/// keeps only its own pixels (not a neighbour's ear poking into its box).
pub struct Frames {
    pub list: Vec<Found>,
    labels: Vec<u32>,
    w: u32,
}

impl Frames {
    pub fn owns(&self, f: &Found, x: u32, y: u32) -> bool {
        self.labels.get(((y / S) * self.w + x / S) as usize) == Some(&f.id)
    }
}

/// Connected regions of `dil`: per-cell labels and (area, box in cells) per label.
fn label(dil: &[bool], w: u32, h: u32) -> (Vec<u32>, Vec<(u32, Box4)>) {
    let mut label = vec![0u32; dil.len()];
    let mut comps: Vec<(u32, Box4)> = vec![];
    for start in 0..dil.len() {
        if !dil[start] || label[start] != 0 {
            continue;
        }
        let id = comps.len() as u32 + 1;
        let mut stack = vec![start];
        label[start] = id;
        let mut b = Box4 { x0: u32::MAX, y0: u32::MAX, x1: 0, y1: 0 };
        let mut area = 0;
        while let Some(i) = stack.pop() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            area += 1;
            b.x0 = b.x0.min(x);
            b.y0 = b.y0.min(y);
            b.x1 = b.x1.max(x);
            b.y1 = b.y1.max(y);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h {
                    let j = (ny * w + nx) as usize;
                    if dil[j] && label[j] == 0 {
                        label[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
        comps.push((area, b));
    }
    (label, comps)
}

/// `n` drawings of similar size? (the n-th biggest isn't a stray speck)
fn enough(comps: &[(u32, Box4)], n: usize) -> bool {
    let mut areas: Vec<u32> = comps.iter().map(|c| c.0).collect();
    areas.sort_by(|a, b| b.cmp(a));
    areas.len() >= n && areas[n - 1] * 8 >= areas[0]
}

/// The `n` biggest separate drawings, in reading order. Works on a 4x-smaller mask
/// with a small dilation, so a wisp of fur doesn't split a frame in two. When two
/// drawings touch (a sitting pose's boots on the ears of the frame below), the sheet
/// is split along the emptiest line near each boundary of its `cols` x `rows` grid.
pub fn find_frames(im: &RgbaImage, n: usize, cols: u32) -> anyhow::Result<Frames> {
    let (w, h) = (im.width().div_ceil(S), im.height().div_ceil(S));
    let rows = (n as u32).div_ceil(cols.max(1));
    let mut mask = vec![false; (w * h) as usize];
    for (x, y, p) in im.enumerate_pixels() {
        if p[3] >= VISIBLE {
            mask[((y / S) * w + x / S) as usize] = true;
        }
    }
    // dilate by 2 cells
    let mut dil = mask.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if mask[(y * w as i32 + x) as usize] {
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                            dil[(ny * w as i32 + nx) as usize] = true;
                        }
                    }
                }
            }
        }
    }
    let (mut labels, mut comps) = label(&dil, w, h);
    if !enough(&comps, n) {
        // the emptiest row/column within an eighth of the sheet of each grid line
        let ink_row = |y: u32| (0..w).filter(|&x| mask[(y * w + x) as usize]).count();
        let ink_col = |x: u32| (0..h).filter(|&y| mask[(y * w + x) as usize]).count();
        for k in 1..rows {
            let c = h * k / rows;
            let (lo, hi) = (c.saturating_sub(h / 8), (c + h / 8).min(h - 1));
            let y = (lo..=hi).min_by_key(|&y| ink_row(y)).unwrap_or(c);
            for x in 0..w {
                dil[(y * w + x) as usize] = false;
            }
        }
        for k in 1..cols {
            let c = w * k / cols;
            let (lo, hi) = (c.saturating_sub(w / 10), (c + w / 10).min(w - 1));
            let x = (lo..=hi).min_by_key(|&x| ink_col(x)).unwrap_or(c);
            for y in 0..h {
                dil[(y * w + x) as usize] = false;
            }
        }
        (labels, comps) = label(&dil, w, h);
    }
    if comps.len() < n {
        anyhow::bail!("found {} drawings on the sheet, expected {n}", comps.len());
    }
    if !enough(&comps, n) {
        anyhow::bail!("one of the {n} frames is tiny: the sheet probably has fewer drawings");
    }
    let mut ids: Vec<usize> = (0..comps.len()).collect();
    ids.sort_by(|a, b| comps[*b].0.cmp(&comps[*a].0));
    let mut list: Vec<Found> = ids[..n]
        .iter()
        .map(|&i| {
            let b = comps[i].1;
            Found {
                b: Box4 { x0: b.x0 * S, y0: b.y0 * S, x1: ((b.x1 + 1) * S).min(im.width()), y1: ((b.y1 + 1) * S).min(im.height()) },
                id: i as u32 + 1,
            }
        })
        .collect();
    // reading order: rows by centre, then left to right
    list.sort_by_key(|f| (f.b.y0 + f.b.y1) / 2);
    let mut grid: Vec<Vec<Found>> = vec![];
    for f in list {
        match grid.last_mut() {
            Some(r) if ((f.b.y0 + f.b.y1) as i64 / 2 - (r[0].b.y0 + r[0].b.y1) as i64 / 2).abs() < (f.b.y1 - f.b.y0) as i64 * 4 / 10 => r.push(f),
            _ => grid.push(vec![f]),
        }
    }
    let list = grid
        .into_iter()
        .flat_map(|mut r| {
            r.sort_by_key(|f| f.b.x0);
            r
        })
        .collect();
    Ok(Frames { list, labels, w })
}

// ---------- registration ----------

pub struct Frame {
    pub img: RgbaImage,
    /// x of the feet centre in `img`
    pub foot_x: f64,
    /// how far the feet are above the row's ground line, px (hops)
    pub lift: f64,
}

fn tight(img: &RgbaImage) -> Option<Box4> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] >= VISIBLE {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 != u32::MAX).then_some(Box4 { x0, y0, x1: x1 + 1, y1: y1 + 1 })
}

/// Centre of what touches the ground: the bottom 8% of the drawing.
fn feet_centre(img: &RgbaImage) -> f64 {
    let h = img.height();
    let band = (h as f64 * 0.08).max(2.0) as u32;
    let (mut lo, mut hi) = (u32::MAX, 0);
    for y in h.saturating_sub(band)..h {
        for x in 0..img.width() {
            if img.get_pixel(x, y)[3] >= VISIBLE {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    if lo == u32::MAX {
        img.width() as f64 / 2.0
    } else {
        (lo + hi) as f64 / 2.0
    }
}

/// Premultiplied resize, so transparent edges don't turn dark.
fn resize(img: &RgbaImage, s: f64) -> RgbaImage {
    let (w, h) = (((img.width() as f64 * s).round() as u32).max(1), ((img.height() as f64 * s).round() as u32).max(1));
    let mut pm = img.clone();
    for p in pm.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = (p[c] as u32 * a / 255) as u8;
        }
    }
    let mut out = imageops::resize(&pm, w, h, imageops::FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p[3] as u32;
        if a > 0 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * 255 / a).min(255)) as u8;
            }
        }
    }
    out
}

/// Pixel art: puts an AI "pixel art" picture onto a real pixel grid. It's shrunk to its
/// true pixel size by averaging, edges are made hard (no soft halo), colours are rounded
/// to a small palette (Sprite Studio's channel quantize, step 16), then every pixel is
/// blown back up into a PIXEL_BLOCK square with no smoothing.
fn pixelate(img: &RgbaImage, s: f64) -> RgbaImage {
    let (w, h) = (((img.width() as f64 * s).round() as u32).max(1), ((img.height() as f64 * s).round() as u32).max(1));
    let mut small = resize(img, s);
    let small = if small.dimensions() == (w, h) { small } else { imageops::resize(&mut small, w, h, imageops::FilterType::Triangle) };
    let mut hard = small.clone();
    for p in hard.pixels_mut() {
        if p[3] < 110 {
            *p = Rgba([0, 0, 0, 0]);
        } else {
            for c in 0..3 {
                p[c] = (((p[c] as u32 + 8) / 16 * 16).min(255)) as u8;
            }
            p[3] = 255;
        }
    }
    imageops::resize(&hard, w * PIXEL_BLOCK, h * PIXEL_BLOCK, imageops::FilterType::Nearest)
}

/// Cuts one sheet into registered frames, all scaled so a standing pose is STAND_H tall
/// (or, for pixel art, STAND_PX real pixels in PIXEL_BLOCK blocks).
pub fn cut_sheet(path: &Path, n: usize, pixel: bool) -> anyhow::Result<Vec<Frame>> {
    let src = clean(&image::open(path)?.to_rgba8());
    let found = find_frames(&src, n, 3)?;
    let boxes: Vec<Box4> = found.list.iter().map(|f| f.b).collect();
    // the ground for each row of the grid: the lowest feet in that row
    let mut raw = vec![];
    for f in &found.list {
        let b = f.b;
        let mut crop = imageops::crop_imm(&src, b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0).to_image();
        for (x, y, p) in crop.enumerate_pixels_mut() {
            if !found.owns(f, b.x0 + x, b.y0 + y) {
                *p = Rgba([0, 0, 0, 0]); // a neighbour's pixels inside this box
            }
        }
        let t = tight(&crop).ok_or_else(|| anyhow::anyhow!("empty frame"))?;
        let img = imageops::crop_imm(&crop, t.x0, t.y0, t.x1 - t.x0, t.y1 - t.y0).to_image();
        raw.push((img, b.y0 + t.y1, (b.y0 + b.y1) / 2));
    }
    let row_of = |cy: u32| boxes.iter().filter(|b| (b.y0 + b.y1) / 2 < cy.saturating_sub(10)).count() / 3;
    let mut ground = std::collections::HashMap::new();
    for (_, bottom, cy) in &raw {
        let g = ground.entry(row_of(*cy)).or_insert(0u32);
        *g = (*g).max(*bottom);
    }
    // scale: the standing frames (not crouched/sitting) set it
    let tallest = raw.iter().map(|r| r.0.height()).max().unwrap_or(1) as f64;
    let mut standing: Vec<f64> = raw.iter().map(|r| r.0.height() as f64).filter(|h| *h >= tallest * 0.8).collect();
    standing.sort_by(|a, b| a.total_cmp(b));
    let s = if pixel { STAND_PX } else { stand_h() } / standing[standing.len() / 2];
    Ok(raw
        .into_iter()
        .map(|(img, bottom, cy)| {
            let mut lift = (ground[&row_of(cy)] - bottom) as f64 * s;
            let img = if pixel {
                lift = lift.round() * PIXEL_BLOCK as f64; // hops move in whole pixels
                pixelate(&img, s)
            } else {
                resize(&img, s)
            };
            let foot_x = feet_centre(&img);
            Frame { img, foot_x, lift }
        })
        .collect())
}

/// What each mood plays, for packs made with the studio's standard sheets.
pub fn standard_states() -> serde_json::Value {
    json!({
        "appear":  {"sheet": "appear", "frames": [0, 1, 2, 3, 4, 5, 4, 5], "fps": 9, "mode": "once"},
        "idle":    {"sheet": "idle", "frames": [0,0,0,0,1,1,0,0,0,0,2,0,0,0,1,1,0,0,0,0,0,3,3,3,0,0,4,4,0,0,0,0,0,0,5,5,5,0], "fps": 4, "mode": "loop"},
        "listen":  {"sheet": "talk", "frames": [0, 1, 2, 3], "extra": [4, 5], "mode": "mouth"},
        "think":   {"sheet": "think", "frames": [0, 0, 1, 1, 2, 3, 2, 3], "fps": 4, "mode": "loop"},
        "send":    {"sheet": "send", "frames": [0, 1, 2, 3, 4, 4, 5], "fps": 10, "mode": "once"},
        "nothing": {"sheet": "error", "frames": [0, 0, 5, 5], "fps": 4, "mode": "once"},
        "error":   {"sheet": "error", "frames": [0, 1, 2, 1, 2, 3, 4], "fps": 6, "mode": "once"},
        "cancel":  {"sheet": "idle", "frames": [3, 3, 0], "fps": 6, "mode": "once"},
        "sleep":   {"sheet": "sleep", "intro": [0, 0, 1, 1], "frames": [2, 2, 3, 3], "fps": 2, "mode": "loop"},
        "wake":    {"sheet": "sleep", "frames": [4, 4, 5], "fps": 6, "mode": "once"}
    })
}

/// Lays the cut sheets out on one shared slot and writes the pack into `out`.
pub fn write_pack(out: &Path, meta: &serde_json::Value, sheets: &[(String, Vec<Frame>)], states: serde_json::Value) -> anyhow::Result<()> {
    std::fs::create_dir_all(out)?;
    let pixel = meta.get("pixel").and_then(|v| v.as_bool()).unwrap_or(false);
    let all = sheets.iter().flat_map(|(_, f)| f);
    let half = all.clone().map(|f| f.foot_x.max(f.img.width() as f64 - f.foot_x)).fold(0.0, f64::max);
    let sw = (half * 2.0).ceil() as u32 + PAD * 2;
    let sh = all.map(|f| f.img.height() as f64 + f.lift).fold(0.0, f64::max).ceil() as u32 + PAD;
    let mut sheets_meta = serde_json::Map::new();
    for (name, frames) in sheets {
        let cols = 3u32.min(frames.len() as u32);
        let rows = (frames.len() as u32).div_ceil(cols);
        let mut sheet = RgbaImage::new(sw * cols, sh * rows);
        let mut heads = vec![];
        for (k, f) in frames.iter().enumerate() {
            let (cx, cy) = ((k as u32 % cols) * sw, (k as u32 / cols) * sh);
            let mut ox = (cx as f64 + sw as f64 / 2.0 - f.foot_x).round() as i64;
            if pixel {
                ox -= (ox - cx as i64).rem_euclid(PIXEL_BLOCK as i64); // stay on the pixel grid
            }
            let oy = (cy + sh) as i64 - f.img.height() as i64 - f.lift.round() as i64;
            imageops::overlay(&mut sheet, &f.img, ox, oy);
            // top-centre of the drawing: where the speech bubble's tail points
            let top = (0..f.img.height()).find(|&y| (0..f.img.width()).any(|x| f.img.get_pixel(x, y)[3] >= VISIBLE)).unwrap_or(0);
            let xs: Vec<u32> = (0..f.img.width()).filter(|&x| (top..(top + 6).min(f.img.height())).any(|y| f.img.get_pixel(x, y)[3] >= VISIBLE)).collect();
            let hx = xs.first().zip(xs.last()).map(|(a, b)| (*a + *b) as f64 / 2.0).unwrap_or(f.img.width() as f64 / 2.0);
            heads.push(json!([((ox - cx as i64) as f64 + hx).round(), (oy - cy as i64) + top as i64]));
        }
        let file = format!("{name}.webp");
        let mut buf = std::io::BufWriter::new(std::fs::File::create(out.join(&file))?);
        image::codecs::webp::WebPEncoder::new_lossless(&mut buf).encode(sheet.as_raw(), sheet.width(), sheet.height(), image::ExtendedColorType::Rgba8)?;
        sheets_meta.insert(name.clone(), json!({"file": file, "cols": cols, "rows": rows, "frames": frames.len(), "heads": heads}));
    }
    let mut char = meta.clone();
    char["format"] = json!(1);
    char["full_body"] = json!(true);
    char["slot"] = json!({"w": sw, "h": sh});
    char["stand_height"] = json!(if pixel { STAND_PX * PIXEL_BLOCK as f64 } else { stand_h() });
    char["sheets"] = serde_json::Value::Object(sheets_meta);
    char["states"] = states;
    std::fs::write(out.join("character.json"), serde_json::to_vec_pretty(&char)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                img.put_pixel(xx, yy, Rgba([30, 30, 30, 255]));
            }
        }
    }

    #[test]
    fn finds_six_frames_in_reading_order() {
        let mut img = RgbaImage::new(600, 400);
        for k in 0..6u32 {
            let (cx, cy) = ((k % 3) * 200 + 60, (k / 3) * 200 + 40);
            blob(&mut img, cx, cy, 60 + k, 120);
        }
        let f = find_frames(&img, 6, 3).unwrap();
        let b: Vec<Box4> = f.list.iter().map(|f| f.b).collect();
        assert_eq!(b.len(), 6);
        assert!(b[0].x0 < b[1].x0 && b[1].x0 < b[2].x0 && b[2].y1 < b[3].y0);
    }

    #[test]
    fn touching_frames_are_split_on_the_grid() {
        let mut img = RgbaImage::new(600, 400);
        for k in 0..6u32 {
            blob(&mut img, (k % 3) * 200 + 60, (k / 3) * 200 + 30, 70, 150);
        }
        blob(&mut img, 270, 175, 8, 60); // an ear from the bottom-middle frame reaching up into the top one
        let f = find_frames(&img, 6, 3).unwrap();
        assert_eq!(f.list.len(), 6);
        let (top, bottom) = (f.list[1], f.list[4]);
        assert!(top.b.y1 <= bottom.b.y0 + 8, "split between the rows");
        assert!(!f.owns(&top, 273, 228) || !f.owns(&bottom, 273, 228));
    }

    #[test]
    fn a_missing_frame_is_caught() {
        let mut img = RgbaImage::new(600, 400);
        for k in 0..5u32 {
            blob(&mut img, (k % 3) * 200 + 60, (k / 3) * 200 + 40, 60, 120);
        }
        blob(&mut img, 500, 380, 3, 3); // a stray speck is not a frame
        assert!(find_frames(&img, 6, 3).is_err());
    }

    #[test]
    fn removes_a_painted_checkerboard_but_keeps_white_inside() {
        let mut img = RgbaImage::new(40, 40);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = if (x / 8 + y / 8) % 2 == 0 { Rgba([255, 255, 255, 255]) } else { Rgba([204, 204, 204, 255]) };
        }
        blob(&mut img, 10, 10, 20, 20); // dark outline box
        for yy in 13..27 {
            for xx in 13..27 {
                img.put_pixel(xx, yy, Rgba([255, 255, 255, 255])); // white eye inside
            }
        }
        let c = clean(&img);
        assert_eq!(c.get_pixel(1, 1)[3], 0);
        assert_eq!(c.get_pixel(9, 1)[3], 0);
        assert_eq!(c.get_pixel(20, 20)[3], 255);
    }

    #[test]
    fn keys_out_a_flat_background() {
        let mut img = RgbaImage::from_pixel(20, 20, Rgba([250, 250, 250, 255]));
        blob(&mut img, 5, 5, 8, 8);
        let c = clean(&img);
        assert_eq!(c.get_pixel(0, 0)[3], 0);
        assert_eq!(c.get_pixel(8, 8)[3], 255);
    }
}
