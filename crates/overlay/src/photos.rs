//! Screenshots in the headset (monado-frame, folded into monadeck): watch
//! `~/Pictures/Monado`, queue new shots as a card on the wrist watch, open them
//! as floating photo windows (copy / delete / translate / share), and a paged
//! gallery on the dashboard's Photos page. Heavy image work runs off-thread.
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::Result;
use ash::vk;
use egui_phosphor::regular as icon;
use openxr as xr;

use crate::gfx::{make_panel, quad_layer, render_panel, theme, PanelGfx};
use crate::mathx::{front_pose, pose_compose, pose_invert, raycast};
use crate::shots::{self, PhotoAction, ShotOutcome};

pub const SLOTS: usize = 3;
const MAX_PENDING: usize = 8;
const GALLERY_PER: usize = 12;
const PHOTO_PX: (u32, u32) = (1200, 820);
const PHOTO_W: f32 = 0.62;
const GRAB_START: f32 = 0.40;
const GRAB_RELEASE: f32 = 0.15;

/// Photo-related settings (persisted in the overlay config).
#[derive(Clone, PartialEq)]
pub struct PhotoCfg {
    pub qr_detect: bool,
    pub qr_autodelete: bool,
    pub skip_wrist_photo: bool,
    pub skip_wrist_qr: bool,
    pub cleanup_days: i32,
    pub crop_margin: i32,
}

/// A queued wrist notification: a screenshot (preview + date) or a decoded QR.
pub struct Pending {
    pub path: PathBuf,
    pub when: String,
    thumb_img: Option<egui::ColorImage>,
    pub thumb: Option<egui::TextureHandle>,
    pub qr: Option<String>,
}

pub struct PhotoSlot {
    pub gfx: PanelGfx,
    pub open: bool,
    photo: Option<egui::TextureHandle>,
    path: Option<PathBuf>,
    text: Option<String>,
    show_text: bool,
    loading: bool,
    translating: bool,
    sharing: bool,
    share_msg: Option<String>,
    when: String,
    grab: Option<(usize, xr::Posef)>,
}

type AsyncMsg = (usize, PathBuf, Result<String, String>);
type PhotoLoadMsg = (usize, PathBuf, egui::ColorImage);
type ShotMsg = (PathBuf, String, ShotOutcome);
type GalleryMsg = (u64, Vec<(egui::ColorImage, String)>);

pub struct Gallery {
    paths: Vec<(PathBuf, String)>,
    pub page: usize,
    pending: Vec<(egui::ColorImage, String)>,
    pub items: Vec<(egui::TextureHandle, String)>,
    pub loading: bool,
    gen: u64,
    /// Scanned at least once (the page opened).
    active: bool,
}

pub struct Photos {
    pub dir: String,
    newest_seen: Option<SystemTime>,
    last_scan: Instant,
    shot_tx: mpsc::Sender<ShotMsg>,
    shot_rx: mpsc::Receiver<ShotMsg>,
    photo_tx: mpsc::Sender<PhotoLoadMsg>,
    photo_rx: mpsc::Receiver<PhotoLoadMsg>,
    async_tx: mpsc::Sender<(bool, AsyncMsg)>, // (is_share, msg)
    async_rx: mpsc::Receiver<(bool, AsyncMsg)>,
    gallery_tx: mpsc::Sender<GalleryMsg>,
    gallery_rx: mpsc::Receiver<GalleryMsg>,
    pub pending: Vec<Pending>,
    pub pending_idx: usize,
    pub slots: Vec<PhotoSlot>,
    pub gallery: Gallery,
    /// A new screenshot landed this frame (haptic/sound cue).
    pub notify_pulse: bool,
    pub translate_ok: bool,
    pub share_ok: bool,
}

/// What the wrist card wants done (drained by the main loop).
#[derive(Default)]
pub struct WristRequests {
    pub open: bool,
    pub dismiss: bool,
    pub older: bool,
    pub newer: bool,
}

