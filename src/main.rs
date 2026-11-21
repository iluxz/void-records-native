mod audio;
mod data;

use audio::Engine;
use data::{fmt_time, log, matches_mood, music_dir, tracks, LogEntry, State, Track, MOODS};
use eframe::egui;
use rand::rng;
use rand::seq::SliceRandom;
use std::time::{Duration, Instant};

fn col(c: (u8, u8, u8)) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

#[derive(Clone, Copy)]
struct Palette {
    name: &'static str,
    bg: (u8, u8, u8),
    panel: (u8, u8, u8),
    ink: (u8, u8, u8),
    dim: (u8, u8, u8),
    amber: (u8, u8, u8),
    purple: (u8, u8, u8),
    cyan: (u8, u8, u8),
    line: (u8, u8, u8),
}

const THEMES: [Palette; 5] = [
    Palette { name: "void", bg: (10, 8, 18), panel: (20, 16, 32), ink: (232, 224, 255), dim: (122, 113, 150), amber: (255, 180, 58), purple: (139, 92, 246), cyan: (94, 230, 255), line: (42, 35, 64) },
    Palette { name: "halloween", bg: (13, 5, 8), panel: (31, 13, 18), ink: (255, 232, 214), dim: (166, 106, 75), amber: (255, 123, 0), purple: (123, 31, 162), cyan: (57, 255, 20), line: (74, 30, 36) },
    Palette { name: "ocean", bg: (3, 12, 20), panel: (10, 26, 42), ink: (214, 240, 255), dim: (90, 138, 170), amber: (0, 212, 255), purple: (0, 102, 255), cyan: (0, 255, 204), line: (19, 51, 77) },
    Palette { name: "forest", bg: (5, 15, 8), panel: (15, 31, 18), ink: (224, 255, 221), dim: (90, 138, 90), amber: (158, 255, 0), purple: (34, 139, 34), cyan: (127, 255, 212), line: (26, 58, 31) },
    Palette { name: "sunset", bg: (21, 8, 16), panel: (40, 18, 28), ink: (255, 228, 240), dim: (176, 106, 134), amber: (255, 94, 120), purple: (255, 42, 109), cyan: (255, 204, 0), line: (74, 30, 48) },
];

struct VrApp {
    tracks: Vec<Track>,
    log: Vec<LogEntry>,
    order: Vec<usize>,
    pos: Option<usize>,
    visible: Vec<usize>,
    filter: String,
    mood: usize,
    state: State,
    engine: Option<Engine>,
    engine_error: String,
    duration: Option<Duration>,
    seek_frac: f32,
    seek_drag: bool,
    spinning: f32,
    last_frame: Instant,
    show_settings: bool,
    show_log: bool,
    show_about: bool,
    status: String,
    last_theme: String,
}

