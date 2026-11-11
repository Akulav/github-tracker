//! App state, event handling, window chrome (title bar, sidebar), navigation and dialogs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Id, Key, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle};
use egui_phosphor::regular as ph;

use super::theme::*;
use super::widgets::*;
use crate::backend::auth::{self, DeviceCode};
use crate::backend::cache::Cache;
use crate::backend::demo;
use crate::backend::github::{Api, Details, History, Repo, User};
use crate::backend::install::{self, Offer, StartupInfo};
use crate::backend::jobs::{self, Bus, Event};
use crate::backend::settings::Settings;
use crate::backend::stats::{self, Freshness};
use crate::backend::update::{self, UpdateInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Repos,
    Maintenance,
    Repo(u64),
    Settings,
}

pub struct Login {
    pub pat: String,
    pub error: Option<String>,
    pub busy: bool,
    pub device: Option<DeviceCode>,
    pub device_started: Option<Instant>,
    pub cancel: Arc<AtomicBool>,
    /// `Some(token)` when a signed-in GitHub CLI was found.
    pub gh_token: Option<String>,
    pub copied: Option<Instant>,
}

impl Default for Login {
    fn default() -> Self {
        Self { pat: String::new(), error: None, busy: false, device: None, device_started: None, cancel: Arc::new(AtomicBool::new(false)), gh_token: None, copied: None }
    }
}

pub struct Session {
    pub api: Arc<Api>,
    pub user: User,
    pub id: u64,
}

pub enum Auth {
    SignedOut(Login),
    SignedIn(Session),
}

#[derive(Default)]
pub struct SyncState {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub error: Option<String>,
    pub errors: Vec<String>,
    pub cancel: Arc<AtomicBool>,
}

/// Lifecycle of a self-update.
#[derive(Debug, Clone)]
pub enum UpdateState {
    None,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading { info: UpdateInfo, done: u64, total: u64 },
    Ready { info: UpdateInfo, path: PathBuf },
    Failed { info: Option<UpdateInfo>, error: String },
}

impl UpdateState {
    pub fn info(&self) -> Option<&UpdateInfo> {
        match self {
            UpdateState::Available(i) | UpdateState::Downloading { info: i, .. } | UpdateState::Ready { info: i, .. } => Some(i),
            UpdateState::Failed { info, .. } => info.as_ref(),
            _ => None,
        }
    }
}

pub enum Loadable<T> {
    Loading,
    Ready(T),
    Failed(String),
}

enum AvatarSlot {
    Loading,
    Ready(TextureHandle),
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

struct Toast {
    kind: ToastKind,
    title: String,
    body: String,
    created: Instant,
}

/// Filter pills on the repositories page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Owned,
    Private,
    Public,
    Forks,
    Archived,
    Attention,
}

impl Filter {
    pub const ALL: [Filter; 7] = [Filter::All, Filter::Owned, Filter::Private, Filter::Public, Filter::Forks, Filter::Archived, Filter::Attention];
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Owned => "Mine",
            Filter::Private => "Private",
            Filter::Public => "Public",
            Filter::Forks => "Forks",
            Filter::Archived => "Archived",
            Filter::Attention => "Needs attention",
        }
    }
}

pub struct ListState {
    pub search: String,
    pub filter: Filter,
    pub selected: usize,
    pub focus_search: bool,
    pub scroll_to_selected: bool,
    /// Last scroll offset of the list.
    pub offset: f32,
}

pub enum Confirm {
    SignOut,
    Uninstall { delete_data: bool },
}

pub struct App {
    pub rt: tokio::runtime::Runtime,
    pub bus: Bus,
    rx: Receiver<Event>,
    pub http: reqwest::Client,
    pub root: PathBuf,
    pub settings: Settings,
    settings_dirty: Option<Instant>,
    pub skip_save: bool,

    pub icon: Option<TextureHandle>,
    pub install_offer: Offer,
    pub install_dir_edit: String,
    pub install_error: Option<String>,

    pub auth: Auth,
    next_session: u64,
    pub repos: Vec<Repo>,
    pub details: HashMap<u64, Details>,
    pub synced_at: Option<DateTime<Utc>>,
    pub sync: SyncState,
    /// Per-commit history of repositories opened this session.
    pub commit_history: HashMap<u64, Loadable<History>>,
    avatars: HashMap<String, AvatarSlot>,

    pub page: Page,
    page_since: Instant,
    history: Vec<Page>,
    pub list: ListState,

    pub update: UpdateState,
    pub show_update_dialog: bool,
    last_update_check: Instant,
    toasts: Vec<Toast>,
    pub confirm: Option<Confirm>,
    shot_taken: Option<usize>,
}

