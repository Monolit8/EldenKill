//! V1's view on top of Elden Ring's: ULTRAKILL's arms, guns, projectiles, explosions and HUD,
//! from the link's overlay triple buffer, keyed (ULTRAKILL clears its background to magenta) and
//! drawn over Elden Ring's final image by a DirectX 12 overlay (hudhook, as er-mario's HUD).
//! Elden Ring's camera follows V1's, so the two line up.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use hudhook::imgui::{self, TextureId};
use hudhook::{ImguiRenderLoop, RenderContext};

use crate::log;
use crate::proto::*;

/// Draw the overlay (set by the frame task: V1 mode on and ULTRAKILL sending).
pub static SHOW: AtomicBool = AtomicBool::new(false);
/// Elden Ring's back buffer size (for the host state: ULTRAKILL renders at this size).
pub static VIEW_W: AtomicU32 = AtomicU32::new(0);
pub static VIEW_H: AtomicU32 = AtomicU32::new(0);

/// A status line (waiting for ULTRAKILL, ...), drawn top left when set.
pub static STATUS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// V1's HP, hard damage and dash stamina (0-300) while V1 plays, for the health bar
pub static V1_HUD: std::sync::Mutex<Option<(i32, f32, f32)>> = std::sync::Mutex::new(None);
/// ULTRAKILL's style: rank (bits 0-7), meter fill 0-255 (bits 8-15), combo going (bit 16)
pub static STYLE: AtomicU32 = AtomicU32::new(0);
/// ULTRAKILL's HUD settings (hudType, crossHair, crossHairColor, crossHairHud, styleMeter,
/// hudBackgroundOpacity): the overlay's HUD follows them
pub static HUD_PREFS: std::sync::Mutex<(i32, i32, i32, i32, bool, f32)> = std::sync::Mutex::new((1, 1, 1, 0, true, 60.0));

fn crosshair_colour(c: i32) -> [f32; 4] {
    match c {
        2 => [0.5, 0.5, 0.5, 1.0],
        3 => [0.0, 0.0, 0.0, 1.0],
        4 => [1.0, 0.0, 0.0, 1.0],
        5 => [0.0, 1.0, 0.0, 1.0],
        6 => [0.0, 0.0, 1.0, 1.0],
        7 => [0.0, 1.0, 1.0, 1.0],
        8 => [1.0, 0.92, 0.016, 1.0],
        9 => [1.0, 0.0, 1.0, 1.0],
        _ => [1.0, 1.0, 1.0, 0.9],
    }
}

/// the menu pointer (client pixels, x << 16 | y) while ULTRAKILL's menu is open
pub static MENU_POINTER: AtomicU32 = AtomicU32::new(0);
static PICTURE: std::sync::Mutex<([f32; 2], [f32; 2])> = std::sync::Mutex::new(([0.0, 0.0], [1.0, 1.0]));

/// Where V1's picture is drawn in the window (top left, size), from the last frame.
pub fn picture_rect() -> ([f32; 2], [f32; 2]) {
    *PICTURE.lock().unwrap_or_else(|e| e.into_inner())
}

/// ULTRAKILL's style feed ("+ HEADSHOT", "+ KILL"...): lines with their colours
static STYLE_FEED: std::sync::Mutex<(u32, Vec<(String, [f32; 4])>)> = std::sync::Mutex::new((0, Vec::new()));
/// when V1 last hit something (the crosshair's hit marker)
static LAST_HIT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub fn note_hit() {
    *LAST_HIT.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
}

/// Takes ULTRAKILL's style feed text when it changed: TextMeshPro lines, `<color=#rrggbb>` kept as
/// the line's colour, other tags dropped.
pub fn read_style_text(link: &crate::link::Link) {
    let mut feed = STYLE_FEED.lock().unwrap_or_else(|e| e.into_inner());
    let Some((seq, text)) = link.style_text(feed.0) else { return };
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut colour = [1.0, 1.0, 1.0, 1.0];
        let mut out = String::new();
        let mut rest = raw;
        while let Some(i) = rest.find('<') {
            out.push_str(&rest[..i]);
            let Some(j) = rest[i..].find('>') else { break };
            let tag = &rest[i + 1..i + j];
            if let Some(hex) = tag.strip_prefix("color=#") {
                let v = |k: usize| u8::from_str_radix(hex.get(k..k + 2).unwrap_or("ff"), 16).unwrap_or(255) as f32 / 255.0;
                colour = [v(0), v(2), v(4), 1.0];
            }
            rest = &rest[i + j + 1..];
        }
        out.push_str(rest);
        let out = out.trim().to_string();
        if !out.is_empty() {
            lines.push((out, colour));
        }
    }
    *feed = (seq, lines);
}