impl VrApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = State::load();
        let tracks = tracks();
        let mut order: Vec<usize> = (0..tracks.len()).collect();
        if state.shuffle {
            order.shuffle(&mut rng());
        }
        let engine = match Engine::new() {
            Ok(e) => Some(e),
            Err(_e) => None,
        };
        let engine_error = if engine.is_none() {
            "no audio device found".to_string()
        } else {
            String::new()
        };
        let mut app = Self {
            tracks,
            log: log(),
            order,
            pos: None,
            visible: Vec::new(),
            filter: String::new(),
            mood: 0,
            state,
            engine,
            engine_error,
            duration: None,
            seek_frac: 0.0,
            seek_drag: false,
            spinning: 0.0,
            last_frame: Instant::now(),
            show_settings: false,
            show_log: false,
            show_about: false,
            status: "click a record to play it".to_string(),
            last_theme: String::new(),
        };
        app.rebuild_visible();
        app
    }

    fn pal(&self) -> &Palette {
        let name = &self.state.theme;
        THEMES.iter().find(|t| t.name == name).unwrap_or(&THEMES[0])
    }

    fn apply_visuals(&self, ctx: &egui::Context) {
        let p = self.pal();
        let mut v = egui::Visuals::dark();
        v.panel_fill = col(p.bg);
        v.window_fill = col(p.panel);
        v.extreme_bg_color = col(p.bg);
        v.selection.bg_fill = col(p.purple);
        v.hyperlink_color = col(p.cyan);
        v.override_text_color = Some(col(p.ink));
        v.weak_text_color = Some(col(p.dim));
        v.widgets.noninteractive.fg_stroke.color = col(p.ink);
        v.widgets.inactive.fg_stroke.color = col(p.dim);
        v.widgets.hovered.fg_stroke.color = col(p.cyan);
        v.widgets.active.fg_stroke.color = col(p.amber);
        v.widgets.inactive.bg_fill = col(p.panel);
        v.widgets.hovered.bg_fill = col(p.line);
        v.widgets.active.bg_fill = col(p.line);
        ctx.set_visuals(v);
    }

    fn rebuild_visible(&mut self) {
        let q = self.filter.trim().to_lowercase();
        let qn: String = q.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let fav_mode = matches!(qn.as_str(), "favorites" | "favs" | "favourites" | "starred");
        let rec_mode = matches!(qn.as_str(), "recent" | "recents" | "history" | "lastplayed");
        let favs = self.state.fav_set();
        let mut idxs: Vec<usize> = if rec_mode {
            let mut v: Vec<usize> = (0..self.tracks.len())
                .filter(|i| self.state.last.contains_key(&self.tracks[*i].f))
                .collect();
            v.sort_by_key(|i| std::cmp::Reverse(self.state.last[&self.tracks[*i].f]));
            v
        } else {
            (0..self.tracks.len()).collect()
        };
        idxs.retain(|i| {
            let t = &self.tracks[*i];
            if fav_mode && !favs.contains(&t.f) { return false; }
            if !fav_mode && !rec_mode && !q.is_empty() {
                let hay = format!("{} {} {}", t.n, t.a.join(" "), t.f).to_lowercase();
                if !hay.contains(&q) { return false; }
            }
            matches_mood(t, MOODS[self.mood])
        });
        self.visible = idxs;
    }

    fn save(&mut self) {
        self.state.shuffle = false;
        self.state.save();
    }

    fn load(&mut self, idx: usize) {
        let Some(t) = self.tracks.get(idx).cloned() else { return };
        let path = music_dir().join(&t.f);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        *self.state.plays.entry(t.f.clone()).or_insert(0) += 1;
        self.state.last.insert(t.f.clone(), now);
        self.pos = Some(idx);
        self.seek_frac = 0.0;
        self.save();
        self.rebuild_visible();
        if let Some(e) = self.engine.as_mut() {
            self.duration = e.play(&path, Duration::ZERO, self.state.speed, self.state.volume);
            self.status = match &e.error {
                Some(err) => format!("error: {err}"),
                None => format!("playing {}", t.n),
            };
        } else {
            self.status = format!("no audio engine: {}", self.engine_error);
        }
    }

    fn toggle(&mut self) {
        let Some(e) = self.engine.as_mut() else { return };
        if e.paused { e.resume(); } else { e.pause(); }
    }

    fn step(&mut self, delta: isize) {
        if self.order.is_empty() { return; }
        let at = self.pos.and_then(|p| self.order.iter().position(|x| *x == p)).unwrap_or(0);
        let n = (at as isize + delta).rem_euclid(self.order.len() as isize) as usize;
        self.load(self.order[n]);
    }

    fn next(&mut self) { self.step(1); }
    fn prev(&mut self) { self.step(-1); }

    fn seek_to(&mut self, frac: f32) {
        let Some(dur) = self.duration else { return };
        let d = Duration::from_secs_f64((frac as f64 * dur.as_secs_f64()).max(0.0));
        if let Some(e) = self.engine.as_mut() {
            e.seek(d, self.state.speed, self.state.volume);
        }
        self.seek_frac = frac;
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Space) { self.toggle(); }
            if i.key_pressed(egui::Key::ArrowRight) { self.next(); }
            if i.key_pressed(egui::Key::ArrowLeft) { self.prev(); }
            if i.key_pressed(egui::Key::S) {
                self.state.shuffle = !self.state.shuffle;
                if self.state.shuffle { self.order.shuffle(&mut rng()); }
                else { self.order = (0..self.tracks.len()).collect(); }
                self.state.save();
            }
            if i.key_pressed(egui::Key::L) {
                self.state.loop_one = !self.state.loop_one;
                self.state.save();
            }
        });
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(f) = dropped.first().map(|f| f.path().to_path_buf()) {
            if let Some(e) = self.engine.as_mut() {
                self.duration = e.play(&f, Duration::ZERO, self.state.speed, self.state.volume);
                self.status = format!("local file: {}", f.file_name().unwrap_or_default().to_string_lossy());
            }
        }
    }

    fn draw_deck(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let amber = col(p.amber);
        let cyan = col(p.cyan);
        let dim = col(p.dim);
        let line = col(p.line);
        let playing = self.engine.as_ref().map(|e| e.is_playing()).unwrap_or(false);

        // turntable panel - fixed height, painted like the web app's #turntable
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(396.0, 230.0), egui::Sense::hover());
        let paint = ui.painter();
        paint.rect_filled(rect, 6.0, col((27, 21, 48)));
        paint.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, line), egui::StrokeKind::Inside);

        // vinyl disc
        let center = egui::pos2(rect.min.x + 24.0 + 95.0, rect.center().y);
        let r = 95.0;
        paint.circle_filled(center + egui::vec2(4.0, 5.0), r + 2.0, egui::Color32::from_gray(4));
        paint.circle_filled(center, r, egui::Color32::from_rgb(12, 12, 12));
        for i in 0..14 {
            let rr = r * (0.34 + i as f32 * 0.047);
            paint.circle_stroke(center, rr, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));
        }
        // sheen highlight that spins with the record
        let a = self.spinning;
        for k in 0..3 {
            let ang = a + k as f32 * 2.094;
            let tip = egui::pos2(center.x + ang.cos() * r * 0.9, center.y + ang.sin() * r * 0.9);
            paint.line_segment([center, tip], egui::Stroke::new(2.0, egui::Color32::from_white_alpha(14)));
        }
        // center label
        let label_r = 37.0;
        paint.circle_filled(center, label_r, col((59, 29, 110)));
        paint.circle_stroke(center, label_r, egui::Stroke::new(2.0, amber));
        let label = self.pos
            .map(|i| self.tracks[i].n.clone())
            .unwrap_or_else(|| "void records".into());
        let short: String = label.chars().take(18).collect();
        paint.text(
            center,
            egui::Align2::CENTER_CENTER,
            short,
            egui::FontId::new(9.0, egui::FontFamily::Monospace),
            amber,
        );
        paint.circle_filled(center, 4.0, col(p.bg));

        // tonearm
        let base = egui::pos2(rect.max.x - 52.0, rect.min.y + 16.0);
        let ang = if playing { 46.0f32 } else { 35.0f32 }.to_radians();
        let tip = egui::pos2(base.x - ang.sin() * 172.0, base.y + ang.cos() * 172.0);
        paint.line_segment([base, tip], egui::Stroke::new(7.0, egui::Color32::from_gray(190)));
        paint.circle_filled(base, 9.0, egui::Color32::from_gray(120));
        paint.rect_filled(
            egui::Rect::from_center_size(tip, egui::vec2(24.0, 28.0)),
            3.0,
            col((43, 37, 64)),
        );

        // now playing
        let title = self.pos.map(|i| self.tracks[i].n.clone()).unwrap_or_else(|| "no record loaded".into());
        ui.label(egui::RichText::new(&title).size(16.0).color(cyan));
        let aliases = self.pos
            .map(|i| if self.tracks[i].a.is_empty() { "void records".into() } else { format!("aka {}", self.tracks[i].a.join(", ")) })
            .unwrap_or_else(|| "click a record on the right".into());
        ui.label(egui::RichText::new(aliases).size(11.0).color(dim));

        // seek bar + times
        let pos = self.engine.as_ref().map(|e| e.position()).unwrap_or_default();
        let dur = self.duration.unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(fmt_time(pos)).size(11.0).color(dim));
            ui.allocate_ui_with_layout(
                egui::Vec2::new(ui.available_width() - 4.0, 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let resp = ui.add(egui::Slider::new(&mut self.seek_frac, 0.0..=1.0).show_value(false));
                    if resp.dragged() { self.seek_drag = true; }
                    if resp.drag_stopped() {
                        self.seek_drag = false;
                        self.seek_to(self.seek_frac);
                    }
                },
            );
            ui.label(egui::RichText::new(fmt_time(dur)).size(11.0).color(dim));
        });

        // transport chips
        ui.horizontal(|ui| {
            let chip = |ui: &mut egui::Ui, label: &str, on: bool| {
                let txt = egui::RichText::new(label).size(12.0).color(if on { col(p.bg) } else { dim });
                let b = egui::Button::new(txt)
                    .fill(if on { amber } else { col(p.panel) })
                    .stroke(egui::Stroke::new(1.0, if on { amber } else { line }))
                    .corner_radius(10.0);
                ui.add(b).clicked()
            };
            if chip(ui, "\u{23EE}", false) { self.prev(); }
            let play_label = if playing { "\u{275A}\u{275A}" } else { "\u{25B6}" };
            if chip(ui, play_label, playing) { self.toggle(); }
            if chip(ui, "\u{23ED}", false) { self.next(); }
            if chip(ui, "\u{1F0CF} shuffle", self.state.shuffle) {
                self.state.shuffle = !self.state.shuffle;
                if self.state.shuffle { self.order.shuffle(&mut rng()); }
                else { self.order = (0..self.tracks.len()).collect(); }
                self.save();
            }
            if chip(ui, "\u{1F501} loop", self.state.loop_one) {
                self.state.loop_one = !self.state.loop_one;
                self.save();
            }
        });

        // volume
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("\u{1F50A}").color(dim));
            if ui.add(egui::Slider::new(&mut self.state.volume, 0..=100u8).show_value(true)).changed() {
                if let Some(e) = self.engine.as_ref() { e.set_volume(self.state.volume); }
                self.save();
            }
        });

        if ui.button("settings").clicked() { self.show_settings = !self.show_settings; }
        if self.show_settings {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("speed");
                if ui.add(egui::Slider::new(&mut self.state.speed, 0.5..=2.0f32)).changed() {
                    if let Some(e) = self.engine.as_mut() {
                        if let Some(pth) = e.source_path().map(|p| p.to_path_buf()) {
                            e.play(&pth, e.position(), self.state.speed, self.state.volume);
                        }
                    }
                    self.save();
                }
            });
            ui.horizontal(|ui| {
                ui.label("theme");
                egui::ComboBox::from_id_salt("theme")
                    .selected_text(self.state.theme.clone())
                    .show_ui(ui, |ui| {
                        for t in THEMES.iter() {
                            if ui.selectable_label(self.state.theme == t.name, t.name).clicked() {
                                self.state.theme = t.name.to_string();
                                self.save();
                            }
                        }
                    });
            });
            ui.label(egui::RichText::new("video bg / crackle / lyrics are web-only for now").size(10.0).color(dim));
        }

        ui.add_space(4.0);
        ui.label(egui::RichText::new(&self.status).size(11.0).color(dim));
        if !self.engine_error.is_empty() {
            ui.label(egui::RichText::new(&self.engine_error).color(egui::Color32::RED).size(11.0));
        }
        ui.label(egui::RichText::new("space play/pause  \u{2190}\u{2192} skip  s shuffle  l loop").size(10.0).color(dim));
    }

    fn draw_list(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let amber = col(p.amber);
        let cyan = col(p.cyan);
        let dim = col(p.dim);
        let line = col(p.line);

        // mood chips
        ui.horizontal(|ui| {
            for (i, m) in MOODS.iter().enumerate() {
                let on = self.mood == i;
                let txt = egui::RichText::new(*m).size(10.0).color(if on { col(p.bg) } else { dim });
                let b = egui::Button::new(txt)
                    .fill(if on { amber } else { col(p.panel) })
                    .stroke(egui::Stroke::new(1.0, if on { amber } else { line }))
                    .corner_radius(10.0);
                if ui.add(b).clicked() {
                    self.mood = i;
                    self.rebuild_visible();
                }
            }
        });

        let resp = ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("filter\u{2026}"));
        if resp.changed() { self.rebuild_visible(); }
        ui.label(egui::RichText::new(format!("{} of {} records", self.visible.len(), self.tracks.len())).size(11.0).color(dim));
        ui.separator();

        let favs = self.state.fav_set();
        let visible = self.visible.clone();
        let plays = self.state.plays.clone();
        let pos = self.pos;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for idx in visible {
                let t = &self.tracks[idx];
                let f = t.f.clone();
                let n = t.n.clone();
                let aliases = t.a.join(" ");
                let is_fav = favs.contains(&f);
                let is_current = pos == Some(idx);
                let count = plays.get(&f).copied().unwrap_or(0);
                let row = ui.horizontal(|ui| {
                    let star = if is_fav { "\u{2605}" } else { "\u{2606}" };
                    if ui.add(egui::Button::new(egui::RichText::new(star).color(if is_fav { amber } else { dim })).frame(false)).clicked() {
                        self.state.toggle_fav(&f);
                        self.save();
                        self.rebuild_visible();
                    }
                    let prefix = if is_current { "\u{25B8} " } else { "" };
                    let label = format!("{}{:>3}  {}", prefix, idx + 1, n);
                    let mut rich = egui::RichText::new(label).size(13.0);
                    if is_current { rich = rich.color(amber).strong(); } else { rich = rich.color(col(p.ink)); }
                    if ui.add(egui::Button::new(rich).frame(false)).clicked() {
                        self.load(idx);
                    }
                    ui.label(egui::RichText::new(aliases).size(10.0).color(dim));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if count > 0 {
                            ui.label(egui::RichText::new(format!("\u{00D7}{count}")).size(10.0).color(dim));
                        }
                    });
                });
                // playing-row tint
                if is_current {
                    ui.painter().rect_filled(row.response.rect, 2.0, col(p.amber).gamma_multiply(0.12));
                }
            }
            if self.visible.is_empty() {
                ui.label(egui::RichText::new("nothing matches.").color(dim));
            }
        });
        let _ = cyan;
    }

    fn draw_windows(&mut self, ctx: &egui::Context) {
        if self.show_log {
            egui::Window::new("update log").open(&mut self.show_log)
                .default_size(egui::Vec2::new(620.0, 520.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for e in &self.log {
                            ui.heading(&e.title);
                            ui.label(egui::RichText::new(format!("{} \u{00B7} {}", e.date, e.kind)).weak().small());
                            for n in &e.notes {
                                ui.label(format!("\u{2022} {n}"));
                            }
                            ui.separator();
                        }
                    });
                });
        }
        if self.show_about {
            egui::Window::new("about").open(&mut self.show_about)
                .default_size(egui::Vec2::new(520.0, 420.0))
                .show(ctx, |ui| {
                    ui.heading("void records (native)");
                    ui.label("a small rust + egui record player for the void records shelf.");
                    ui.label("no electron, no webview. just the shelf and the music.");
                    ui.separator();
                    ui.label("built by iluxz. same shelf as the web app.");
                    ui.label("webm/mp4 play through a tiny ffmpeg pipe so video files work too.");
                    ui.hyperlink("https://iluxz.github.io/void-records/");
                });
        }
    }
}