#[derive(Default)]
pub struct GalleryRequests {
    pub open: Option<usize>,
    pub delete: Option<usize>,
    pub prev: bool,
    pub next: bool,
    pub refresh: bool,
}

pub struct PhotoInput {
    pub ray: Option<(xr::Posef, f32)>,
    pub ptr: [Option<(f32, f32, bool)>; SLOTS],
    pub hit_t: Option<f32>,
}

impl Photos {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: &xr::Session<xr::Vulkan>,
        device: &ash::Device,
        allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>,
        render_pass: vk::RenderPass,
        format: vk::Format,
        srgb: bool,
        cfg: &PhotoCfg,
    ) -> Result<Self> {
        let dir = std::env::var("MONADO_SCREENSHOT_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}/Pictures/Monado", std::env::var("HOME").unwrap_or_default()));
        let cleaned = shots::cleanup_old(&dir, cfg.cleanup_days);
        if cleaned > 0 {
            log::info!("photos: removed {cleaned} screenshot(s) older than {} days", cfg.cleanup_days);
        }
        let mut slots = Vec::with_capacity(SLOTS);
        for _ in 0..SLOTS {
            let gfx = make_panel(
                session,
                device,
                allocator.clone(),
                render_pass,
                format,
                srgb,
                PHOTO_PX,
                (PHOTO_W, PHOTO_W * PHOTO_PX.1 as f32 / PHOTO_PX.0 as f32),
                xr::Posef::IDENTITY,
            )?;
            slots.push(PhotoSlot {
                gfx,
                open: false,
                photo: None,
                path: None,
                text: None,
                show_text: false,
                loading: false,
                translating: false,
                sharing: false,
                share_msg: None,
                when: String::new(),
                grab: None,
            });
        }
        let (shot_tx, shot_rx) = mpsc::channel();
        let (photo_tx, photo_rx) = mpsc::channel();
        let (async_tx, async_rx) = mpsc::channel();
        let (gallery_tx, gallery_rx) = mpsc::channel();
        let newest_seen = shots::scan_all(&dir).first().map(|(_, m)| *m);
        log::info!("photos: watching {dir} for new screenshots");
        Ok(Self {
            dir,
            newest_seen,
            last_scan: Instant::now(),
            shot_tx,
            shot_rx,
            photo_tx,
            photo_rx,
            async_tx,
            async_rx,
            gallery_tx,
            gallery_rx,
            pending: Vec::new(),
            pending_idx: 0,
            slots,
            gallery: Gallery { paths: Vec::new(), page: 0, pending: Vec::new(), items: Vec::new(), loading: false, gen: 0, active: false },
            notify_pulse: false,
            translate_ok: shots::translate::configured(),
            share_ok: shots::picsur::configured(),
        })
    }

    // --- Watching + async results -------------------------------------------

    pub fn poll(&mut self, cfg: &PhotoCfg, hmd: Option<&xr::Posef>) {
        self.notify_pulse = false;
        if self.last_scan.elapsed().as_secs_f32() > 1.0 {
            self.last_scan = Instant::now();
            let all = shots::scan_all(&self.dir);
            let fresh: Vec<PathBuf> = all
                .iter()
                .take_while(|(_, m)| self.newest_seen.is_none_or(|s| *m > s))
                .map(|(p, _)| p.clone())
                .collect();
            if let Some((_, m)) = all.first() {
                self.newest_seen = Some(*m);
            }
            self.notify_pulse = !fresh.is_empty();
            for path in fresh.into_iter().rev() {
                log::info!("photos: new screenshot {}", path.display());
                let tx = self.shot_tx.clone();
                let (qd, qa, crop) = (cfg.qr_detect, cfg.qr_autodelete, cfg.crop_margin);
                std::thread::spawn(move || {
                    let when = shots::shot_time(&path);
                    match shots::process_new_shot(&path, qd, qa, crop, 256) {
                        Ok(outcome) => {
                            let _ = tx.send((path, when, outcome));
                        }
                        Err(e) => log::warn!("photos: process {path:?}: {e}"),
                    }
                });
            }
        }
        while let Ok((path, when, outcome)) = self.shot_rx.try_recv() {
            match outcome {
                ShotOutcome::Qr(content) => {
                    if cfg.skip_wrist_qr {
                        self.open_qr(&content, &when, hmd);
                    } else {
                        self.pending.insert(0, Pending { path, when, thumb_img: None, thumb: None, qr: Some(content) });
                        self.pending.truncate(MAX_PENDING);
                        self.pending_idx = 0;
                    }
                }
                ShotOutcome::Photo(color) => {
                    if cfg.skip_wrist_photo {
                        self.open_photo(&path, &when, hmd);
                    } else {
                        self.pending.insert(0, Pending { path, when, thumb_img: Some(color), thumb: None, qr: None });
                        self.pending.truncate(MAX_PENDING);
                        self.pending_idx = 0;
                    }
                }
            }
            if self.gallery.active {
                self.gallery_rescan();
            }
        }
        while let Ok((slot, path, color)) = self.photo_rx.try_recv() {
            if let Some(s) = self.slots.get_mut(slot) {
                if s.open && s.loading && s.path.as_deref() == Some(path.as_path()) {
                    s.photo = Some(s.gfx.ctx.load_texture("screenshot", color, egui::TextureOptions::LINEAR));
                    s.loading = false;
                }
            }
        }
        while let Ok((is_share, (i, path, res))) = self.async_rx.try_recv() {
            let Some(s) = self.slots.get_mut(i) else { continue };
            if !s.open || s.path.as_deref() != Some(path.as_path()) {
                continue;
            }
            if is_share {
                s.sharing = false;
                s.share_msg = Some(match res {
                    Ok(url) => {
                        shots::copy_text_to_clipboard(&url);
                        format!("{}  Link copied: {}", icon::CHECK, truncate(&url, 40))
                    }
                    Err(e) => format!("Share failed: {e}"),
                });
            } else {
                s.translating = false;
                s.show_text = true;
                s.text = Some(match res {
                    Ok(t) => t,
                    Err(e) => format!("Translation failed:\n{e}"),
                });
            }
        }
        while let Ok((gen, items)) = self.gallery_rx.try_recv() {
            if gen == self.gallery.gen {
                self.gallery.pending = items;
                self.gallery.loading = false;
            }
        }
    }

    /// Upload thumbnails decoded for the current gallery page into `ctx`
    /// (the dashboard's main panel context).
    pub fn gallery_textures(&mut self, ctx: &egui::Context) {
        if self.gallery.pending.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.gallery.pending);
        self.gallery.items = items
            .into_iter()
            .enumerate()
            .map(|(k, (img, when))| (ctx.load_texture(format!("gallery-{k}"), img, egui::TextureOptions::LINEAR), when))
            .collect();
    }

    /// Make sure the wrist card's preview has a texture in `ctx` (the watch).
    pub fn wrist_textures(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.pending.get_mut(self.pending_idx) {
            if p.thumb.is_none() {
                if let Some(img) = p.thumb_img.take() {
                    p.thumb = Some(ctx.load_texture("wrist-thumb", img, egui::TextureOptions::LINEAR));
                }
            }
        }
    }

    // --- Gallery -----------------------------------------------------------------

    pub fn gallery_active(&self) -> bool {
        self.gallery.active
    }

    pub fn gallery_total(&self) -> usize {
        self.gallery.paths.len()
    }

    pub fn gallery_pages(&self) -> usize {
        self.gallery.paths.len().div_ceil(GALLERY_PER).max(1)
    }

    pub fn gallery_open(&mut self) {
        if !self.gallery.active {
            self.gallery.active = true;
            self.gallery_rescan();
        }
    }

    pub fn gallery_close(&mut self) {
        self.gallery.active = false;
        self.gallery.items.clear();
        self.gallery.pending.clear();
    }

    fn gallery_rescan(&mut self) {
        self.gallery.paths = shots::scan_all(&self.dir).into_iter().map(|(p, _)| {
            let w = shots::shot_time(&p);
            (p, w)
        }).collect();
        self.gallery.page = self.gallery.page.min(self.gallery.paths.len().saturating_sub(1) / GALLERY_PER);
        self.gallery_load_page();
    }

    fn gallery_load_page(&mut self) {
        let start = (self.gallery.page * GALLERY_PER).min(self.gallery.paths.len());
        let end = (start + GALLERY_PER).min(self.gallery.paths.len());
        let slice: Vec<(PathBuf, String)> = self.gallery.paths[start..end].to_vec();
        self.gallery.items.clear();
        self.gallery.loading = !slice.is_empty();
        self.gallery.gen += 1;
        let (gen, tx) = (self.gallery.gen, self.gallery_tx.clone());
        std::thread::spawn(move || {
            let mut out = Vec::new();
            for (path, when) in slice {
                match shots::load_thumb_image(&path, 256) {
                    Ok(color) => out.push((color, when)),
                    Err(e) => log::warn!("photos: gallery thumb {path:?}: {e}"),
                }
            }
            let _ = tx.send((gen, out));
        });
    }

    pub fn gallery_apply(&mut self, req: GalleryRequests, hmd: Option<&xr::Posef>) {
        if req.refresh {
            self.gallery_rescan();
        }
        if req.prev && self.gallery.page > 0 {
            self.gallery.page -= 1;
            self.gallery_load_page();
        }
        if req.next && self.gallery.page + 1 < self.gallery_pages() {
            self.gallery.page += 1;
            self.gallery_load_page();
        }
        if let Some(k) = req.open {
            let idx = self.gallery.page * GALLERY_PER + k;
            if let Some((p, w)) = self.gallery.paths.get(idx).cloned() {
                self.open_photo(&p, &w, hmd);
            }
        }
        if let Some(k) = req.delete {
            let idx = self.gallery.page * GALLERY_PER + k;
            if let Some((p, _)) = self.gallery.paths.get(idx).cloned() {
                let _ = std::fs::remove_file(&p);
                log::info!("photos: deleted {}", p.display());
                self.pending.retain(|q| q.path != p);
                self.pending_idx = self.pending_idx.min(self.pending.len().saturating_sub(1));
                self.gallery_rescan();
            }
        }
    }

    // --- Wrist queue -----------------------------------------------------------------

    pub fn wrist_apply(&mut self, req: WristRequests, hmd: Option<&xr::Posef>) {
        if req.older && self.pending_idx + 1 < self.pending.len() {
            self.pending_idx += 1;
        }
        if req.newer && self.pending_idx > 0 {
            self.pending_idx -= 1;
        }
        if req.open {
            if let Some(p) = self.pending.get(self.pending_idx) {
                let (path, when, qr) = (p.path.clone(), p.when.clone(), p.qr.clone());
                match qr {
                    Some(content) => self.open_qr(&content, &when, hmd),
                    None => self.open_photo(&path, &when, hmd),
                }
                self.pending.remove(self.pending_idx);
                self.pending_idx = self.pending_idx.min(self.pending.len().saturating_sub(1));
            }
        }
        if req.dismiss && !self.pending.is_empty() {
            self.pending.remove(self.pending_idx);
            self.pending_idx = self.pending_idx.min(self.pending.len().saturating_sub(1));
        }
    }

    // --- Floating windows --------------------------------------------------------

    fn free_slot(&self) -> usize {
        self.slots.iter().position(|s| !s.open).unwrap_or(0)
    }

    fn reset_slot(s: &mut PhotoSlot, when: &str, hmd: Option<&xr::Posef>, slot: usize) {
        s.photo = None;
        s.path = None;
        s.text = None;
        s.show_text = false;
        s.loading = false;
        s.translating = false;
        s.sharing = false;
        s.share_msg = None;
        s.grab = None;
        s.when = when.to_string();
        s.open = true;
        if let Some(h) = hmd {
            s.gfx.pose = front_pose(h, 0.95, (slot as f32 - 1.0) * 0.36, 0.0, false);
        }
    }

    pub fn open_photo(&mut self, path: &Path, when: &str, hmd: Option<&xr::Posef>) {
        let slot = self.free_slot();
        let s = &mut self.slots[slot];
        Self::reset_slot(s, when, hmd, slot);
        s.path = Some(path.to_path_buf());
        s.loading = true;
        let (tx, p) = (self.photo_tx.clone(), path.to_path_buf());
        std::thread::spawn(move || match shots::load_image(&p) {
            Ok(color) => {
                let _ = tx.send((slot, p, color));
            }
            Err(e) => log::warn!("photos: load {p:?}: {e}"),
        });
        log::info!("photos: opening {} in slot {slot}", path.display());
    }

    fn open_qr(&mut self, content: &str, when: &str, hmd: Option<&xr::Posef>) {
        if content.starts_with("http://") || content.starts_with("https://") {
            match std::process::Command::new("xdg-open").arg(content).spawn() {
                Ok(_) => log::info!("photos: opened {content}"),
                Err(e) => log::warn!("photos: xdg-open {content}: {e}"),
            }
        } else {
            let slot = self.free_slot();
            let s = &mut self.slots[slot];
            Self::reset_slot(s, when, hmd, slot);
            s.text = Some(content.to_string());
            s.show_text = true;
        }
    }

    fn close_slot(s: &mut PhotoSlot) {
        s.open = false;
        s.photo = None;
        s.path = None;
        s.text = None;
        s.show_text = false;
        s.loading = false;
        s.translating = false;
        s.sharing = false;
        s.share_msg = None;
        s.grab = None;
        s.when.clear();
    }

    /// Laser interaction with the open windows: hover/click pointer per slot,
    /// grip to move. `max_t`: distance of a closer hit elsewhere.
    pub fn update_input(&mut self, hands: &[crate::desktop::HandInput], max_t: Option<f32>) -> PhotoInput {
        let mut out = PhotoInput { ray: None, ptr: [None; SLOTS], hit_t: None };
        for s in &mut self.slots {
            if let Some((hand, offset)) = s.grab {
                match hands.get(hand) {
                    Some(h) if h.active && h.grip >= GRAB_RELEASE => s.gfx.pose = pose_compose(&h.aim, &offset),
                    _ => s.grab = None,
                }
            }
        }
        if self.slots.iter().any(|s| s.grab.is_some()) {
            return out;
        }
        let mut best: Option<(usize, usize, f32, f32, f32)> = None;
        for (hi, h) in hands.iter().enumerate().filter(|(_, h)| h.active) {
            for (si, s) in self.slots.iter().enumerate().filter(|(_, s)| s.open) {
                if let Some((u, v, t)) = raycast(&h.aim, &s.gfx.pose, s.gfx.size_m) {
                    if max_t.is_some_and(|m| t >= m) {
                        continue;
                    }
                    if best.map_or(true, |b| t < b.4) {
                        best = Some((si, hi, u, v, t));
                    }
                }
            }
        }
        if let Some((si, hi, u, v, t)) = best {
            let h = &hands[hi];
            if h.grip > GRAB_START {
                self.slots[si].grab = Some((hi, pose_compose(&pose_invert(&h.aim), &self.slots[si].gfx.pose)));
                return out;
            }
            out.ptr[si] = Some((u, v, h.select));
            out.ray = Some((h.aim, t));
            out.hit_t = Some(t);
        }
        out
    }

    /// Render every open window and apply its buttons.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &ash::Device,
        render_pass: vk::RenderPass,
        cmd: vk::CommandBuffer,
        cmd_pool: vk::CommandPool,
        queue: vk::Queue,
        fence: vk::Fence,
        elapsed: f64,
        ptr: &[Option<(f32, f32, bool)>; SLOTS],
    ) -> Result<()> {
        let (translate_ok, share_ok) = (self.translate_ok, self.share_ok);
        for i in 0..self.slots.len() {
            if !self.slots[i].open {
                continue;
            }
            let mut action = PhotoAction::None;
            {
                let s = &mut self.slots[i];
                let view = PhotoView {
                    tex: s.photo.clone(),
                    text: s.text.clone(),
                    show_text: s.show_text,
                    loading: s.loading,
                    translating: s.translating,
                    sharing: s.sharing,
                    share_msg: s.share_msg.clone(),
                    when: s.when.clone(),
                    translate_ok,
                    share_ok,
                };
                render_panel(&mut s.gfx, device, render_pass, cmd, cmd_pool, queue, fence, true, ptr[i], (0.0, 0.0), elapsed, |ctx| {
                    build_photo(ctx, &view, &mut action)
                })?;
            }
            match action {
                PhotoAction::Translate => {
                    if let Some(p) = self.slots[i].path.clone() {
                        self.slots[i].translating = true;
                        let tx = self.async_tx.clone();
                        std::thread::spawn(move || {
                            let res = shots::translate::translate_image(&p);
                            let _ = tx.send((false, (i, p, res)));
                        });
                    }
                }
                PhotoAction::Share => {
                    if let Some(p) = self.slots[i].path.clone() {
                        self.slots[i].sharing = true;
                        self.slots[i].share_msg = None;
                        let tx = self.async_tx.clone();
                        std::thread::spawn(move || {
                            let res = shots::picsur::upload(&p);
                            let _ = tx.send((true, (i, p, res)));
                        });
                    }
                }
                PhotoAction::ToggleView => self.slots[i].show_text = !self.slots[i].show_text,
                PhotoAction::Copy => {
                    if self.slots[i].show_text {
                        if let Some(t) = &self.slots[i].text {
                            shots::copy_text_to_clipboard(t);
                        }
                    } else if let Some(p) = &self.slots[i].path {
                        shots::copy_to_clipboard(&p.to_string_lossy());
                    }
                }
                PhotoAction::Delete => {
                    if let Some(p) = self.slots[i].path.clone() {
                        let _ = std::fs::remove_file(&p);
                        log::info!("photos: deleted {}", p.display());
                        self.pending.retain(|q| q.path != p);
                        self.pending_idx = self.pending_idx.min(self.pending.len().saturating_sub(1));
                        if self.gallery.active {
                            self.gallery_rescan();
                        }
                    }
                    Self::close_slot(&mut self.slots[i]);
                }
                PhotoAction::Dismiss => Self::close_slot(&mut self.slots[i]),
                PhotoAction::None => {}
            }
        }
        Ok(())
    }

    pub fn layers<'a>(&'a self, space: &'a xr::Space) -> Vec<xr::CompositionLayerQuad<'a, xr::Vulkan>> {
        self.slots.iter().filter(|s| s.open).map(|s| quad_layer(&s.gfx, space, true)).collect()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

// --- egui ---------------------------------------------------------------------

struct PhotoView {
    tex: Option<egui::TextureHandle>,
    text: Option<String>,
    show_text: bool,
    loading: bool,
    translating: bool,
    sharing: bool,
    share_msg: Option<String>,
    when: String,
    translate_ok: bool,
    share_ok: bool,
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(18, 22, 28, 240))
        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(40, 110, 120)))
        .corner_radius(16)
        .inner_margin(egui::Margin::same(14))
        // Keep the stroke inside the panel (it was clipped at the bottom edge).
        .outer_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            add(ui);
        });
}