pub const SIDEBAR_W: f32 = 224.0;

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: StartupInfo) -> Self {
        install_fonts(&cc.egui_ctx);
        apply_style(&cc.egui_ctx);

        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("tokio runtime");
        let (tx, rx) = channel();
        let bus = Bus::new(tx, cc.egui_ctx.clone());
        let http = crate::backend::net::client();
        let root = install::data_root();
        let settings = Settings::load(&root);
        let icon = load_texture(&cc.egui_ctx, "app_icon", include_bytes!("../../assets/icon.png"));
        let install_dir_edit = match &startup.offer {
            Offer::Install { suggested } => suggested.display().to_string(),
            _ => String::new(),
        };

        let mut app = Self {
            rt,
            bus,
            rx,
            http,
            root,
            settings,
            settings_dirty: None,
            skip_save: false,
            icon,
            install_offer: startup.offer.clone(),
            install_dir_edit,
            install_error: None,
            auth: Auth::SignedOut(Login::default()),
            next_session: 1,
            repos: Vec::new(),
            details: HashMap::new(),
            synced_at: None,
            sync: SyncState::default(),
            commit_history: HashMap::new(),
            avatars: HashMap::new(),
            page: Page::Overview,
            page_since: Instant::now(),
            history: Vec::new(),
            list: ListState { search: String::new(), filter: Filter::All, selected: 0, focus_search: false, scroll_to_selected: false, offset: 0.0 },
            update: UpdateState::None,
            show_update_dialog: false,
            last_update_check: Instant::now(),
            toasts: Vec::new(),
            confirm: None,
            shot_taken: None,
        };
        if let Some(from) = &startup.updated_from {
            app.toast(ToastKind::Success, format!("Updated to {}", update::CURRENT_VERSION), format!("from version {from}. Enjoy!"));
        } else if startup.installed {
            app.toast(ToastKind::Success, "Installed", "Akulav Tracker is ready. Sign in to see your repositories.");
        }
        if app.install_offer == Offer::None {
            app.start_normally();
        }
        app
    }

    /// After the (Linux/macOS) install screen, or right away.
    fn start_normally(&mut self) {
        if self.settings.auto_check_updates {
            self.check_updates(false);
        }
        if demo::enabled() {
            let cache = demo::cache(Utc::now());
            let user = cache.user.clone();
            self.apply_cache(cache);
            self.enter_session("demo".into(), user, false);
            if std::env::var_os("AKULAV_SHOT_DIR").is_none() {
                self.toast(ToastKind::Info, "Demo mode", "Made-up repositories; nothing is read from GitHub.");
            }
            return;
        }
        match auth::store::load(&self.root) {
            Some(token) => match Cache::load(&self.root, None) {
                // Show the last sync at once; the sync that follows also checks the token.
                Some(cache) => {
                    let user = cache.user.clone();
                    self.apply_cache(cache);
                    self.enter_session(token, user, self.settings.sync_on_start);
                }
                None => {
                    let mut login = Login { busy: true, ..Default::default() };
                    login.error = None;
                    self.auth = Auth::SignedOut(login);
                    jobs::sign_in_with_token(self.rt.handle(), self.bus.clone(), self.http.clone(), token);
                }
            },
            None => self.probe_gh_cli(),
        }
    }

    pub fn probe_gh_cli(&mut self) {
        let bus = self.bus.clone();
        self.rt.spawn_blocking(move || {
            if let Some(t) = auth::gh_cli_token() {
                bus.send(Event::GhCliToken(t));
            }
        });
    }

    fn apply_cache(&mut self, cache: Cache) {
        self.repos = cache.repos;
        self.details = cache.details;
        self.synced_at = cache.synced_at;
    }

    fn enter_session(&mut self, token: String, user: User, sync: bool) {
        let id = self.next_session;
        self.next_session += 1;
        let api = Arc::new(Api::new(self.http.clone(), token));
        self.auth = Auth::SignedIn(Session { api, user, id });
        self.page = Page::Overview;
        self.history.clear();
        if sync || self.repos.is_empty() {
            self.start_sync(false);
        }
    }

    pub fn session(&self) -> Option<&Session> {
        match &self.auth {
            Auth::SignedIn(s) => Some(s),
            _ => None,
        }
    }

    pub fn login_mut(&mut self) -> Option<&mut Login> {
        match &mut self.auth {
            Auth::SignedOut(l) => Some(l),
            _ => None,
        }
    }

    pub fn start_sync(&mut self, force: bool) {
        let Some(s) = self.session() else { return };
        if self.sync.running || demo::enabled() {
            return;
        }
        let (api, id) = (s.api.clone(), s.id);
        self.sync = SyncState { running: true, cancel: Arc::new(AtomicBool::new(false)), ..Default::default() };
        jobs::sync(self.rt.handle(), self.bus.clone(), api, id, self.details.clone(), force, self.sync.cancel.clone());
    }

    pub fn sign_out(&mut self) {
        self.sync.cancel.store(true, Ordering::Relaxed);
        self.sync = SyncState::default();
        if !demo::enabled() {
            auth::store::clear(&self.root);
            Cache::clear(&self.root);
        }
        self.repos.clear();
        self.details.clear();
        self.commit_history.clear();
        self.synced_at = None;
        self.auth = Auth::SignedOut(Login::default());
        self.page = Page::Overview;
        self.history.clear();
        self.probe_gh_cli();
        self.toast(ToastKind::Info, "Signed out", "Your token and cached data were removed from this computer.");
    }

    fn save_cache(&self) {
        let Some(s) = self.session() else { return };
        if demo::enabled() {
            return;
        }
        let cache = Cache { format: 0, user: s.user.clone(), repos: self.repos.clone(), details: self.details.clone(), synced_at: self.synced_at };
        if let Err(e) = cache.save(&self.root) {
            eprintln!("could not save the cache: {e}");
        }
    }

    pub fn mark_dirty(&mut self) {
        self.settings_dirty = Some(Instant::now());
    }

    pub fn save_settings_now(&mut self) {
        self.settings_dirty = None;
        if let Err(e) = self.settings.save(&self.root) {
            eprintln!("could not save settings: {e}");
        }
    }

    pub fn check_updates(&mut self, manual: bool) {
        if matches!(self.update, UpdateState::Downloading { .. } | UpdateState::Ready { .. } | UpdateState::Checking) {
            return;
        }
        if manual {
            self.update = UpdateState::Checking;
        }
        self.last_update_check = Instant::now();
        jobs::check_update(self.rt.handle(), self.bus.clone(), self.http.clone(), manual);
    }

    pub fn start_update_download(&mut self, info: UpdateInfo) {
        self.update = UpdateState::Downloading { info: info.clone(), done: 0, total: 0 };
        jobs::download_update(self.rt.handle(), self.bus.clone(), self.http.clone(), info);
    }

    pub fn toast(&mut self, kind: ToastKind, title: impl Into<String>, body: impl Into<String>) {
        self.toasts.push(Toast { kind, title: title.into(), body: body.into(), created: Instant::now() });
    }

    // -----------------------------------------------------------------------------------------
    // Data helpers
    // -----------------------------------------------------------------------------------------

    pub fn login(&self) -> &str {
        self.session().map(|s| s.user.login.as_str()).unwrap_or("")
    }

    pub fn is_owned(&self, r: &Repo) -> bool {
        r.owner.eq_ignore_ascii_case(self.login())
    }

    /// Repositories counted in the overview and maintenance pages (per the settings).
    pub fn counted(&self) -> Vec<&Repo> {
        self.repos
            .iter()
            .filter(|r| self.settings.include_forks || !r.fork)
            .filter(|r| self.settings.include_archived || !r.archived)
            .filter(|r| !self.settings.only_owned || self.is_owned(r))
            .collect()
    }

    pub fn repo(&self, id: u64) -> Option<&Repo> {
        self.repos.iter().find(|r| r.id == id)
    }

    /// Avatar texture for `url`, fetched on first use.
    pub fn avatar(&mut self, url: &str) -> Option<TextureHandle> {
        if url.is_empty() {
            return None;
        }
        match self.avatars.get(url) {
            Some(AvatarSlot::Ready(t)) => Some(t.clone()),
            Some(_) => None,
            None => {
                self.avatars.insert(url.to_string(), AvatarSlot::Loading);
                jobs::avatar(self.rt.handle(), self.bus.clone(), self.http.clone(), url.to_string(), self.root.join("avatars"));
                None
            }
        }
    }

    pub fn open_repo(&mut self, id: u64) {
        self.go(Page::Repo(id));
        let pushed = self.repo(id).and_then(|r| r.pushed_at);
        let fresh = match self.commit_history.get(&id) {
            Some(Loadable::Loading) => true,
            Some(Loadable::Ready(h)) => h.seen_push == pushed,
            _ => false,
        };
        if !fresh {
            self.load_history(id);
        }
    }

    pub fn load_history(&mut self, id: u64) {
        let (Some(s), Some(r)) = (self.session(), self.repo(id)) else { return };
        let (api, session, r) = (s.api.clone(), s.id, r.clone());
        if demo::enabled() {
            self.commit_history.insert(id, Loadable::Ready(demo::history(&r)));
            return;
        }
        self.commit_history.insert(id, Loadable::Loading);
        jobs::history(self.rt.handle(), self.bus.clone(), api, session, r);
    }

    pub fn go(&mut self, page: Page) {
        if self.page != page {
            self.history.push(self.page);
            if self.history.len() > 50 {
                self.history.remove(0);
            }
            self.page = page;
            self.page_since = Instant::now();
        }
    }

    pub fn back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.page = p;
            self.page_since = Instant::now();
        } else if matches!(self.page, Page::Repo(_)) {
            self.page = Page::Repos;
            self.page_since = Instant::now();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------------------------

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            let current = self.session().map(|s| s.id);
            match ev {
                Event::GhCliToken(t) => {
                    if let Some(l) = self.login_mut() {
                        l.gh_token = Some(t);
                    }
                }
                Event::DeviceCode(dc) => {
                    if let Some(l) = self.login_mut() {
                        l.device = Some(dc);
                        l.device_started = Some(Instant::now());
                    }
                }
                Event::SignedIn { token, user } => {
                    if self.session().is_some() {
                        continue;
                    }
                    if let Err(e) = auth::store::save(&self.root, &token) {
                        self.toast(ToastKind::Warning, "Couldn't save your sign-in", format!("You'll need to sign in again next time ({e:#})."));
                    }
                    match Cache::load(&self.root, Some(&user.login)) {
                        Some(c) => self.apply_cache(c),
                        None => {
                            self.repos.clear();
                            self.details.clear();
                            self.synced_at = None;
                        }
                    }
                    let name = user.name.clone().unwrap_or_else(|| user.login.clone());
                    self.enter_session(token, user, true);
                    self.toast(ToastKind::Success, format!("Welcome, {name}"), "Fetching your repositories…");
                }
                Event::SignInFailed(e) => {
                    if let Some(l) = self.login_mut() {
                        l.busy = false;
                        l.device = None;
                        l.error = Some(e);
                    }
                }
                Event::Repos { session, repos } if Some(session) == current => {
                    let ids: std::collections::HashSet<u64> = repos.iter().map(|r| r.id).collect();
                    self.details.retain(|id, _| ids.contains(id));
                    self.repos = repos;
                }
                Event::Details { session, id, details } if Some(session) == current => {
                    self.details.insert(id, details);
                }
                Event::SyncProgress { session, done, total } if Some(session) == current => {
                    self.sync.done = done;
                    self.sync.total = total;
                }
                Event::SyncDone { session, errors } if Some(session) == current => {
                    self.sync.running = false;
                    self.synced_at = Some(Utc::now());
                    if !errors.is_empty() {
                        self.toast(ToastKind::Warning, format!("{} repositories could not be read", errors.len()), errors[0].clone());
                    }
                    self.sync.errors = errors;
                    self.save_cache();
                }
                Event::SyncFailed { session, error } if Some(session) == current => {
                    self.sync.running = false;
                    // Keep what was read before the failure (the sync time stays as it was).
                    if !error.contains("401") {
                        self.save_cache();
                    }
                    if error.contains("401") {
                        self.sign_out();
                        if let Some(l) = self.login_mut() {
                            l.error = Some("Your GitHub sign-in expired or was revoked. Sign in again.".into());
                        }
                    } else {
                        self.toast(ToastKind::Error, "Sync failed", error.clone());
                        self.sync.error = Some(error);
                    }
                }
                Event::History { session, id, result } if Some(session) == current => {
                    self.commit_history.insert(
                        id,
                        match result {
                            Ok(c) => Loadable::Ready(c),
                            Err(e) => Loadable::Failed(e),
                        },
                    );
                }
                Event::Avatar { url, bytes } => {
                    let slot = bytes.and_then(|b| load_texture(ctx, &url, &b)).map(AvatarSlot::Ready).unwrap_or(AvatarSlot::Failed);
                    self.avatars.insert(url, slot);
                }
                Event::UpdateChecked { manual, result } => match result {
                    Ok(Some(info)) => {
                        if !matches!(&self.update, UpdateState::Available(i) if i.version == info.version) {
                            self.toast(ToastKind::Info, format!("Version {} is available", info.version), "Update in one click from the title bar.");
                        }
                        self.update = UpdateState::Available(info);
                        if manual {
                            self.show_update_dialog = true;
                        }
                    }
                    Ok(None) => {
                        if manual || matches!(self.update, UpdateState::Checking) {
                            self.update = UpdateState::UpToDate;
                        }
                    }
                    Err(e) => {
                        if manual {
                            self.update = UpdateState::Failed { info: None, error: e };
                        }
                    }
                },
                Event::UpdateProgress { done, total } => {
                    if let UpdateState::Downloading { done: d, total: t, .. } = &mut self.update {
                        *d = done;
                        *t = total;
                    }
                }
                Event::UpdateReady(path) => {
                    if let Some(info) = self.update.info().cloned() {
                        self.update = UpdateState::Ready { info, path };
                        self.show_update_dialog = true;
                    }
                }
                Event::UpdateFailed(error) => {
                    let info = self.update.info().cloned();
                    self.update = UpdateState::Failed { info, error };
                }
                // Events from an older session (signed out meanwhile).
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Keyboard navigation
    // -----------------------------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.session().is_none() || self.confirm.is_some() || self.show_update_dialog {
            return;
        }
        let typing = ctx.wants_keyboard_input();
        let (cmd, alt, back_mouse) = ctx.input(|i| (i.modifiers.command, i.modifiers.alt, i.pointer.button_pressed(egui::PointerButton::Extra1)));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        if back_mouse || (alt && pressed(Key::ArrowLeft)) || (!typing && (pressed(Key::Backspace) || (pressed(Key::Escape) && matches!(self.page, Page::Repo(_))))) {
            self.back();
        }
        if cmd {
            for (k, p) in [(Key::Num1, Page::Overview), (Key::Num2, Page::Repos), (Key::Num3, Page::Maintenance), (Key::Num4, Page::Settings)] {
                if pressed(k) {
                    self.go(p);
                }
            }
            if pressed(Key::F) || pressed(Key::K) {
                self.go(Page::Repos);
                self.list.focus_search = true;
            }
            if pressed(Key::R) {
                self.start_sync(false);
            }
        }
        if pressed(Key::F5) {
            self.start_sync(ctx.input(|i| i.modifiers.shift));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Chrome
    // -----------------------------------------------------------------------------------------

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let drag = ui.interact(rect, Id::new("title_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }

        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            if let Some(icon) = &self.icon {
                ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(20.0, 20.0)));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("AKULAV").font(black(13.5)).color(TEXT).extra_letter_spacing(1.5));
            ui.label(RichText::new("TRACKER").font(semibold(13.0)).color(MUTED).extra_letter_spacing(1.5));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if window_button(ui, ph::X, true).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                if window_button(ui, if max { ph::CORNERS_IN } else { ph::CORNERS_OUT }, false).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                if window_button(ui, ph::MINUS, false).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                ui.add_space(10.0);
                let label = match &self.update {
                    UpdateState::Available(i) => Some((format!("Update {} available", i.version), GOLD)),
                    UpdateState::Downloading { done, total, .. } => Some((
                        if *total > 0 { format!("Downloading update {:.0}%", *done as f32 * 100.0 / *total as f32) } else { "Downloading update…".into() },
                        BLUE,
                    )),
                    UpdateState::Ready { .. } => Some(("Restart to update".into(), ACCENT_HI)),
                    UpdateState::Failed { info: Some(_), .. } => Some(("Update failed".into(), RED)),
                    _ => None,
                };
                if let Some((text, color)) = label {
                    let r = chip(ui, ph::ARROW_CIRCLE_UP, &text, color).interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        self.show_update_dialog = true;
                    }
                }
            });
        });
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, BORDER));
    }

    fn attention_count(&self) -> usize {
        let now = Utc::now();
        self.counted()
            .into_iter()
            .filter(|r| !r.archived && matches!(stats::freshness(r, self.details.get(&r.id), now), Freshness::Stale | Freshness::Dormant))
            .count()
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        let attention = self.attention_count();
        let current = match self.page {
            Page::Repo(_) => Page::Repos,
            p => p,
        };
        for (page, icon, label, badge) in [
            (Page::Overview, ph::SQUARES_FOUR, "Overview", None),
            (Page::Repos, ph::GIT_BRANCH, "Repositories", Some((self.repos.len().to_string(), SURFACE_3))),
            (Page::Maintenance, ph::HEARTBEAT, "Maintenance", (attention > 0).then(|| (attention.min(999).to_string(), with_alpha(freshness_color(Freshness::Stale), 200)))),
            (Page::Settings, ph::GEAR_SIX, "Settings", None),
        ] {
            if nav_item(ui, icon, label, current == page, badge).clicked() {
                self.go(page);
                if page == Page::Repos {
                    self.list.focus_search = false;
                }
            }
            ui.add_space(2.0);
        }

        // Sync status.
        ui.add_space(14.0);
        let r = ui.max_rect();
        ui.painter().hline(r.left() + 6.0..=r.right() - 6.0, ui.cursor().top(), Stroke::new(1.0_f32, BORDER));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.set_width(SIDEBAR_W - 80.0);
                if self.sync.running {
                    ui.label(RichText::new("Syncing…").font(semibold(13.0)).color(TEXT));
                    ui.add_space(-6.0);
                    let sub = if self.sync.total > 0 { format!("{} of {} repositories", self.sync.done, self.sync.total) } else { "Listing repositories".into() };
                    ui.label(RichText::new(sub).font(regular(11.5)).color(MUTED));
                } else {
                    let (t, c) = match (&self.sync.error, self.synced_at) {
                        (Some(_), _) => ("Sync failed".to_string(), RED),
                        (None, Some(t)) => (format!("Synced {}", stats::ago(t, Utc::now())), TEXT),
                        (None, None) => ("Not synced yet".to_string(), MUTED),
                    };
                    ui.label(RichText::new(t).font(semibold(13.0)).color(c));
                    ui.add_space(-6.0);
                    ui.label(RichText::new("F5 or Ctrl+R to refresh").font(regular(11.5)).color(FAINT));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(4.0);
                if self.sync.running {
                    spinner(ui, 22.0, ACCENT_HI);
                } else if icon_button(ui, ph::ARROWS_CLOCKWISE, "Sync now (Shift+click: refetch everything)").clicked() {
                    let force = ui.input(|i| i.modifiers.shift);
                    self.start_sync(force);
                }
            });
        });
        if self.sync.running {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_width(SIDEBAR_W - 36.0);
                    let frac = (self.sync.total > 0).then(|| self.sync.done as f32 / self.sync.total as f32);
                    progress_bar(ui, frac, 4.0);
                });
            });
        }

        // Profile card pinned to the bottom.
        let bottom = ui.max_rect().bottom();
        let card = Rect::from_min_max(pos2(ui.max_rect().left() + 2.0, bottom - 64.0 - 36.0), pos2(ui.max_rect().right() - 2.0, bottom - 36.0));
        let card_resp = ui.interact(card, Id::new("profile_card"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(card_resp.id, card_resp.hovered(), 0.1);
        ui.painter().rect_filled(card, CornerRadius::same(R_MD), lerp_color(SURFACE_2, SURFACE_3, hover));
        ui.painter().rect_stroke(card, CornerRadius::same(R_MD), Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
        let (login, name, avatar_url) = match self.session() {
            Some(s) => (s.user.login.clone(), s.user.name.clone().unwrap_or_else(|| s.user.login.clone()), s.user.avatar_url.clone()),
            None => (String::new(), String::new(), String::new()),
        };
        let tex = self.avatar(&avatar_url);
        let av = Rect::from_min_size(card.min + vec2(12.0, 12.0), vec2(40.0, 40.0));
        paint_avatar(ui.painter(), av, tex.as_ref(), &login);
        let p = ui.painter();
        p.text(pos2(av.right() + 10.0, av.top() + 11.0), Align2::LEFT_CENTER, super::charts::truncate(&name, 18), semibold(14.0), TEXT);
        p.text(pos2(av.right() + 10.0, av.bottom() - 9.0), Align2::LEFT_CENTER, format!("@{}", super::charts::truncate(&login, 18)), regular(12.0), MUTED);
        if card_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open your GitHub profile").clicked() {
            open_url(&format!("https://github.com/{login}"));
        }

        // Credit line.
        let credit = Rect::from_center_size(pos2(ui.max_rect().center().x, bottom - 16.0), vec2(ui.max_rect().width() - 12.0, 20.0));
        let credit_resp = ui.interact(credit, Id::new("sidebar_credit"), Sense::click());
        ui.painter().text(
            credit.center(),
            Align2::CENTER_CENTER,
            format!("v{}  ·  made by {} {}", update::CURRENT_VERSION, update::AUTHOR, ph::HEART),
            regular(11.0),
            if credit_resp.hovered() { ACCENT_HI } else { FAINT },
        );
        if credit_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Akulav Tracker on GitHub").clicked() {
            open_url(update::REPO_URL);
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        const LIFETIME: f32 = 6.0;
        self.toasts.retain(|t| t.created.elapsed().as_secs_f32() < LIFETIME);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.screen_rect();
        let mut y = screen.top() + 54.0;
        let mut remove = None;
        for (i, t) in self.toasts.iter().enumerate().rev().take(4) {
            let age = t.created.elapsed().as_secs_f32();
            let alpha = ((LIFETIME - age) / 0.4).clamp(0.0, 1.0) * (age / 0.15).clamp(0.0, 1.0);
            let (color, icon) = match t.kind {
                ToastKind::Info => (BLUE, ph::INFO),
                ToastKind::Success => (ACCENT_HI, ph::CHECK_CIRCLE),
                ToastKind::Warning => (GOLD, ph::WARNING),
                ToastKind::Error => (RED, ph::WARNING_OCTAGON),
            };
            let w = 340.0;
            let x = screen.right() - w - 16.0 + (1.0 - alpha) * 30.0;
            let area = egui::Area::new(Id::new(("toast", t.created))).order(egui::Order::Foreground).fixed_pos(pos2(x, y));
            let resp = area.show(ctx, |ui| {
                ui.set_opacity(alpha);
                Frame::new()
                    .fill(SURFACE_2)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .corner_radius(CornerRadius::same(R_MD))
                    .inner_margin(Margin::symmetric(14, 12))
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) })
                    .show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).font(regular(20.0)).color(color));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&t.title).font(semibold(13.5)).color(TEXT));
                                if !t.body.is_empty() {
                                    ui.add_space(-6.0);
                                    ui.label(RichText::new(&t.body).font(regular(12.5)).color(MUTED));
                                }
                            });
                        });
                    })
            });
            if resp.response.interact(Sense::click()).clicked() {
                remove = Some(i);
            }
            y += resp.response.rect.height() + 8.0;
        }
        if let Some(i) = remove {
            self.toasts.remove(i);
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (title, body, action) = match c {
            Confirm::SignOut => ("Sign out?", "Your token and the cached repository data are removed from this computer.".to_string(), "Sign out"),
            Confirm::Uninstall { .. } => (
                "Uninstall Akulav Tracker?",
                if install::UNINSTALL_ASKS_ABOUT_DATA {
                    "The uninstaller starts and this window closes. It asks whether to keep your cache and settings.".to_string()
                } else {
                    format!("Removes the app from {}.", install::installed_root().map(|r| r.display().to_string()).unwrap_or_default())
                },
                "Uninstall",
            ),
        };
        let mut close = false;
        let mut go = false;
        let modal = egui::Modal::new(Id::new("confirm")).frame(Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(CornerRadius::same(R_LG)).inner_margin(Margin::same(24)));
        let resp = modal.show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(RichText::new(title).font(bold(19.0)));
            ui.label(RichText::new(body).color(MUTED));
            if let Some(Confirm::Uninstall { delete_data }) = &mut self.confirm {
                if !install::UNINSTALL_ASKS_ABOUT_DATA {
                    ui.add_space(6.0);
                    ui.checkbox(delete_data, "Also delete the cache, settings and saved sign-in");
                }
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if danger_button(ui, action, vec2(120.0, 36.0), true).clicked() {
                        go = true;
                    }
                    if ghost_button(ui, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
        });
        if go {
            match self.confirm.take() {
                Some(Confirm::SignOut) => self.sign_out(),
                Some(Confirm::Uninstall { delete_data }) => {
                    if delete_data {
                        auth::store::clear(&self.root);
                    }
                    match install::uninstall(delete_data) {
                        Ok(()) => {
                            self.skip_save = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => self.toast(ToastKind::Error, "Uninstall failed", format!("{e:#}")),
                    }
                }
                None => {}
            }
        } else if close || resp.should_close() {
            self.confirm = None;
        }
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_update_dialog {
            return;
        }
        let state = self.update.clone();
        let Some(info) = state.info().cloned() else {
            self.show_update_dialog = false;
            return;
        };
        let mut close = false;
        let modal = egui::Modal::new(Id::new("update_dialog")).frame(Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(CornerRadius::same(R_LG)).inner_margin(Margin::same(26)));
        let resp = modal.show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(R_MD), with_alpha(ACCENT, 40));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, ph::ROCKET_LAUNCH, regular(24.0), ACCENT_HI);
                ui.vertical(|ui| {
                    kicker(ui, "Akulav Tracker update");
                    ui.label(RichText::new(format!("Version {}", info.version)).font(bold(20.0)));
                });
            });
            ui.add_space(6.0);
            let how = if cfg!(windows) {
                "It downloads in the background, installs into the same folder and restarts the app. Nothing to click through."
            } else {
                "It downloads in the background and replaces this app in place, then restarts it."
            };
            ui.label(RichText::new(format!("You have {}. {how}", update::CURRENT_VERSION)).color(MUTED));
            if !info.notes_url.is_empty() && ui.link(RichText::new(format!("{}  What's new", ph::ARROW_SQUARE_OUT)).font(semibold(13.0))).clicked() {
                open_url(&info.notes_url);
            }
            ui.add_space(12.0);
            match &state {
                UpdateState::Downloading { done, total, .. } => {
                    let frac = (*total > 0).then(|| *done as f32 / *total as f32);
                    ui.label(
                        RichText::new(match frac {
                            Some(f) => format!("Downloading… {:.0}%  ·  {} / {}", f * 100.0, stats::bytes(*done), stats::bytes(*total)),
                            None => format!("Downloading… {}", stats::bytes(*done)),
                        })
                        .font(semibold(13.5)),
                    );
                    progress_bar(ui, frac, 6.0);
                    ui.add_space(6.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ghost_button(ui, "Hide").clicked() {
                            close = true;
                        }
                    });
                }
                UpdateState::Ready { path, .. } => {
                    ui.label(RichText::new(format!("{}  Downloaded and verified (SHA-256)", ph::CHECK_CIRCLE)).font(semibold(14.0)).color(ACCENT_HI));
                    ui.add_space(8.0);
                    let path = path.clone();
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Restart & update", ph::ARROW_CLOCKWISE), vec2(190.0, 38.0), true).clicked() {
                                self.save_settings_now();
                                self.save_cache();
                                match install::apply_update(&path, &info.sha256) {
                                    Ok(()) => {
                                        self.skip_save = true;
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                                    }
                                    Err(e) => self.update = UpdateState::Failed { info: Some(info.clone()), error: format!("{e:#}") },
                                }
                            }
                            if ghost_button(ui, "Later").clicked() {
                                close = true;
                            }
                        });
                    });
                }
                UpdateState::Failed { error, .. } => {
                    ui.label(RichText::new(error).color(RED).font(regular(12.5)));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Try again", ph::ARROW_CLOCKWISE), vec2(150.0, 38.0), true).clicked() {
                                self.start_update_download(info.clone());
                            }
                            if ghost_button(ui, "Close").clicked() {
                                close = true;
                            }
                        });
                    });
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, &format!("{}  Download update", ph::DOWNLOAD_SIMPLE), vec2(190.0, 38.0), true).clicked() {
                                self.start_update_download(info.clone());
                            }
                            if ghost_button(ui, "Later").clicked() {
                                close = true;
                            }
                        });
                    });
                }
            }
        });
        if close || resp.should_close() {
            self.show_update_dialog = false;
        }
    }

    /// Linux/macOS, started outside the install folder: install (or update the installed copy)
    /// into a folder of the user's choice, or just run from here.
    fn install_screen(&mut self, ui: &mut egui::Ui) {
        let offer = self.install_offer.clone();
        let updating = matches!(offer, Offer::Update { .. });
        let (mut go, mut skip) = (false, false);
        ui.add_space((ui.available_height() * 0.5 - 200.0).max(24.0));
        ui.vertical_centered(|ui| {
            card().inner_margin(Margin::same(28)).show(ui, |ui| {
                ui.set_width(520.0_f32.min(ui.available_width()));
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        if let Some(icon) = &self.icon {
                            ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(46.0, 46.0)));
                        }
                        ui.vertical(|ui| {
                            kicker(ui, &format!("Version {}  ·  made by {}", update::CURRENT_VERSION, update::AUTHOR));
                            ui.label(RichText::new(if updating { "Update Akulav Tracker" } else { "Install Akulav Tracker" }).font(bold(20.0)));
                        });
                    });
                    ui.add_space(10.0);
                    match &offer {
                        Offer::Install { .. } => {
                            ui.label(RichText::new("Choose a folder. The app and its data (cache, settings) go there, and updates keep using it.").color(MUTED));
                            ui.add_space(14.0);
                            ui.label(RichText::new("Install folder").font(semibold(13.0)));
                            let edit = egui::TextEdit::singleline(&mut self.install_dir_edit).desired_width(f32::INFINITY).font(regular(13.5)).margin(Margin::symmetric(10, 8));
                            if ui.add(edit).changed() {
                                self.install_error = None;
                            }
                            let extras = if cfg!(target_os = "macos") { "Creates Akulav Tracker.app in this folder." } else { "Also adds an app-menu entry, a Desktop shortcut and the akulav-tracker command." };
                            ui.label(RichText::new(extras).font(regular(12.0)).color(FAINT));
                        }
                        Offer::Update { root, installed } => {
                            let from = installed.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "an older version".into());
                            ui.label(
                                RichText::new(format!("Akulav Tracker {from} is installed in {}. Update it to {}? Your settings and cache stay where they are.", root.display(), update::CURRENT_VERSION))
                                    .color(MUTED),
                            );
                        }
                        Offer::None => {}
                    }
                    if let Some(err) = &self.install_error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(err).font(regular(12.5)).color(RED));
                    }
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = if updating { format!("{}  Update", ph::ARROW_CIRCLE_UP) } else { format!("{}  Install", ph::DOWNLOAD_SIMPLE) };
                            if primary_button(ui, &label, vec2(150.0, 38.0), true).clicked() {
                                go = true;
                            }
                            if ghost_button(ui, "Just run it").on_hover_text("Run this copy without installing it").clicked() {
                                skip = true;
                            }
                        });
                    });
                });
            });
        });
        if go {
            self.run_install(ui.ctx());
        } else if skip {
            self.install_offer = Offer::None;
            self.start_normally();
        }
    }

    fn run_install(&mut self, ctx: &egui::Context) {
        let result = match self.install_offer.clone() {
            Offer::Install { .. } => install::parse_install_dir(&self.install_dir_edit)
                .and_then(|dir| install::install_and_launch(&dir, &["--installed"]).map_err(|e| format!("Could not install: {e:#}"))),
            Offer::Update { root, installed } => {
                let from = installed.unwrap_or_default();
                let args: Vec<&str> = if from.is_empty() { Vec::new() } else { vec!["--updated-from", from.as_str()] };
                install::install_and_launch(&root, &args).map_err(|e| format!("Could not update: {e:#}"))
            }
            Offer::None => return,
        };
        match result {
            Ok(()) => {
                self.skip_save = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.install_error = Some(e),
        }
    }

    /// Development aid: with `AKULAV_SHOT_DIR` set, visit every page, save a PNG of each, then quit.
    fn shot_tour(&mut self, ctx: &egui::Context) {
        let Some(dir) = std::env::var_os("AKULAV_SHOT_DIR").map(PathBuf::from) else { return };
        // AKULAV_SHOT_SIZE=1600x1000 shoots at that window size (points).
        if ctx.input(|i| i.time) < 0.5 {
            if let Some((w, h)) = std::env::var("AKULAV_SHOT_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))) {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w, h)));
            }
        }
        let first_repo = self.repos.first().map(|r| r.id);
        let pages: Vec<(&str, Option<Page>)> = vec![
            ("overview", Some(Page::Overview)),
            ("repos", Some(Page::Repos)),
            ("maintenance", Some(Page::Maintenance)),
            ("repo", first_repo.map(Page::Repo)),
            ("settings", Some(Page::Settings)),
        ];
        // Steps of 1.5 s, counted in tenths of a second.
        let frame = (ctx.input(|i| i.time) * 10.0) as usize;
        for ev in ctx.input(|i| i.raw.events.clone()) {
            if let egui::Event::Screenshot { image, user_data, .. } = ev {
                if let Some(name) = user_data.data.as_ref().and_then(|d| d.downcast_ref::<String>()) {
                    let _ = std::fs::create_dir_all(&dir);
                    let [w, h] = image.size;
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                        let _ = img.save(dir.join(format!("{name}.png")));
                    }
                }
            }
        }
        const STEP: usize = 15;
        let idx = frame / STEP;
        if idx >= pages.len() {
            if frame > pages.len() * STEP + 5 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if let (name, Some(page)) = pages[idx] {
            if self.session().is_some() && self.page != page {
                if let Page::Repo(id) = page {
                    self.open_repo(id);
                } else {
                    self.go(page);
                }
            }
            if frame % STEP == STEP - 3 && self.shot_taken != Some(idx) {
                self.shot_taken = Some(idx);
                let name = if self.session().is_none() { "login".to_string() } else { name.to_string() };
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(name)));
            }
        }
        ctx.request_repaint();
    }

    /// A window closed while minimized is saved at its minimized size (about 160×60) and comes
    /// back that small; the restored size skips the minimum, so grow it back.
    fn keep_min_size(&self, ctx: &egui::Context) {
        let (rect, minimized) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().minimized.unwrap_or(false)));
        let Some(rect) = rect else { return };
        let min = vec2(crate::MIN_WINDOW[0], crate::MIN_WINDOW[1]);
        if !minimized && (rect.width() < min.x - 1.0 || rect.height() < min.y - 1.0) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(rect.size().max(min)));
        }
    }

    fn resize_handles(&self, ctx: &egui::Context) {
        use egui::viewport::ResizeDirection as D;
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let r = ctx.screen_rect();
        let t = 5.0;
        let zones = [
            (Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + t)), D::North, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(pos2(r.left(), r.bottom() - t), r.right_bottom()), D::South, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(r.left_top(), pos2(r.left() + t, r.bottom())), D::West, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(pos2(r.right() - t, r.top()), r.right_bottom()), D::East, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_size(r.left_top(), vec2(t * 2.0, t * 2.0)), D::NorthWest, egui::CursorIcon::ResizeNwSe),
            (Rect::from_min_size(pos2(r.right() - t * 2.0, r.top()), vec2(t * 2.0, t * 2.0)), D::NorthEast, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_size(pos2(r.left(), r.bottom() - t * 2.0), vec2(t * 2.0, t * 2.0)), D::SouthWest, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_size(r.right_bottom() - vec2(t * 3.0, t * 3.0), vec2(t * 3.0, t * 3.0)), D::SouthEast, egui::CursorIcon::ResizeNwSe),
        ];
        egui::Area::new(Id::new("resize_handles")).order(egui::Order::Debug).fixed_pos(r.min).interactable(true).show(ctx, |ui| {
            for (i, (rect, dir, cursor)) in zones.into_iter().enumerate().rev() {
                let resp = ui.interact(rect, Id::new(("resize", i)), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ctx.set_cursor_icon(cursor);
                }
                if resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [BG.r() as f32 / 255.0, BG.g() as f32 / 255.0, BG.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.keep_min_size(ctx);
        self.handle_events(ctx);
        egui::TopBottomPanel::top("title_bar").exact_height(42.0).frame(Frame::new().fill(SURFACE)).show(ctx, |ui| self.title_bar(ui));

        if self.install_offer != Offer::None {
            egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| self.install_screen(ui));
        } else if self.session().is_none() {
            egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| self.login_page(ui));
        } else {
            self.shortcuts(ctx);
            if self.settings.auto_check_updates && self.last_update_check.elapsed() > Duration::from_secs(3 * 3600) {
                self.check_updates(false);
            }
            egui::SidePanel::left("sidebar")
                .exact_width(SIDEBAR_W)
                .resizable(false)
                .frame(Frame::new().fill(SURFACE).inner_margin(Margin { left: 12, right: 12, top: 0, bottom: 0 }))
                .show(ctx, |ui| {
                    let r = ui.max_rect();
                    ui.painter().vline(r.right() + 11.5, r.y_range(), Stroke::new(1.0_f32, BORDER));
                    self.sidebar(ui)
                });

            // Pages fade in when switched.
            let fade = (self.page_since.elapsed().as_secs_f32() / 0.2).min(1.0);
            let fade = 1.0 - (1.0 - fade).powi(3);
            if fade < 1.0 {
                ctx.request_repaint();
            }
            egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| {
                ui.set_opacity(fade);
                match self.page {
                    Page::Overview => self.overview_page(ui),
                    Page::Repos => self.repos_page(ui),
                    Page::Maintenance => self.maintenance_page(ui),
                    Page::Repo(id) => self.repo_page(ui, id),
                    Page::Settings => self.settings_page(ui),
                }
            });
            // "Synced 3 minutes ago" stays current.
            ctx.request_repaint_after(Duration::from_secs(30));
        }

        self.update_dialog(ctx);
        self.confirm_ui(ctx);
        self.toasts_ui(ctx);
        self.resize_handles(ctx);
        self.shot_tour(ctx);

        if let Some(t) = self.settings_dirty.filter(|_| !self.skip_save) {
            if t.elapsed() > Duration::from_millis(500) {
                self.save_settings_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(600));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.sync.cancel.store(true, Ordering::Relaxed);
        if let Some(l) = self.login_mut() {
            l.cancel.store(true, Ordering::Relaxed);
        }
        if !self.skip_save {
            self.save_settings_now();
            if !self.sync.running {
                self.save_cache();
            }
        }
    }
}