impl eframe::App for VrApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if self.last_theme != self.state.theme {
            self.apply_visuals(ui.ctx());
            self.last_theme = self.state.theme.clone();
        }
        self.handle_keys(ui.ctx());

        let ended = self.engine.as_ref().map(|e| e.ended()).unwrap_or(false);
        if ended {
            if self.state.loop_one {
                if let Some(i) = self.pos { self.load(i); }
            } else {
                self.next();
            }
        }

        let playing = self.engine.as_ref().map(|e| e.is_playing()).unwrap_or(false);
        if playing {
            self.spinning += dt * 2.4 * self.state.speed;
            // cap repaint to ~30fps instead of max fps - huge perf win
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
        if !self.seek_drag {
            if let (Some(e), Some(d)) = (self.engine.as_ref(), self.duration) {
                if !d.is_zero() {
                    self.seek_frac = (e.position().as_secs_f32() / d.as_secs_f32()).clamp(0.0, 1.0);
                }
            }
        }

        // header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("void records").size(28.0).color(col(self.pal().amber)).strong());
            ui.label(egui::RichText::new(format!("{} tracks", self.tracks.len())).size(11.0).color(col(self.pal().dim)));
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::Vec2::new(396.0, ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.draw_deck(ui),
            );
            ui.separator();
            ui.vertical(|ui| self.draw_list(ui));
        });

        self.draw_windows(ui.ctx());
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("void records")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "void records",
        options,
        Box::new(|cc| Ok(Box::new(VrApp::new(cc)))),
    )
}