const RANKS: [(&str, &str, [f32; 4]); 8] = [
    ("D", "DESTRUCTIVE", [0.35, 0.6, 1.0, 1.0]),
    ("C", "CHAOTIC", [0.3, 0.9, 0.35, 1.0]),
    ("B", "BRUTAL", [1.0, 0.85, 0.2, 1.0]),
    ("A", "ANARCHIC", [1.0, 0.55, 0.1, 1.0]),
    ("S", "SUPREME", [1.0, 0.2, 0.15, 1.0]),
    ("SS", "SSADISTIC", [1.0, 0.15, 0.15, 1.0]),
    ("SSS", "SSSHITSTORM", [1.0, 0.1, 0.1, 1.0]),
    ("U", "ULTRAKILL", [1.0, 0.8, 0.2, 1.0]),
];

/// V1's health and dashes (bottom left) and the bosses' health (bottom centre): Elden Ring's HUD
/// is hidden while V1 plays, and ULTRAKILL's doesn't come through the overlay.
fn draw_hud(ui: &imgui::Ui, at: [f32; 2], size: [f32; 2]) {
    // (the background list, after V1's picture: the text windows go on top of the bars)
    let draw = ui.get_background_draw_list();
    let rect = |a: [f32; 2], b: [f32; 2], c: [f32; 4]| draw.add_rect(a, b, c).filled(true).build();
    let scale = (size[1] / 1080.0).max(0.6);
    let (hud_type, crosshair, crosshair_col, _crosshair_hud, style_meter, bg_opacity) = *HUD_PREFS.lock().unwrap_or_else(|e| e.into_inner());
    let bg = (bg_opacity / 100.0).clamp(0.0, 1.0);
    let v1 = *V1_HUD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((hp, hard, stamina)) = v1.filter(|_| hud_type != 0) {
        let (x, y, w, hh) = (at[0] + 40.0 * scale, at[1] + size[1] - 110.0 * scale, 380.0 * scale, 26.0 * scale);
        rect([x - 3.0, y - 3.0], [x + w + 3.0, y + hh + 3.0], [0.0, 0.0, 0.0, bg]);
        let f = (hp.max(0) as f32 / 100.0).min(1.0);
        rect([x, y], [x + w * f, y + hh], [0.85, 0.1, 0.1, 0.95]);
        // hard damage: the part of the bar that can't heal back for now
        let hard = (hard / 100.0).clamp(0.0, 1.0);
        if hard > 0.0 {
            rect([x + w * (1.0 - hard), y], [x + w, y + hh], [0.35, 0.35, 0.35, 0.95]);
        }
        ui.window("##ek_hp")
            .position([x + 6.0, y - 2.0 * scale], imgui::Condition::Always)
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .scroll_bar(false)
            .no_inputs()
            .bg_alpha(0.0)
            .always_auto_resize(true)
            .build(|| {
                ui.set_window_font_scale(1.5 * scale);
                ui.text(format!("{hp}"));
            });
        // three dashes
        let (py, ph, gap) = (y + hh + 8.0 * scale, 10.0 * scale, 6.0 * scale);
        let pw = (w - gap * 2.0) / 3.0;
        for i in 0..3 {
            let px = x + i as f32 * (pw + gap);
            let fill = ((stamina - i as f32 * 100.0) / 100.0).clamp(0.0, 1.0);
            rect([px - 2.0, py - 2.0], [px + pw + 2.0, py + ph + 2.0], [0.0, 0.0, 0.0, 0.6]);
            rect([px, py], [px + pw * fill, py + ph], if fill >= 1.0 { [0.2, 0.75, 1.0, 0.95] } else { [0.15, 0.4, 0.55, 0.95] });
        }
    }
    // the style meter (top right), while a combo is going
    let style = STYLE.load(Ordering::Relaxed);
    // (only while there's style to show: an empty D sat in the corner all the time)
    let scoring = style & 0xFF > 0 || (style >> 8) & 0xFF > 0;
    if v1.is_some() && hud_type != 0 && style_meter && style & (1 << 16) != 0 && scoring {
        let (letter, name, colour) = RANKS[((style & 0xFF) as usize).min(RANKS.len() - 1)];
        let fill = ((style >> 8) & 0xFF) as f32 / 255.0;
        let w = 260.0 * scale;
        let x = at[0] + size[0] - w - 40.0 * scale;
        let y = at[1] + 140.0 * scale;
        rect([x - 10.0, y - 10.0], [x + w + 10.0, y + 70.0 * scale], [0.0, 0.0, 0.0, 0.55]);
        ui.window("##ek_style")
            .position([x, y - 6.0 * scale], imgui::Condition::Always)
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .scroll_bar(false)
            .no_inputs()
            .bg_alpha(0.0)
            .always_auto_resize(true)
            .build(|| {
                ui.set_window_font_scale(2.6 * scale);
                ui.text_colored(colour, letter);
                ui.same_line();
                ui.set_window_font_scale(1.5 * scale);
                ui.text(name);
            });
        let by = y + 40.0 * scale;
        rect([x, by], [x + w, by + 10.0 * scale], [0.2, 0.2, 0.2, 0.9]);
        rect([x, by], [x + w * fill, by + 10.0 * scale], colour);
    }
    // the style feed under the meter
    if v1.is_some() {
        let feed = STYLE_FEED.lock().unwrap_or_else(|e| e.into_inner()).1.clone();
        if !feed.is_empty() && hud_type != 0 && style_meter {
            let x = at[0] + size[0] - 300.0 * scale - 40.0 * scale;
            let y = at[1] + 225.0 * scale;
            ui.window("##ek_feed")
                .position([x, y], imgui::Condition::Always)
                .title_bar(false)
                .resizable(false)
                .movable(false)
                .scroll_bar(false)
                .no_inputs()
                .bg_alpha(0.35)
                .always_auto_resize(true)
                .build(|| {
                    ui.set_window_font_scale(1.3 * scale);
                    for (line, colour) in feed.iter().take(8) {
                        ui.text_colored(*colour, line);
                    }
                });
        }
        // ULTRAKILL's crosshair: four short lines and a dot; an X on a hit (not over its menu)
        let c = [at[0] + size[0] * 0.5, at[1] + size[1] * 0.5];
        if super::cursor::MENU_MODE.load(Ordering::Relaxed) {
            return;
        }
        let fg = ui.get_foreground_draw_list();
        let (gap, len, th) = (5.0 * scale, 9.0 * scale, 2.0 * scale);
        let white = crosshair_colour(crosshair_col);
        // the crosshair: eldenkill.ini's `crosshair` (0 none, the default; 1 the cross; 2 the cross
        // and its corners; -1 = ULTRAKILL's own setting)
        static FORCED: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
        let forced = *FORCED.get_or_init(|| crate::paths::config_f32("crosshair", 0.0) as i32);
        let crosshair = if forced >= 0 { forced } else { crosshair };
        if crosshair >= 1 {
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                fg.add_line([c[0] + dx * gap, c[1] + dy * gap], [c[0] + dx * (gap + len), c[1] + dy * (gap + len)], white).thickness(th).build();
            }
            fg.add_circle(c, 1.5 * scale, white).filled(true).build();
        }
        if crosshair >= 2 {
            let (a, b) = (20.0 * scale, 26.0 * scale);
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                fg.add_line([c[0] + sx * a, c[1] + sy * b], [c[0] + sx * b, c[1] + sy * b], white).thickness(th).build();
                fg.add_line([c[0] + sx * b, c[1] + sy * a], [c[0] + sx * b, c[1] + sy * b], white).thickness(th).build();
            }
        }
        let hit = LAST_HIT.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| t.elapsed().as_secs_f32() < 0.15);
        if hit {
            let (a, b) = (7.0 * scale, 14.0 * scale);
            let red = [1.0, 0.25, 0.2, 1.0];
            for (dx, dy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                fg.add_line([c[0] + dx * a, c[1] + dy * a], [c[0] + dx * b, c[1] + dy * b], red).thickness(th).build();
            }
        }
    }
    // Elden Ring's boss bars, in its style: the name over a long thin red bar in a dark frame, the
    // HP lost to the last hits in pale gold draining after it, and the damage number on the right
    static TRAIL: std::sync::Mutex<Vec<(f32, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());
    let bosses = super::combat::BOSSES.lock().unwrap_or_else(|e| e.into_inner());
    let mut trail = TRAIL.lock().unwrap_or_else(|e| e.into_inner());
    trail.resize(bosses.len(), (1.0, std::time::Instant::now()));
    for (i, b) in bosses.iter().enumerate() {
        let w = size[0] * 0.48;
        let hh = 9.0 * scale;
        let x = at[0] + (size[0] - w) * 0.5;
        let y = at[1] + size[1] - (95.0 + i as f32 * 52.0) * scale;
        let f = (b.hp as f32 / b.max as f32).clamp(0.0, 1.0);
        // the pale part: where the bar was, draining after a moment
        let (t, since) = &mut trail[i];
        if f > *t {
            *t = f;
        }
        if since.elapsed().as_secs_f32() > 0.7 && *t > f {
            *t = (*t - 0.35 * ui.io().delta_time).max(f);
        }
        if (b.before as f32 / b.max as f32) > f && *t <= f + 0.0001 {
            *since = std::time::Instant::now();
            *t = (b.before as f32 / b.max as f32).clamp(f, 1.0);
        }
        // frame: a dark band with a thin dull-gold edge
        rect([x - 4.0, y - 4.0], [x + w + 4.0, y + hh + 4.0], [0.55, 0.47, 0.30, 0.85]);
        rect([x - 2.0, y - 2.0], [x + w + 2.0, y + hh + 2.0], [0.05, 0.04, 0.03, 0.95]);
        rect([x, y], [x + w * *t, y + hh], [0.83, 0.71, 0.45, 0.95]);
        rect([x, y], [x + w * f, y + hh], [0.60, 0.08, 0.06, 1.0]);
        rect([x, y], [x + w * f, y + hh * 0.35], [0.75, 0.16, 0.12, 1.0]);
        ui.window(format!("##ek_boss{i}"))
            .position([x - 2.0, y - 30.0 * scale], imgui::Condition::Always)
            .size([w + 4.0, 28.0 * scale], imgui::Condition::Always)
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .scroll_bar(false)
            .no_inputs()
            .bg_alpha(0.0)
            .build(|| {
                ui.set_window_font_scale(1.35 * scale);
                ui.text_colored([0.93, 0.90, 0.82, 1.0], &b.name);
                if b.damage > 0 && since.elapsed().as_secs_f32() < 3.0 {
                    let label = format!("{}", b.damage);
                    let tw = ui.calc_text_size(&label)[0];
                    ui.same_line_with_pos(w + 4.0 - tw - 6.0);
                    ui.text_colored([1.0, 1.0, 1.0, 1.0], &label);
                }
            });
    }
}

