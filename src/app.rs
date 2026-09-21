use crate::{
    api::{AdminShare, AdminStats, AdminUser, CabinetClient, CabinetFile, Folder, Share, User},
    config::{self, AppConfig},
    credentials,
};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Files,
    Shares,
    Admin,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Grid,
    List,
}

enum Dialog {
    CreateFolder { name: String },
    Rename { id: String, value: String },
    Move { id: String, target: Option<String> },
    ShareUser { id: String, username: String },
    AdminCreateUser {
        username: String,
        password: String,
        quota_gb: String,
    },
    AdminEditUser {
        id: String,
        username: String,
        password: String,
        quota_gb: String,
    },
}

type DialogAction = Box<dyn FnOnce(&mut CabinetApp)>;

#[derive(Clone, Copy)]
enum RefreshAfter {
    None,
    Files,
    Shares,
    Admin,
}

enum Message {
    Restore(Result<(CabinetClient, User), String>),
    Login(Result<(CabinetClient, User), String>),
    Refresh(Result<(Vec<CabinetFile>, Vec<Folder>, User), String>),
    Action(Result<String, String>, RefreshAfter),
    ShareLink(Result<String, String>),
    Shares(Result<Vec<Share>, String>),
    Thumbnail(String, Result<Option<Vec<u8>>, String>),
    Admin(Result<(AdminStats, Vec<AdminUser>, Vec<AdminShare>, String), String>),
}

pub struct CabinetApp {
    tx: Sender<Message>,
    rx: Receiver<Message>,
    client: Option<CabinetClient>,
    user: Option<User>,
    server_url: String,
    username: String,
    password: String,
    files: Vec<CabinetFile>,
    folders: Vec<Folder>,
    current_folder: Option<String>,
    selected_file: Option<String>,
    search: String,
    view_mode: ViewMode,
    screen: Screen,
    busy: usize,
    status: String,
    dialog: Option<Dialog>,
    shares: Vec<Share>,
    thumbnails: HashMap<String, egui::TextureHandle>,
    thumbnail_pending: HashSet<String>,
    admin_stats: Option<AdminStats>,
    admin_users: Vec<AdminUser>,
    admin_shares: Vec<AdminShare>,
    admin_logs: String,
    allow_quit: bool,
    pending_clipboard: Option<String>,
    quit_requested: Arc<AtomicBool>,
    _tray: Option<TrayIcon>,
}