pub fn window_button(ui: &mut egui::Ui, icon: &str, is_close: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(46.0, 42.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let bg = if is_close { Color32::from_rgb(196, 43, 28) } else { Color32::from_white_alpha(14) };
    if hover > 0.0 {
        ui.painter().rect_filled(rect, 0.0, with_alpha(bg, (bg.a() as f32 * hover) as u8));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, regular(15.0), lerp_color(MUTED, TEXT, hover));
    resp
}

fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, active: bool, badge: Option<(String, Color32)>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let act = ui.ctx().animate_bool_with_time(resp.id.with("a"), active, 0.15);
    let p = ui.painter();
    let bg = lerp_color(with_alpha(Color32::WHITE, (hover * 8.0) as u8), with_alpha(ACCENT, 30), act);
    p.rect_filled(rect, CornerRadius::same(R_MD), bg);
    if act > 0.0 {
        let bar = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.0 * act), vec2(3.0, 20.0 * act));
        p.rect_filled(bar, CornerRadius::same(2), ACCENT_HI);
    }
    let fg = lerp_color(lerp_color(MUTED, TEXT, hover), TEXT, act);
    p.text(rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, icon, regular(18.0), if active { ACCENT_HI } else { fg });
    p.text(rect.left_center() + vec2(44.0, 0.0), Align2::LEFT_CENTER, label, semibold(14.0), fg);
    if let Some((text, color)) = badge {
        let g = p.layout_no_wrap(text, bold(11.0), TEXT);
        let w = (g.size().x + 12.0).max(22.0);
        let b = Rect::from_center_size(rect.right_center() - vec2(10.0 + w / 2.0, 0.0), vec2(w, 20.0));
        p.rect_filled(b, CornerRadius::same(255), color);
        p.galley(b.center() - g.size() / 2.0, g, TEXT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Decode an image into a texture. Size limits keep a hostile file (avatars come from the
/// network) from exhausting memory.
pub fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}