pub fn set_status(text: Option<String>) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

struct Overlay {
    texture: Option<(TextureId, u32, u32)>,
    rgba: Vec<u8>,
    have_frame: bool,
    frames: u64,
}

pub fn install(module: usize) -> bool {
    use hudhook::hooks::dx12::ImguiDx12Hooks;
    let hmodule = hudhook::windows::Win32::Foundation::HINSTANCE(module as _);
    let overlay = Overlay { texture: None, rgba: Vec::new(), have_frame: false, frames: 0 };
    match hudhook::Hudhook::builder().with::<ImguiDx12Hooks>(overlay).with_hmodule(hmodule).build().apply() {
        Ok(()) => {
            log("overlay: hooked DirectX 12");
            true
        }
        Err(e) => {
            log(format!("overlay: hook failed: {e:?}"));
            false
        }
    }
}

/// Where Elden Ring's picture is in the window: it renders 16:9 (`aspect` in eldenkill.ini), centred
/// with black bars on wider or taller screens (a 3440x1440 window shows a 2560x1440 picture).
fn picture(window: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    static ASPECT: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    let aspect = *ASPECT.get_or_init(|| crate::paths::config_f32("aspect", 0.0));
    let [w, h] = window;
    // 0: the whole window (the ultrawide patch: Elden Ring fills it)
    if w <= 0.0 || h <= 0.0 || aspect <= 0.0 {
        return ([0.0, 0.0], window);
    }
    let aspect = aspect.clamp(1.0, 4.0);
    let (pw, ph) = if w / h > aspect { (h * aspect, h) } else { (w, w / aspect) };
    ([((w - pw) * 0.5).round(), ((h - ph) * 0.5).round()], [pw.round(), ph.round()])
}