impl CabinetApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let cfg = config::load();
        let quit_requested = Arc::new(AtomicBool::new(false));
        let tray = setup_tray(&cc.egui_ctx, quit_requested.clone());

        let mut app = Self {
            tx,
            rx,
            client: None,
            user: None,
            server_url: cfg.server_url.clone(),
            username: cfg.username.clone(),
            password: String::new(),
            files: Vec::new(),
            folders: Vec::new(),
            current_folder: None,
            selected_file: None,
            search: String::new(),
            view_mode: ViewMode::Grid,
            screen: Screen::Files,
            busy: 0,
            status: String::new(),
            dialog: None,
            shares: Vec::new(),
            thumbnails: HashMap::new(),
            thumbnail_pending: HashSet::new(),
            admin_stats: None,
            admin_users: Vec::new(),
            admin_shares: Vec::new(),
            admin_logs: String::new(),
            allow_quit: false,
            pending_clipboard: None,
            quit_requested,
            _tray: tray,
        };
        app.restore_session();
        app
    }

    fn restore_session(&mut self) {
        let cfg = config::load();
        let Some(token) = credentials::load() else {
            return;
        };
        if cfg.server_url.is_empty() {
            return;
        }
        self.busy += 1;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = CabinetClient::from_token(&cfg.server_url, token)
                .and_then(|client| client.me().map(|user| (client, user)));
            let _ = tx.send(Message::Restore(result));
        });
    }

    fn login(&mut self) {
        if self.server_url.trim().is_empty()
            || self.username.trim().is_empty()
            || self.password.is_empty()
        {
            self.status = "Enter a server URL, username, and password.".into();
            return;
        }
        self.busy += 1;
        let tx = self.tx.clone();
        let server = self.server_url.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        thread::spawn(move || {
            let _ = tx.send(Message::Login(CabinetClient::login(
                &server, &username, &password,
            )));
        });
    }

    fn refresh(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.busy += 1;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client.list_files().and_then(|files| {
                client
                    .list_folders()
                    .and_then(|folders| client.me().map(|user| (files, folders, user)))
            });
            let _ = tx.send(Message::Refresh(result));
        });
    }

    fn refresh_shares(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.busy += 1;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Message::Shares(client.list_shares()));
        });
    }

    fn load_thumbnail(&mut self, id: String) {
        if self.thumbnails.contains_key(&id) || !self.thumbnail_pending.insert(id.clone()) {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.thumbnail_pending.remove(&id);
            return;
        };
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client.thumbnail(&id);
            let _ = tx.send(Message::Thumbnail(id, result));
        });
    }

    fn refresh_admin(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.busy += 1;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client.admin_stats().and_then(|stats| {
                client.admin_users().and_then(|users| {
                    client.admin_shares().and_then(|shares| {
                        client.admin_logs().map(|logs| (stats, users, shares, logs))
                    })
                })
            });
            let _ = tx.send(Message::Admin(result));
        });
    }

    fn run_action<F>(&mut self, refresh: RefreshAfter, f: F)
    where
        F: FnOnce(CabinetClient) -> Result<String, String> + Send + 'static,
    {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.busy += 1;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = f(client);
            let _ = tx.send(Message::Action(result, refresh));
        });
    }

    fn process_messages(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.rx.try_recv() {
            if !matches!(message, Message::Thumbnail(_, _)) {
                self.busy = self.busy.saturating_sub(1);
            }
            match message {
                Message::Restore(result) | Message::Login(result) => match result {
                    Ok((client, user)) => {
                        if let Err(error) = credentials::save(client.token()) {
                            self.status = format!("Connected, but session storage failed: {error}");
                        } else {
                            self.status = format!("Connected to {}", client.base_url());
                        }
                        self.server_url = client.base_url().to_string();
                        self.username = user.username.clone();
                        let _ = config::save(&AppConfig {
                            server_url: self.server_url.clone(),
                            username: self.username.clone(),
                        });
                        self.password.clear();
                        self.client = Some(client);
                        self.user = Some(user);
                        self.refresh();
                    }
                    Err(error) => {
                        credentials::clear();
                        self.status = error;
                    }
                },
                Message::Refresh(result) => match result {
                    Ok((files, folders, user)) => {
                        let live_ids: HashSet<String> =
                            files.iter().map(|file| file.id.clone()).collect();
                        self.thumbnails.retain(|id, _| live_ids.contains(id));
                        self.thumbnail_pending.retain(|id| live_ids.contains(id));
                        self.files = files;
                        self.folders = folders;
                        self.user = Some(user);
                        self.status.clear();
                    }
                    Err(error) => self.handle_error(error),
                },
                Message::Action(result, refresh) => match result {
                    Ok(message) => {
                        self.status = message;
                        match refresh {
                            RefreshAfter::None => {}
                            RefreshAfter::Files => self.refresh(),
                            RefreshAfter::Shares => self.refresh_shares(),
                            RefreshAfter::Admin => self.refresh_admin(),
                        }
                    }
                    Err(error) => self.handle_error(error),
                },
                Message::ShareLink(result) => match result {
                    Ok(url) => {
                        self.pending_clipboard = Some(url);
                        self.status = "Public link copied to clipboard".into();
                        self.refresh_shares();
                    }
                    Err(error) => self.handle_error(error),
                },
                Message::Shares(result) => match result {
                    Ok(shares) => self.shares = shares,
                    Err(error) => self.handle_error(error),
                },
                Message::Thumbnail(id, result) => {
                    self.thumbnail_pending.remove(&id);
                    if let Ok(Some(bytes)) = result {
                        if let Ok(decoded) = image::load_from_memory(&bytes) {
                            let rgba = decoded.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let color =
                                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                            let texture = ctx.load_texture(
                                format!("cabinet-thumbnail-{id}"),
                                color,
                                egui::TextureOptions::LINEAR,
                            );
                            self.thumbnails.insert(id, texture);
                        }
                    }
                }
                Message::Admin(result) => match result {
                    Ok((stats, users, shares, logs)) => {
                        self.admin_stats = Some(stats);
                        self.admin_users = users;
                        self.admin_shares = shares;
                        self.admin_logs = logs;
                    }
                    Err(error) => self.handle_error(error),
                },
            }
        }
    }

    fn handle_error(&mut self, error: String) {
        if CabinetClient::is_unauthorized(&error) {
            credentials::clear();
            self.client = None;
            self.user = None;
            self.files.clear();
            self.folders.clear();
        }
        self.status = error;
    }

    fn logout(&mut self) {
        if let Some(client) = self.client.take() {
            thread::spawn(move || client.logout());
        }
        credentials::clear();
        self.user = None;
        self.files.clear();
        self.folders.clear();
        self.current_folder = None;
        self.selected_file = None;
        self.status = "Signed out".into();
    }

    fn upload(&mut self) {
        let files = rfd::FileDialog::new().pick_files();
        let Some(paths) = files else {
            return;
        };
        let parent = self.current_folder.clone();
        self.run_action(RefreshAfter::Files, move |client| {
            let count = paths.len();
            for path in paths {
                client.upload_file(&path, parent.as_deref())?;
            }
            Ok(if count == 1 {
                "Upload complete".into()
            } else {
                format!("{count} files uploaded")
            })
        });
    }

    fn download_selected(&mut self) {
        let Some(file) = self.selected().cloned() else {
            return;
        };
        let destination = rfd::FileDialog::new().set_file_name(&file.name).save_file();
        let Some(destination) = destination else {
            return;
        };
        self.run_action(RefreshAfter::None, move |client| {
            client.download_file(&file.id, &destination)?;
            Ok(format!("Downloaded {}", file.name))
        });
    }

    fn selected(&self) -> Option<&CabinetFile> {
        let id = self.selected_file.as_deref()?;
        self.files.iter().find(|file| file.id == id)
    }

    fn folder_map(&self) -> HashMap<&str, &Folder> {
        self.folders
            .iter()
            .map(|folder| (folder.id.as_str(), folder))
            .collect()
    }

    fn breadcrumbs(&self) -> Vec<Folder> {
        let map = self.folder_map();
        let mut output = Vec::new();
        let mut cursor = self.current_folder.as_deref();
        let mut visited = HashSet::new();
        while let Some(id) = cursor {
            if !visited.insert(id.to_string()) {
                break;
            }
            let Some(folder) = map.get(id) else {
                break;
            };
            output.push((*folder).clone());
            cursor = folder.parent_id.as_deref();
        }
        output.reverse();
        output
    }

    fn filtered_folders(&self) -> Vec<Folder> {
        let query = self.search.to_lowercase();
        self.folders
            .iter()
            .filter(|folder| {
                if !query.is_empty() {
                    folder.name.to_lowercase().contains(&query)
                } else {
                    folder.parent_id.as_deref() == self.current_folder.as_deref()
                }
            })
            .cloned()
            .collect()
    }

    fn filtered_files(&self) -> Vec<CabinetFile> {
        let query = self.search.to_lowercase();
        self.files
            .iter()
            .filter(|file| {
                if !query.is_empty() {
                    file.name.to_lowercase().contains(&query)
                } else {
                    file.parent_id.as_deref() == self.current_folder.as_deref()
                }
            })
            .cloned()
            .collect()
    }

    fn ui_login(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                ui.add_space(90.0);
                ui.heading(
                    RichText::new("Cabinet")
                        .size(34.0)
                        .strong()
                        .color(Color32::from_rgb(37, 99, 235)),
                );
                ui.label(
                    RichText::new("Connect to your self-hosted Cabinet server")
                        .color(Color32::GRAY),
                );
                ui.add_space(24.0);
                ui.set_max_width(430.0);
                ui.label("Server URL");
                ui.text_edit_singleline(&mut self.server_url);
                ui.add_space(8.0);
                ui.label("Username");
                ui.text_edit_singleline(&mut self.username);
                ui.add_space(8.0);
                ui.label("Password");
                let response =
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                ui.add_space(16.0);
                let login = ui.add_enabled(
                    self.busy == 0,
                    egui::Button::new("Connect").min_size(Vec2::new(160.0, 38.0)),
                );
                if login.clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.login();
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new(
                        "Your session token is stored in the operating system credential store.",
                    )
                    .small()
                    .color(Color32::GRAY),
                );
                if !self.status.is_empty() {
                    ui.add_space(16.0);
                    ui.label(RichText::new(&self.status).color(Color32::from_rgb(190, 55, 55)));
                }
            });
        });
    }

    fn ui_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("sidebar")
            .exact_size(210.0)
            .show(root, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("▣")
                            .size(24.0)
                            .color(Color32::from_rgb(37, 99, 235)),
                    );
                    ui.heading("Cabinet");
                });
                ui.add_space(20.0);
                if nav_button(ui, "Files", self.screen == Screen::Files).clicked() {
                    self.screen = Screen::Files;
                }
                if nav_button(ui, "Shares", self.screen == Screen::Shares).clicked() {
                    self.screen = Screen::Shares;
                    self.refresh_shares();
                }
                if self.user.as_ref().is_some_and(|u| u.role == "admin")
                    && nav_button(ui, "Administration", self.screen == Screen::Admin).clicked()
                {
                    self.screen = Screen::Admin;
                    self.refresh_admin();
                }
                if nav_button(ui, "Settings", self.screen == Screen::Settings).clicked() {
                    self.screen = Screen::Settings;
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    if ui.button("Sign out").clicked() {
                        self.logout();
                    }
                    if let Some(user) = &self.user {
                        ui.label(RichText::new(&user.role).small().color(Color32::GRAY));
                        ui.label(RichText::new(&user.username).strong());
                    }
                });
            });
    }

    fn ui_files(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("toolbar")
            .exact_size(62.0)
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui.button("Files").clicked() {
                        self.current_folder = None;
                    }
                    for crumb in self.breadcrumbs() {
                        ui.label("/");
                        if ui.button(&crumb.name).clicked() {
                            self.current_folder = Some(crumb.id);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(if self.view_mode == ViewMode::Grid {
                                "List"
                            } else {
                                "Grid"
                            })
                            .clicked()
                        {
                            self.view_mode = if self.view_mode == ViewMode::Grid {
                                ViewMode::List
                            } else {
                                ViewMode::Grid
                            };
                        }
                        if ui.button("Refresh").clicked() {
                            self.refresh();
                        }
                        ui.add_sized(
                            [230.0, 30.0],
                            egui::TextEdit::singleline(&mut self.search).hint_text("Search files"),
                        );
                    });
                });
            });

        if self.selected().is_some() {
            self.ui_details(root);
        }

        egui::CentralPanel::default().show(root, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading("Your files");
                    if let Some(user) = &self.user {
                        ui.label(
                            RichText::new(format!(
                                "{} used of {}",
                                format_bytes(user.used_space),
                                format_bytes(user.quota)
                            ))
                            .color(Color32::GRAY),
                        );
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(self.busy == 0, egui::Button::new("Upload"))
                        .clicked()
                    {
                        self.upload();
                    }
                    if ui.button("New folder").clicked() {
                        self.dialog = Some(Dialog::CreateFolder {
                            name: String::new(),
                        });
                    }
                    if let Some(folder_id) = self.current_folder.clone() {
                        if ui.button("Delete folder").clicked() {
                            let parent = self
                                .folder_map()
                                .get(folder_id.as_str())
                                .and_then(|folder| folder.parent_id.clone());
                            self.run_action(RefreshAfter::Files, move |client| {
                                client.delete_folder(&folder_id)?;
                                Ok("Folder deleted".into())
                            });
                            self.current_folder = parent;
                        }
                    }
                });
            });
            ui.add_space(18.0);

            let folders = self.filtered_folders();
            let files = self.filtered_files();
            if self.view_mode == ViewMode::List {
                egui::Grid::new("file-list")
                    .striped(true)
                    .min_col_width(120.0)
                    .show(ui, |ui| {
                        ui.strong("Name");
                        ui.strong("Type");
                        ui.strong("Size");
                        ui.end_row();
                        for folder in folders {
                            if ui
                                .selectable_label(false, format!("📁 {}", folder.name))
                                .double_clicked()
                            {
                                self.current_folder = Some(folder.id);
                            }
                            ui.label("Folder");
                            ui.label("—");
                            ui.end_row();
                        }
                        for file in files {
                            let selected = self.selected_file.as_deref() == Some(file.id.as_str());
                            if ui.selectable_label(selected, &file.name).clicked() {
                                self.selected_file = Some(file.id.clone());
                            }
                            ui.label(file.mime_type.as_deref().unwrap_or("File"));
                            ui.label(format_bytes(file.size));
                            ui.end_row();
                        }
                    });
            } else {
                ui.horizontal_wrapped(|ui| {
                    for folder in folders {
                        if card(ui, "📁", &folder.name, "Folder", false, None).clicked() {
                            self.current_folder = Some(folder.id);
                        }
                    }
                    for file in files {
                        let selected = self.selected_file.as_deref() == Some(file.id.as_str());
                        if file.thumbnail.is_some()
                            && !self.thumbnails.contains_key(&file.id)
                            && !self.thumbnail_pending.contains(&file.id)
                        {
                            self.load_thumbnail(file.id.clone());
                        }
                        let texture = self.thumbnails.get(&file.id);
                        if card(
                            ui,
                            "📄",
                            &file.name,
                            &format_bytes(file.size),
                            selected,
                            texture,
                        )
                        .clicked()
                        {
                            self.selected_file = Some(file.id.clone());
                        }
                    }
                });
            }
        });
    }

    fn ui_details(&mut self, root: &mut egui::Ui) {
        let Some(file) = self.selected().cloned() else {
            return;
        };
        egui::Panel::right("details")
            .exact_size(300.0)
            .show(root, |ui| {
                ui.add_space(14.0);
                ui.heading(&file.name);
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        format_bytes(file.size),
                        file.mime_type.as_deref().unwrap_or("File")
                    ))
                    .color(Color32::GRAY),
                );
                ui.add_space(18.0);
                if ui.button("Download").clicked() {
                    self.download_selected();
                }
                if ui.button("Rename").clicked() {
                    self.dialog = Some(Dialog::Rename {
                        id: file.id.clone(),
                        value: file.name.clone(),
                    });
                }
                if ui.button("Move").clicked() {
                    self.dialog = Some(Dialog::Move {
                        id: file.id.clone(),
                        target: file.parent_id.clone(),
                    });
                }
                if ui.button("Copy public link").clicked() {
                    let tx = self.tx.clone();
                    let Some(client) = self.client.clone() else {
                        return;
                    };
                    let id = file.id.clone();
                    self.busy += 1;
                    thread::spawn(move || {
                        let result = client.create_public_share(&id);
                        let _ = tx.send(Message::ShareLink(result));
                    });
                }
                if ui.button("Share with Cabinet user").clicked() {
                    self.dialog = Some(Dialog::ShareUser {
                        id: file.id.clone(),
                        username: String::new(),
                    });
                }
                ui.add_space(12.0);
                if ui
                    .add(egui::Button::new(
                        RichText::new("Delete").color(Color32::from_rgb(200, 45, 45)),
                    ))
                    .clicked()
                {
                    let id = file.id.clone();
                    self.selected_file = None;
                    self.run_action(RefreshAfter::Files, move |client| {
                        client.delete_file(&id)?;
                        Ok("File deleted".into())
                    });
                }
            });
    }

    fn ui_admin(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Administration");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.refresh_admin();
                    }
                    if ui.button("Scrub database").clicked() {
                        self.run_action(RefreshAfter::Admin, move |client| {
                            let removed = client.admin_scrub()?;
                            Ok(format!("Database scrub complete: {removed} missing file record(s) removed"))
                        });
                    }
                    if ui.button("Create user").clicked() {
                        self.dialog = Some(Dialog::AdminCreateUser {
                            username: String::new(),
                            password: String::new(),
                            quota_gb: "50".into(),
                        });
                    }
                });
            });
            ui.label(
                RichText::new("Server overview from the Cabinet admin API").color(Color32::GRAY),
            );
            ui.add_space(16.0);
            if let Some(stats) = &self.admin_stats {
                ui.horizontal_wrapped(|ui| {
                    stat(ui, "Users", stats.total_users.to_string());
                    stat(ui, "Files", stats.total_files.to_string());
                    stat(ui, "Shares", stats.total_shares.to_string());
                    stat(ui, "Storage", format_bytes(stats.total_storage_used));
                });
            }
            ui.add_space(18.0);
            ui.heading("Users");
            let admin_users = self.admin_users.clone();
            egui::Grid::new("admin-users").striped(true).show(ui, |ui| {
                ui.strong("Username");
                ui.strong("Role");
                ui.strong("Storage");
                ui.strong("");
                ui.end_row();
                for user in admin_users {
                    ui.label(&user.username);
                    ui.label(&user.role);
                    ui.label(format!(
                        "{} / {}",
                        format_bytes(user.used_space),
                        format_bytes(user.quota)
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Edit").clicked() {
                            self.dialog = Some(Dialog::AdminEditUser {
                                id: user.id.clone(),
                                username: user.username.clone(),
                                password: String::new(),
                                quota_gb: format!("{:.1}", user.quota as f64 / 1024_f64.powi(3)),
                            });
                        }
                        if user.role != "admin" && ui.button("Delete").clicked() {
                            let id = user.id.clone();
                            self.run_action(RefreshAfter::Admin, move |client| {
                                client.admin_delete_user(&id)?;
                                Ok("User deleted".into())
                            });
                        }
                    });
                    ui.end_row();
                }
            });
            ui.add_space(18.0);
            ui.heading("Public shares");
            let admin_shares = self.admin_shares.clone();
            egui::Grid::new("admin-shares")
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("File");
                    ui.strong("Creator");
                    ui.strong("Downloads");
                    ui.strong("");
                    ui.end_row();
                    for share in admin_shares {
                        ui.label(share.file_name.as_deref().unwrap_or(&share.file_id));
                        ui.label(share.creator_name.as_deref().unwrap_or(&share.creator_id));
                        ui.label(share.downloads.to_string());
                        if ui.button("Revoke").clicked() {
                            let id = share.id.clone();
                            self.run_action(RefreshAfter::Admin, move |client| {
                                client.admin_revoke_share(&id)?;
                                Ok("Share revoked".into())
                            });
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(18.0);
            ui.heading("Server logs");
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.monospace(&self.admin_logs);
                });
        });
    }

    fn ui_shares(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Public shares");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.refresh_shares();
                    }
                });
            });
            ui.label(
                RichText::new("Manage public links created from Cabinet.")
                    .color(Color32::GRAY),
            );
            ui.add_space(16.0);

            if self.shares.is_empty() {
                ui.label("No active public shares.");
                return;
            }

            let shares = self.shares.clone();
            egui::Grid::new("shares-list")
                .striped(true)
                .min_col_width(120.0)
                .show(ui, |ui| {
                    ui.strong("File");
                    ui.strong("Downloads");
                    ui.strong("Limit");
                    ui.strong("Expires");
                    ui.strong("");
                    ui.end_row();

                    for share in shares {
                        ui.label(
                            share
                                .file_name
                                .as_deref()
                                .unwrap_or(&share.file_id),
                        );
                        ui.label(share.downloads.to_string());
                        ui.label(
                            share
                                .download_limit
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "∞".to_string()),
                        );
                        ui.label(
                            share
                                .expires_at
                                .as_deref()
                                .unwrap_or("Never"),
                        );
                        if ui.button("Revoke").clicked() {
                            let id = share.id.clone();
                            self.run_action(RefreshAfter::Shares, move |client| {
                                client.revoke_share(&id)?;
                                Ok("Share revoked".to_string())
                            });
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn ui_settings(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.heading("Settings");
            ui.add_space(16.0);
            ui.group(|ui| {
                ui.strong("Cabinet server");
                ui.label(&self.server_url);
                ui.label(RichText::new("Each installation connects to the server chosen by its user. No public Cabinet host is hardcoded.").color(Color32::GRAY));
            });
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.strong("Tray persistence");
                ui.label("Closing the window hides Cabinet to the system tray. Use Quit in the tray menu to exit completely.");
            });
        });
    }

    fn ui_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut action: Option<DialogAction> = None;
        if let Some(dialog) = &mut self.dialog {
            egui::Window::new(match dialog {
                Dialog::CreateFolder { .. } => "New folder",
                Dialog::Rename { .. } => "Rename file",
                Dialog::Move { .. } => "Move file",
                Dialog::ShareUser { .. } => "Share with user",
                Dialog::AdminCreateUser { .. } => "Create user",
                Dialog::AdminEditUser { .. } => "Edit user",
            })
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                match dialog {
                    Dialog::CreateFolder { name } => {
                        ui.label("Folder name");
                        ui.text_edit_singleline(name);
                        let value = name.trim().to_string();
                        if ui.button("Create").clicked() && !value.is_empty() {
                            action = Some(Box::new(move |app| {
                                let parent = app.current_folder.clone();
                                app.run_action(RefreshAfter::Files, move |client| {
                                    client.create_folder(&value, parent.as_deref())?;
                                    Ok("Folder created".into())
                                });
                            }));
                            close = true;
                        }
                    }
                    Dialog::Rename { id, value } => {
                        ui.label("File name");
                        ui.text_edit_singleline(value);
                        let id = id.clone();
                        let value = value.trim().to_string();
                        if ui.button("Rename").clicked() && !value.is_empty() {
                            action = Some(Box::new(move |app| {
                                app.run_action(RefreshAfter::Files, move |client| {
                                    client.rename_file(&id, &value)?;
                                    Ok("File renamed".into())
                                });
                            }));
                            close = true;
                        }
                    }
                    Dialog::Move { id, target } => {
                        ui.label("Destination folder");
                        egui::ComboBox::from_id_salt("move-target")
                            .selected_text(
                                target
                                    .as_deref()
                                    .and_then(|id| {
                                        self.folders
                                            .iter()
                                            .find(|folder| folder.id == id)
                                            .map(|folder| folder.name.as_str())
                                    })
                                    .unwrap_or("Files"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(target, None, "Files");
                                for folder in &self.folders {
                                    ui.selectable_value(
                                        target,
                                        Some(folder.id.clone()),
                                        &folder.name,
                                    );
                                }
                            });
                        let id = id.clone();
                        let target = target.clone();
                        if ui.button("Move").clicked() {
                            action = Some(Box::new(move |app| {
                                app.run_action(RefreshAfter::Files, move |client| {
                                    client.move_file(&id, target.as_deref())?;
                                    Ok("File moved".into())
                                });
                            }));
                            close = true;
                        }
                    }
                    Dialog::ShareUser { id, username } => {
                        ui.label("Cabinet username");
                        ui.text_edit_singleline(username);
                        let id = id.clone();
                        let username = username.trim().to_string();
                        if ui.button("Share").clicked() && !username.is_empty() {
                            action = Some(Box::new(move |app| {
                                app.run_action(RefreshAfter::None, move |client| {
                                    client.share_with_user(&id, &username)?;
                                    Ok("File shared".into())
                                });
                            }));
                            close = true;
                        }
                    }
                    Dialog::AdminCreateUser {
                        username,
                        password,
                        quota_gb,
                    } => {
                        ui.label("Username");
                        ui.text_edit_singleline(username);
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(password).password(true));
                        ui.label("Quota (GiB)");
                        ui.text_edit_singleline(quota_gb);

                        let username = username.trim().to_string();
                        let password = password.clone();
                        let quota = quota_gb.trim().parse::<f64>().ok()
                            .map(|value| (value * 1024_f64.powi(3)) as i64);

                        let create_clicked = ui.button("Create").clicked();
                        if create_clicked && !username.is_empty() && password.len() >= 12 {
                            if let Some(quota) = quota {
                                action = Some(Box::new(move |app| {
                                    app.run_action(RefreshAfter::Admin, move |client| {
                                        client.admin_create_user(&username, &password, quota)?;
                                        Ok("User created".into())
                                    });
                                }));
                                close = true;
                            }
                        }
                    }
                    Dialog::AdminEditUser {
                        id,
                        username,
                        password,
                        quota_gb,
                    } => {
                        ui.label(format!("User: {username}"));
                        ui.label("New password (leave blank to keep current)");
                        ui.add(egui::TextEdit::singleline(password).password(true));
                        ui.label("Quota (GiB)");
                        ui.text_edit_singleline(quota_gb);

                        let id = id.clone();
                        let password = if password.trim().is_empty() {
                            None
                        } else {
                            Some(password.clone())
                        };
                        let quota = quota_gb.trim().parse::<f64>().ok()
                            .map(|value| (value * 1024_f64.powi(3)) as i64);

                        if ui.button("Save").clicked() && quota.is_some() {
                            action = Some(Box::new(move |app| {
                                app.run_action(RefreshAfter::Admin, move |client| {
                                    client.admin_update_user(
                                        &id,
                                        password.as_deref(),
                                        quota,
                                    )?;
                                    Ok("User updated".into())
                                });
                            }));
                            close = true;
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        }
        if close {
            self.dialog = None;
        }
        if let Some(action) = action {
            action(self);
        }
    }
}