fn paint_corner_brackets(painter: &egui::Painter, r: egui::Rect, len: f32, stroke: egui::Stroke) {
    let c = [
        (r.left_top(), egui::vec2(1.0, 0.0), egui::vec2(0.0, 1.0)),
        (r.right_top(), egui::vec2(-1.0, 0.0), egui::vec2(0.0, 1.0)),
        (r.left_bottom(), egui::vec2(1.0, 0.0), egui::vec2(0.0, -1.0)),
        (r.right_bottom(), egui::vec2(-1.0, 0.0), egui::vec2(0.0, -1.0)),
    ];
    for (p, dx, dy) in c {
        painter.line_segment([p, p + dx * len], stroke);
        painter.line_segment([p, p + dy * len], stroke);
    }
}

fn build_photo(ctx: &egui::Context, v: &PhotoView, action: &mut PhotoAction) {
    let has_img = v.tex.is_some();
    let has_text = v.text.is_some();
    let busy = v.translating || v.sharing || v.loading;
    let showing_text = !busy && has_text && (v.show_text || !has_img);
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        card(ui, |ui| {
            let title = if showing_text && has_img {
                format!("{}  Translation", icon::TRANSLATE)
            } else if showing_text {
                format!("{}  Content", icon::QR_CODE)
            } else {
                format!("{}  Screenshot", icon::CAMERA)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).size(20.0).strong().color(egui::Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&v.when).color(theme::ON_SURFACE_VAR));
                });
            });
            ui.add_space(6.0);
            let footer_h = if v.share_msg.is_some() { 76.0 } else { 52.0 };
            let body_h = (ui.available_height() - footer_h).max(40.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), body_h), |ui| {
                if busy {
                    let label = if v.translating {
                        "  Translating…"
                    } else if v.sharing {
                        "  Uploading…"
                    } else {
                        "  Loading…"
                    };
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Spinner::new().size(36.0));
                        ui.label(egui::RichText::new(label).color(theme::ON_SURFACE_VAR));
                    });
                } else if showing_text {
                    let txt = v.text.as_deref().unwrap_or("");
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(txt).size(22.0).color(egui::Color32::WHITE)).wrap());
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        if let Some(t) = &v.tex {
                            let resp = ui.add(egui::Image::new(t).max_size(ui.available_size() * 0.96).corner_radius(8));
                            paint_corner_brackets(ui.painter(), resp.rect.expand(8.0), 24.0, egui::Stroke::new(2.5, theme::PRIMARY));
                        } else {
                            ui.label(egui::RichText::new("No content").color(theme::ON_SURFACE_VAR));
                        }
                    });
                }
            });
            if let Some(msg) = &v.share_msg {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(msg).size(14.0).color(theme::PRIMARY));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(format!("{}  Copy", icon::COPY)).clicked() {
                    *action = PhotoAction::Copy;
                }
                if has_img && ui.button(format!("{}  Delete", icon::TRASH)).clicked() {
                    *action = PhotoAction::Delete;
                }
                if has_img && !busy {
                    if has_text {
                        let label = if showing_text { format!("{}  Image", icon::IMAGE) } else { format!("{}  Text", icon::TEXT_T) };
                        if ui.button(label).clicked() {
                            *action = PhotoAction::ToggleView;
                        }
                    } else if v.translate_ok && ui.button(format!("{}  Translate", icon::TRANSLATE)).clicked() {
                        *action = PhotoAction::Translate;
                    }
                    if v.share_ok && ui.button(egui::RichText::new(icon::SHARE_FAT).size(18.0)).on_hover_text("Share").clicked() {
                        *action = PhotoAction::Share;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("{}  Close", icon::X)).clicked() {
                        *action = PhotoAction::Dismiss;
                    }
                });
            });
        });
    });
}

