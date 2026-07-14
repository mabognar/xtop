use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{backend::Backend, widgets::TableState, Terminal};
use std::io;
use std::time::{Duration, Instant};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};
use syntect::highlighting::ThemeSet;
use ratatui::prelude::Color;
use crate::ui::ui;

pub struct App {
    pub(crate) s: System,
    pub(crate) networks: Networks,
    pub(crate) update_freq: u64,
    pub(crate) table_state: TableState,
    pub(crate) filter_text: String,
    pub(crate) cursor_position: usize,
    pub(crate) sort_col: u8,
    pub(crate) current_col: u8,
    pub(crate) reverse: bool,
    pub(crate) editing: bool,
    pub(crate) show_popup: bool,
    pub(crate) process_info: u8,
    theme_set: ThemeSet,
    available_themes: Vec<String>,
    pub current_theme: String,
    pub ui_colors: crate::config::UiColors,
    pub theme_changed_time: Option<Instant>,
    pub notification: Option<String>,
    pub notification_time: Option<Instant>,
    pub update_rx: Option<Receiver<String>>,
    pub update_version: Option<String>,
}

impl App {

    fn show_notification(&mut self, msg: String) {
        self.notification = Some(msg);
        self.notification_time = Some(Instant::now());
    }

    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0)); // Start with first row selected

        let mut theme_set = syntect::highlighting::ThemeSet::load_defaults();

        if let Some(theme_dir) = crate::config::Config::get_theme_dir() {
            let _ = theme_set.add_from_folder(&theme_dir);
        }

        let mut available_themes: Vec<String> = theme_set.themes.keys().cloned().collect();
        available_themes.sort();

        let saved_theme = crate::config::Config::load_config();

        let current_theme = if available_themes.contains(&saved_theme) {
            saved_theme // Use the user's saved preference
        } else if available_themes.contains(&"Default-Dark".to_string()) {
            "Default-Dark".to_string() // Fallback to standard default
        } else if !available_themes.is_empty() {
            available_themes[0].clone() // Fallback to first available
        } else {
            "No-Themes-Found".to_string()
        };

        let ui_colors = if let Some(theme) = theme_set.themes.get(&current_theme) {
            crate::config::UiColors::from_theme(theme)
        } else {
            // fallback if parsing fails
            crate::config::UiColors {
                bg: Color::Rgb(0,0,0), fg: Color::Rgb(255,255,255),
                menu_bg: Color::Rgb(40,40,40), selected_bg: Color::Rgb(60,60,60),
                accent: Color::Rgb(100,200,255),
                title: Color::Rgb(200, 200, 100),
                is_dark: true,
            }
        };

        let (update_tx, update_rx) = mpsc::channel();
        thread::spawn(move || {
            let current_version = env!("CARGO_PKG_VERSION");
            if let Ok(resp) = ureq::get("https://api.github.com/repos/mabognar/xtop/releases/latest")
                .set("User-Agent", "xtop-update-checker")
                .timeout(Duration::from_secs(3))
                .call()
            {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(tag) = json["tag_name"].as_str() {
                        let latest_version = tag.trim_start_matches('v');
                        if latest_version != current_version {
                            let _ = update_tx.send(latest_version.to_string());
                        }
                    }
                }
            }
        });

        Self {
            s: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            update_freq: 1000,
            table_state,
            filter_text: String::new(),
            cursor_position: 0,
            sort_col: 2,
            current_col: 2,
            reverse: false,
            editing: false,
            show_popup: false,
            process_info: 0,
            theme_set,
            available_themes,
            current_theme,
            ui_colors,
            theme_changed_time: None,
            notification: None,
            notification_time: None,
            update_rx: Some(update_rx),
            update_version: None,
        }
    }

    fn cycle_theme(&mut self) {
        if self.available_themes.is_empty() { return; }
        if let Some(current_idx) = self.available_themes.iter().position(|t| t == &self.current_theme) {
            let next_idx = (current_idx + 1) % self.available_themes.len();
            self.current_theme = self.available_themes[next_idx].clone();
        }

        crate::config::Config::save_config(&self.current_theme);

        // recalculate colors after switching
        if let Some(theme) = self.theme_set.themes.get(&self.current_theme) {
            self.ui_colors = crate::config::UiColors::from_theme(theme);
        }

        self.theme_changed_time = Some(Instant::now());
    }
}