impl eframe::App for CabinetApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_messages(ctx);

        if let Some(text) = self.pending_clipboard.take() {
            ctx.copy_text(text);
        }

        if self.quit_requested.swap(false, Ordering::SeqCst) {
            self.allow_quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if ctx.input(|input| input.viewport().close_requested()) && !self.allow_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.client.is_none() {
            self.ui_login(root);
            return;
        }

        self.ui_sidebar(root);

        if self.busy > 0 {
            egui::Panel::bottom("status")
                .exact_size(26.0)
                .show(root, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if self.status.is_empty() {
                            "Working…"
                        } else {
                            &self.status
                        });
                    });
                });
        } else if !self.status.is_empty() {
            egui::Panel::bottom("status")
                .exact_size(26.0)
                .show(root, |ui| {
                    ui.label(&self.status);
                });
        }

        match self.screen {
            Screen::Files => self.ui_files(root),
            Screen::Shares => self.ui_shares(root),
            Screen::Admin => self.ui_admin(root),
            Screen::Settings => self.ui_settings(root),
        }

        let ctx = root.ctx().clone();
        self.ui_dialog(&ctx);
    }
}
fn setup_tray(ctx: &egui::Context, quit_requested: Arc<AtomicBool>) -> Option<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::with_id("show", "Show Cabinet", true, None);
    let quit = MenuItem::with_id("quit", "Quit Cabinet", true, None);
    let _ = menu.append(&show);
    let _ = menu.append(&quit);

    let ctx_menu = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id().as_ref() {
        "show" => {
            ctx_menu.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx_menu.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        "quit" => {
            quit_requested.store(true, Ordering::SeqCst);
            ctx_menu.request_repaint();
        }
        _ => {}
    }));

    let ctx_click = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |_event| {
        ctx_click.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx_click.send_viewport_cmd(egui::ViewportCommand::Focus);
    }));

    let icon = tray_icon_image();
    TrayIconBuilder::new()
        .with_tooltip("Cabinet")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .build()
        .ok()
}