/// The wrist card, drawn inside the watch's clock area when a shot is queued.
pub fn wrist_card(
    ui: &mut egui::Ui,
    thumb: Option<&egui::TextureHandle>,
    qr: Option<&str>,
    when: &str,
    idx: usize,
    total: usize,
) -> WristRequests {
    // Full-width card: bigger preview + room for the text beside it.
    let mut req = WristRequests::default();
    ui.horizontal(|ui| {
        let title = if qr.is_some() { "QR code" } else { "New screenshot" };
        let glyph = if qr.is_some() { icon::QR_CODE } else { icon::CAMERA };
        ui.label(egui::RichText::new(format!("{glyph}  {title}")).size(14.0).strong().color(egui::Color32::WHITE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(egui::RichText::new(icon::X).size(13.0)).min_size(egui::vec2(26.0, 22.0))).on_hover_text("Dismiss").clicked() {
                req.dismiss = true;
            }
            if total > 1 {
                ui.label(egui::RichText::new(format!("{} / {total}", idx + 1)).size(12.0).color(theme::ON_SURFACE_VAR));
            }
        });
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let arrow = egui::vec2(26.0, 76.0);
        if ui.add_enabled(idx + 1 < total, egui::Button::new(egui::RichText::new(icon::CARET_LEFT).size(18.0)).min_size(arrow)).clicked() {
            req.older = true;
        }
        let size = egui::vec2(120.0, 76.0);
        match (qr, thumb) {
            (Some(content), _) => {
                let ic = egui::RichText::new(icon::QR_CODE).size(36.0).color(egui::Color32::BLACK);
                if ui.add_sized(size, egui::Button::new(ic).fill(theme::PRIMARY)).on_hover_text("Open").clicked() {
                    req.open = true;
                }
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 44.0).max(40.0));
                    ui.label(egui::RichText::new(truncate(content, 80)).size(12.0).color(theme::ON_SURFACE_VAR));
                    ui.label(egui::RichText::new("Tap the code to open").size(11.0).color(theme::ON_SURFACE_VAR));
                });
            }
            (None, Some(t)) => {
                let img = egui::Image::new(t).fit_to_exact_size(size).corner_radius(8);
                if ui.add(egui::ImageButton::new(img).frame(false)).on_hover_text("Open").clicked() {
                    req.open = true;
                }
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 44.0).max(40.0));
                    ui.label(egui::RichText::new(when).size(13.0).color(theme::ON_SURFACE_VAR));
                    ui.label(egui::RichText::new("Tap the preview to open").size(11.0).color(theme::ON_SURFACE_VAR));
                });
            }
            (None, None) => {
                ui.allocate_space(size);
            }
        }
        if ui.add_enabled(idx > 0, egui::Button::new(egui::RichText::new(icon::CARET_RIGHT).size(18.0)).min_size(arrow)).clicked() {
            req.newer = true;
        }
    });
    req
}