pub fn main_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {

    loop {

        if let Some(rx) = &app.update_rx {
            if let Ok(version) = rx.try_recv() {
                app.update_version = Some(version.clone());
                app.show_notification(format!("Press u to update xtop to {}", version));
                app.update_rx = None; // Stop checking once we receive a result
            }
        }

        // refresh system data before drawing
        app.s.refresh_cpu_usage();
        app.s.refresh_memory();
        app.s.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything().without_tasks(),
        );

        // refresh network data
        app.networks.refresh(true);

        terminal.draw(|f| ui(f, app)).expect("xtop panic");

        if event::poll(Duration::from_millis(app.update_freq))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.editing {
                        false => match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('t') => app.cycle_theme(),
                            KeyCode::Char('s') => {
                                app.editing = true;
                                app.process_info = 0;
                                app.cursor_position = app.filter_text.chars().count();
                            }
                            KeyCode::Char('f') => app.table_state.select_first(),
                            KeyCode::Char('l') => app.table_state.select_last(),
                            KeyCode::Up => {
                                let i = match app.table_state.selected() {
                                    Some(i) => if i == 0 { 0 } else { i - 1 },
                                    None => 0,
                                };
                                app.table_state.select(Some(i));
                            }
                            KeyCode::Down => {
                                let count = app.s.processes().len();
                                let i = match app.table_state.selected() {
                                    Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                    None => 0,
                                };
                                app.table_state.select(Some(i));
                            }
                            KeyCode::Char('-') => {
                                if app.update_freq > 200 {
                                    app.update_freq -= 200;
                                }
                            }
                            KeyCode::Char('+') => {
                                if app.update_freq < 3000 {
                                    app.update_freq += 200;
                                }
                            }
                            KeyCode::Char('p') => {
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                    // move up (^p)
                                    let i = match app.table_state.selected() {
                                        Some(i) => if i == 0 { 0 } else { i - 1 },
                                        None => 0,
                                    };
                                    app.table_state.select(Some(i));
                                } else {
                                    // sort by pid ('p')
                                    if app.sort_col == app.current_col {
                                        app.reverse = !app.reverse;
                                    }
                                    app.sort_col = 0;
                                }
                            }
                            KeyCode::Char('n') => {
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                    // move down (^n)
                                    let count = app.s.processes().len();
                                    let i = match app.table_state.selected() {
                                        Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                        None => 0,
                                    };
                                    app.table_state.select(Some(i));
                                } else {
                                    // sort by name ('n')
                                    if app.sort_col == app.current_col {
                                        app.reverse = !app.reverse;
                                    }
                                    app.sort_col = 1;
                                }
                            }
                            KeyCode::Char('m') => {
                                if app.sort_col == app.current_col {
                                    app.reverse = !app.reverse;
                                }
                                app.sort_col = 2;
                            }
                            KeyCode::Char('c') => {
                                if app.sort_col == app.current_col {
                                    app.reverse = !app.reverse;
                                }
                                app.sort_col = 3;
                            }
                            KeyCode::Char('?') => {
                                app.show_popup = !app.show_popup;
                            }
                            KeyCode::Enter => {
                                if app.table_state.selected().is_some() {
                                    app.process_info = if app.process_info == 0 { 1 } else { 0 };
                                }
                            }
                            KeyCode::Char('u') => {
                                if app.update_version.is_some() {
                                    let _ = webbrowser::open("https://github.com/mabognar/xtop/releases/latest");
                                    app.show_notification(String::from("Opened browser for update"));
                                } else {
                                    app.show_notification(String::from("No updates available"));
                                }
                            }
                            _ => {}
                        },

                        true => match key.code {
                            KeyCode::Esc => {
                                app.editing = false;
                                app.filter_text.clear();
                                app.cursor_position = 0;
                            }
                            KeyCode::Enter => {
                                if app.table_state.selected().is_some() {
                                    app.process_info = if app.process_info == 0 { 1 } else { 0 };
                                }
                            }
                            KeyCode::Char(c) => {
                                // Handle Ctrl shortcuts
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                    if c == 'b' && app.cursor_position > 0 {
                                        app.cursor_position -= 1;
                                    } else if c == 'f' && app.cursor_position < app.filter_text.chars().count() {
                                        app.cursor_position += 1;
                                    } else if c == 'p' {
                                        // move up (^p)
                                        let i = match app.table_state.selected() {
                                            Some(i) => if i == 0 { 0 } else { i - 1 },
                                            None => 0,
                                        };
                                        app.table_state.select(Some(i));
                                    } else if c == 'n' {
                                        // move down (^n)
                                        let count = app.s.processes().len();
                                        let i = match app.table_state.selected() {
                                            Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                            None => 0,
                                        };
                                        app.table_state.select(Some(i));
                                    }
                                }
                                // standard typing
                                else if !key.modifiers.intersects(crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::ALT) {
                                    let mut chars: Vec<char> = app.filter_text.chars().collect();
                                    chars.insert(app.cursor_position, c);
                                    app.filter_text = chars.into_iter().collect();
                                    app.cursor_position += 1;

                                    if app.filter_text.is_empty() {
                                        app.process_info = 0;
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                if app.filter_text.is_empty() {
                                    app.editing = false;
                                    app.process_info = 0;
                                    app.cursor_position = 0;
                                } else if app.cursor_position > 0 {
                                    let mut chars: Vec<char> = app.filter_text.chars().collect();
                                    app.cursor_position -= 1;
                                    chars.remove(app.cursor_position);
                                    app.filter_text = chars.into_iter().collect();

                                    if app.filter_text.is_empty() {
                                        app.process_info = 0;
                                    }
                                }
                            }
                            KeyCode::Delete => {
                                if app.cursor_position < app.filter_text.chars().count() {
                                    let mut chars: Vec<char> = app.filter_text.chars().collect();
                                    chars.remove(app.cursor_position);
                                    app.filter_text = chars.into_iter().collect();

                                    if app.filter_text.is_empty() {
                                        app.process_info = 0;
                                    }
                                }
                            }
                            KeyCode::Left => {
                                if app.cursor_position > 0 {
                                    app.cursor_position -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if app.cursor_position < app.filter_text.chars().count() {
                                    app.cursor_position += 1;
                                }
                            }
                            KeyCode::Up => {
                                let i = match app.table_state.selected() {
                                    Some(i) => if i == 0 { 0 } else { i - 1 },
                                    None => 0,
                                };
                                app.table_state.select(Some(i));
                            }
                            KeyCode::Down => {
                                let count = app.s.processes().len();
                                let i = match app.table_state.selected() {
                                    Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                    None => 0,
                                };
                                app.table_state.select(Some(i));
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
    }
}