fn tray_icon_image() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = ((y * SIZE + x) * 4) as usize;
            let inside = (5..27).contains(&x) && (5..27).contains(&y);
            let drawer = (9..23).contains(&x) && ((10..15).contains(&y) || (18..23).contains(&y));
            let (r, g, b, a) = if drawer {
                (255, 255, 255, 255)
            } else if inside {
                (37, 99, 235, 255)
            } else {
                (0, 0, 0, 0)
            };
            rgba[i..i + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("valid tray icon")
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(247, 249, 252);
    visuals.window_fill = Color32::WHITE;
    visuals.selection.bg_fill = Color32::from_rgb(219, 234, 254);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(37, 99, 235));
    ctx.set_visuals(visuals);
}

fn nav_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    ui.add_sized([180.0, 36.0], egui::Button::new(label).selected(active))
}

fn card(
    ui: &mut egui::Ui,
    icon: &str,
    name: &str,
    meta: &str,
    selected: bool,
    texture: Option<&egui::TextureHandle>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(165.0, 190.0), Sense::click());
    let fill = if selected {
        Color32::from_rgb(232, 241, 255)
    } else {
        Color32::WHITE
    };
    ui.painter().rect(
        rect,
        12.0,
        fill,
        Stroke::new(1.0, Color32::from_rgb(224, 229, 238)),
        egui::StrokeKind::Inside,
    );
    let preview_rect = egui::Rect::from_min_max(
        rect.min + Vec2::new(8.0, 8.0),
        egui::pos2(rect.max.x - 8.0, rect.min.y + 120.0),
    );
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            preview_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center_top() + Vec2::new(0.0, 48.0),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(42.0),
            Color32::from_rgb(92, 132, 190),
        );
    }
    let title = truncate(name, 22);
    ui.painter().text(
        rect.left_bottom() + Vec2::new(12.0, -38.0),
        egui::Align2::LEFT_BOTTOM,
        title,
        egui::FontId::proportional(14.0),
        Color32::from_rgb(35, 45, 62),
    );
    ui.painter().text(
        rect.left_bottom() + Vec2::new(12.0, -17.0),
        egui::Align2::LEFT_BOTTOM,
        meta,
        egui::FontId::proportional(11.0),
        Color32::GRAY,
    );
    response
}

fn stat(ui: &mut egui::Ui, label: &str, value: String) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(Vec2::new(150.0, 72.0));
        ui.label(RichText::new(label).small().color(Color32::GRAY));
        ui.label(RichText::new(value).size(23.0).strong());
    });
}

fn format_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 B".into();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size:.0} {}", units[unit])
    } else {
        format!("{size:.1} {}", units[unit])
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