/// The gallery grid for the dashboard's Photos page.
pub fn gallery_ui(
    ui: &mut egui::Ui,
    items: &[(egui::TextureHandle, String)],
    page: usize,
    pages: usize,
    total: usize,
    loading: bool,
) -> GalleryRequests {
    let mut req = GalleryRequests::default();
    const COLS: usize = 4;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{total} screenshot(s)")).color(theme::ON_SURFACE_VAR));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{}  Refresh", icon::ARROWS_CLOCKWISE)).clicked() {
                req.refresh = true;
            }
            let next = egui::Button::new(egui::RichText::new(icon::CARET_RIGHT).size(18.0));
            if ui.add_enabled(page + 1 < pages, next).clicked() {
                req.next = true;
            }
            ui.label(egui::RichText::new(format!("Page {} / {pages}", page + 1)).color(theme::ON_SURFACE_VAR));
            let prev = egui::Button::new(egui::RichText::new(icon::CARET_LEFT).size(18.0));
            if ui.add_enabled(page > 0, prev).clicked() {
                req.prev = true;
            }
        });
    });
    ui.add_space(8.0);
    if total == 0 {
        ui.label(egui::RichText::new("No screenshots yet — make the finger-frame gesture in a game.").color(theme::ON_SURFACE_VAR));
        return req;
    }
    if loading {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(36.0));
        });
        ui.add_space(40.0);
        return req;
    }
    egui::Grid::new("gallery_grid").spacing(egui::vec2(14.0, 14.0)).show(ui, |ui| {
        for (k, (tex, when)) in items.iter().enumerate() {
            ui.vertical(|ui| {
                let img = egui::Image::new(tex).fit_to_exact_size(egui::vec2(280.0, 175.0)).corner_radius(8);
                if ui.add(egui::ImageButton::new(img).frame(false)).clicked() {
                    req.open = Some(k);
                }
                ui.horizontal(|ui| {
                    ui.small(egui::RichText::new(when).color(theme::ON_SURFACE_VAR));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(egui::RichText::new(icon::TRASH).color(theme::ON_SURFACE_VAR)).on_hover_text("Delete").clicked() {
                            req.delete = Some(k);
                        }
                    });
                });
            });
            if (k + 1) % COLS == 0 {
                ui.end_row();
            }
        }
    });
    req
}