/// ULTRAKILL's frame -> straight RGBA with the key colour made transparent, flipped upright.
fn convert(out: &mut Vec<u8>, w: usize, h: usize, flags: u32, src: &[u8]) {
    out.resize(w * h * 4, 0);
    let key = flags & OV_CHROMA_KEY != 0;
    let bottom_up = flags & OV_BOTTOM_UP != 0;
    for y in 0..h {
        let sy = if bottom_up { h - 1 - y } else { y };
        let s = &src[sy * w * 4..(sy + 1) * w * 4];
        let d = &mut out[y * w * 4..(y + 1) * w * 4];
        for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
            let (r, g, b) = (sp[0], sp[1], sp[2]);
            // the key colour, give or take ULTRAKILL's colour grading (and its edges, half keyed)
            let a = if !key {
                sp[3]
            } else if r > 230 && g < 30 && b > 230 {
                0
            } else if r > 200 && g < 70 && b > 200 {
                110
            } else {
                255
            };
            dp.copy_from_slice(&[r, g, b, a]);
        }
    }
}

impl ImguiRenderLoop for Overlay {
    fn initialize<'a>(&'a mut self, ctx: &mut imgui::Context, _render_context: &'a mut dyn RenderContext) {
        ctx.io_mut().mouse_draw_cursor = false;
        ctx.set_ini_filename(None);
    }

    fn before_render<'a>(&'a mut self, ctx: &mut imgui::Context, render_context: &'a mut dyn RenderContext) {
        let (_, size) = picture(ctx.io().display_size);
        VIEW_W.store(size[0] as u32, Ordering::Relaxed);
        VIEW_H.store(size[1] as u32, Ordering::Relaxed);
        // imgui must never take the game's mouse or keyboard
        ctx.io_mut().config_flags.insert(imgui::ConfigFlags::NO_MOUSE);
        let Some(link) = super::link() else { return };
        let Some(frame) = link.take_overlay() else { return };
        let (w, h) = (frame.width as u32, frame.height as u32);
        convert(&mut self.rgba, frame.width, frame.height, frame.flags, frame.pixels);
        match self.texture {
            Some((id, tw, th)) if tw == w && th == h => {
                if let Err(e) = render_context.replace_texture(id, &self.rgba, w, h) {
                    log(format!("overlay: texture update failed: {e:?}"));
                }
            }
            _ => match render_context.load_texture(&self.rgba, w, h) {
                Ok(id) => {
                    log(format!("overlay: ULTRAKILL's view is {w}x{h}"));
                    self.texture = Some((id, w, h));
                }
                Err(e) => log(format!("overlay: texture creation failed: {e:?}")),
            },
        }
        self.have_frame = true;
        self.frames += 1;
    }

    fn render(&mut self, ui: &mut imgui::Ui) {
        let (at, size) = picture(ui.io().display_size);
        *PICTURE.lock().unwrap_or_else(|e| e.into_inner()) = (at, size);
        if SHOW.load(Ordering::Relaxed) && self.have_frame {
            if let Some((id, _, _)) = self.texture {
                ui.get_background_draw_list().add_image(id, at, [at[0] + size[0], at[1] + size[1]]).build();
            }
        }
        if SHOW.load(Ordering::Relaxed) {
            draw_hud(ui, at, size);
        }
        // ULTRAKILL's menu pointer (the game hides the real cursor)
        if super::cursor::MENU_MODE.load(Ordering::Relaxed) {
            let v = MENU_POINTER.load(Ordering::Relaxed);
            let p = [(v >> 16) as f32, (v & 0xFFFF) as f32];
            let fg = ui.get_foreground_draw_list();
            fg.add_triangle(p, [p[0] + 4.0, p[1] + 18.0], [p[0] + 13.0, p[1] + 13.0], [1.0, 1.0, 1.0, 1.0]).filled(true).build();
            fg.add_triangle(p, [p[0] + 4.0, p[1] + 18.0], [p[0] + 13.0, p[1] + 13.0], [0.0, 0.0, 0.0, 1.0]).thickness(1.5).build();
        }
        super::debug::draw(ui, at);
        if let Some(text) = STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let draw = ui.get_foreground_draw_list();
            draw.add_text([13.0, 13.0], [0.0, 0.0, 0.0, 1.0], &text);
            draw.add_text([12.0, 12.0], [1.0, 1.0, 1.0, 1.0], &text);
        }
    }
}
