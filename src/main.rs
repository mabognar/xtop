mod config;
mod app;
mod ui;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Line, Style},
    style::Color,
    symbols,
    text::Span,
    widgets::{Block, BorderType, Borders, Cell, Clear, LineGauge, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};

use std::io;
use std::time::{Duration, Instant};
use ratatui::prelude::Stylize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, Users, Networks};
use default_net;
use default_net::get_default_interface;
use syntect::highlighting::ThemeSet;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use crate::config::Config;

/// Application state
struct App {
    s: System,
    networks: Networks,
    update_freq: u64,
    table_state: TableState,
    filter_text: String,
    cursor_position: usize,
    sort_col: u8,
    current_col: u8,
    reverse: bool,
    editing: bool,
    show_popup: bool,
    process_info: u8,
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

    fn new() -> Self {
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
            // Safe fallback if parsing fails
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

fn main() -> Result<(), io::Error> {
    let _ = Config::initialize_themes();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = main_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn main_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {

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
                                    // Move up (Ctrl+P)
                                    let i = match app.table_state.selected() {
                                        Some(i) => if i == 0 { 0 } else { i - 1 },
                                        None => 0,
                                    };
                                    app.table_state.select(Some(i));
                                } else {
                                    // Sort by PID ('p')
                                    if app.sort_col == app.current_col {
                                        app.reverse = !app.reverse;
                                    }
                                    app.sort_col = 0;
                                }
                            }
                            KeyCode::Char('n') => {
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                    // Move down (Ctrl+N)
                                    let count = app.s.processes().len();
                                    let i = match app.table_state.selected() {
                                        Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                        None => 0,
                                    };
                                    app.table_state.select(Some(i));
                                } else {
                                    // Sort by Name ('n')
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
                                        // Move up (Ctrl+P)
                                        let i = match app.table_state.selected() {
                                            Some(i) => if i == 0 { 0 } else { i - 1 },
                                            None => 0,
                                        };
                                        app.table_state.select(Some(i));
                                    } else if c == 'n' {
                                        // Move down (Ctrl+N)
                                        let count = app.s.processes().len();
                                        let i = match app.table_state.selected() {
                                            Some(i) => if i >= count - 1 { count - 1 } else { i + 1 },
                                            None => 0,
                                        };
                                        app.table_state.select(Some(i));
                                    }
                                }
                                // Standard typing
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

fn ui(f: &mut Frame, app: &mut App) {
    let colors = app.ui_colors;

    // map colors
    let c_bg = colors.bg;
    let c_fg = colors.fg;
    let c_border = colors.accent;

    let c_pipe = match colors.menu_bg {
        Color::Rgb(r, g, b) => {
            if colors.is_dark {
                // For dark themes, make the pipe slightly lighter than the menu background
                Color::Rgb(r.saturating_add(40), g.saturating_add(40), b.saturating_add(40))
            } else {
                // For light themes, make the pipe slightly darker than the menu background
                Color::Rgb(r.saturating_sub(40), g.saturating_sub(40), b.saturating_sub(40))
            }
        }
        _ => Color::DarkGray, // Fallback just in case
    };
    let c_row_highlight = colors.selected_bg;
    let c_border_search = colors.accent;
    let c_title = colors.title;
    let c_menu_mut = colors.accent;
    let c_hot_key = colors.accent;
    let c_table_header = colors.accent;
    let c_popup_border = colors.accent;
    let c_menu = colors.fg; // normal text
    let c_mem_total = Color::Rgb(200, 200, 100);
    let c_mem_used = Color::Rgb(200, 100, 100);
    let c_mem_avail = Color::Rgb(100, 200, 100);
    let c_mem_free = Color::Rgb(50, 255, 255);

    let mut process_list: Vec<_> = app.s.processes().values().collect();
    if !app.filter_text.is_empty() {
        process_list.retain(|p| {
            p.pid().to_string().to_lowercase().contains(&app.filter_text.to_lowercase())
                || p.name().to_string_lossy().to_lowercase().contains(&app.filter_text.to_lowercase())
        });
        if process_list.is_empty() {
            app.process_info = 0;
        }
    }

    // setup terminal
    let size = f.area();
    let terminal_width = size.width;
    let terminal_height = size.height;

    if terminal_width < 66 || terminal_height < 20 {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(size);
        let p = Paragraph::new("Terminal size must be at least\n 66 x 20\n to display 'xtop'").centered();
        f.render_widget(p, horizontal[0]);
        return;
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(size);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_layout[0]); // Notice we split main_layout[0] now, not size

    let left_panel = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(6), Constraint::Length(6)])
        .split(horizontal[0]);

    let right_panel = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3 * if app.editing { 1 } else { 0 }),
            Constraint::Fill(1),
            Constraint::Length((app.process_info as u16) * 7),
        ])
        .split(horizontal[1]);

    // search Box
    let search_style = match app.editing {
        false => Style::default(),
        true => Style::default().fg(Color::White),
    };

    let search_bar = Paragraph::new(app.filter_text.as_str())
        .style(search_style)
        .block(
            Block::default()
                .title_style(c_title)
                .borders(Borders::ALL)
                .border_style(c_border_search)
                .border_type(BorderType::Rounded)
                .title(
                    Line::from(" Type to search, escape to exit ")
                        .style(Style::default().bold()),
                ),
        )
        .bg(c_bg)
        .fg(c_fg);
    f.render_widget(search_bar, right_panel[0]);

    if app.editing {
        let inner_width = right_panel[0].width.saturating_sub(2);
        let cursor_x = (app.cursor_position as u16).min(inner_width);

        f.set_cursor_position((
            right_panel[0].x + 1 + cursor_x,
            right_panel[0].y + 1,
        ));
    }


    ////////////////////////////////////////////////////////////////////////////////////////
    // CPU
    let mut rows = Vec::new();
    for cpu in app.s.cpus().iter() {
        rows.push(Row::new(vec![
            Cell::from(Line::from(cpu.name().to_string()).right_aligned()),
            Cell::from(Line::from(format!("{:.1}%", cpu.cpu_usage())).right_aligned()),
        ]));
    }

    let loadavg = System::load_average();

    let table = Table::new(
        rows,
        [Constraint::Length(6), Constraint::Length(7)],
    )
        .block(Block::default().borders(Borders::ALL))
        .header(
            Row::new(vec![
                Cell::from(Line::from("CPU").right_aligned()),
                Cell::from(Line::from("Usage").right_aligned()),
            ])
                .style(Style::default().bold().fg(c_table_header)),
        )
        .column_spacing(0)
        .block(
            Block::default()
                .title(Line::from(" Core Information ").style(Style::default().bold()))
                .title_style(c_title)
                .borders(Borders::ALL)
                .border_style(c_border)
                .border_type(BorderType::Rounded)
                .title_bottom(
                    Line::from(vec![
                        Span::styled(" Load Ave: ", Style::default().fg(colors.accent)),
                        Span::styled(
                            format!("{:.2} {:.2} {:.2} ", loadavg.one, loadavg.five, loadavg.fifteen),
                            Style::default().fg(c_menu_mut),
                        ),
                    ])
                        .right_aligned(),
                )
                .bg(c_bg)
                .fg(c_fg),
        );

    f.render_widget(table, left_panel[0]);

    // cpu gauge
    let mut area_vec = vec![];
    for i in 1..=app.s.cpus().len() {
        area_vec.push(Rect::new(15, (i + 1) as u16, left_panel[0].width - 17, 1));

        if (i as u16) < (left_panel[0].height - 2) {
            let cpuusage = app.s.cpus().get(i - 1).unwrap().cpu_usage();
            let gauge = LineGauge::default()
                .label("")
                .filled_style(Style::new().fg(Color::Rgb(
                    (cpuusage * 255.0 / 100.0) as u8,
                    ((100.0 - cpuusage) * 255.0 / 100.0) as u8,
                    0,
                )))
                .unfilled_style(Style::new().fg(colors.menu_bg))
                .filled_symbol(symbols::line::THICK_HORIZONTAL)
                .ratio((cpuusage / 100.0) as f64);
            f.render_widget(&gauge, area_vec[i - 1]);
        }
    }


    ////////////////////////////////////////////////////////////////////////////////////////
    // Memory
    let mem_rows = vec![
        Row::new(vec![
            Cell::from("Total: "),
            Cell::from(
                Line::from(format!(
                    "{:.1}",
                    (app.s.total_memory() as f32) / (1024.0f32.powi(3))
                ))
                    .right_aligned(),
            ),
        ]),
        Row::new(vec![
            Cell::from("Used: "),
            Cell::from(
                Line::from(format!(
                    "{:.1}",
                    (app.s.used_memory() as f32) / (1024.0f32.powi(3))
                ))
                    .right_aligned(),
            ),
        ]),
        Row::new(vec![
            Cell::from("Avail: "),
            Cell::from(
                Line::from(format!(
                    "{:.1}",
                    (app.s.available_memory() as f32) / (1024.0f32.powi(3))
                ))
                    .right_aligned(),
            ),
        ]),
        Row::new(vec![
            Cell::from("Free: "),
            Cell::from(
                Line::from(format!(
                    "{:.1}",
                    (app.s.free_memory() as f32) / (1024.0f32.powi(3))
                ))
                    .right_aligned(),
            ),
        ]),
    ];

    let mut memory_vec = vec![];
    memory_vec.push(app.s.total_memory() as f64);
    memory_vec.push(app.s.used_memory() as f64);
    memory_vec.push(app.s.available_memory() as f64);
    memory_vec.push(app.s.free_memory() as f64);

    let mem_table = Table::new(mem_rows, [Constraint::Length(6), Constraint::Length(5)])
        .block(Block::default().borders(Borders::ALL))
        .column_spacing(1)
        .block(
            Block::default()
                .title(Line::from(" Memory (GB) ").style(Style::default().bold()))
                .title_style(c_title)
                .borders(Borders::ALL)
                .border_style(c_border)
                .border_type(BorderType::Rounded)
                .bg(c_bg)
                .fg(c_fg),
        );

    f.render_widget(mem_table, left_panel[1]);

    // memory gauge
    let color_memory = vec![c_mem_total, c_mem_used, c_mem_avail, c_mem_free];
    let mut area_vec = vec![];
    for i in 0..4 {
        area_vec.push(Rect::new(
            14,
            left_panel[1].y + (i + 1) as u16,
            left_panel[1].width - 16,
            1,
        ));

        let gauge = LineGauge::default()
            .label("")
            .filled_style(Style::new().fg(color_memory[i]))
            .unfilled_style(Style::new().fg(colors.menu_bg))
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .ratio(memory_vec[i] / memory_vec[0]);
        f.render_widget(&gauge, area_vec[i]);
    }


    ////////////////////////////////////////////////////////////////////////////////////////
    // Network
    let (iface_name, iface_ip) = match get_default_interface() {
        Ok(iface) => (iface.name, format!("{:?}", iface.ipv4)),
        Err(_) => ("Unknown".to_string(), "Unknown".to_string()),
    };

    let mut iface_rec = 0;
    let mut iface_tra = 0;

    // Use the networks data stored in app state
    if let Some(net_data) = app.networks.get(&iface_name) {
        // Calculate per second rate based on update frequency
        let freq_multiplier = 1000.0 / (app.update_freq as f64);
        iface_rec = (net_data.received() as f64 * freq_multiplier) as u64;
        iface_tra = (net_data.transmitted() as f64 * freq_multiplier) as u64;
    }

    let net_rows = vec![
        Row::new(vec![
            Cell::from("Inter: "),
            Cell::from(Line::from(iface_name).right_aligned()),
        ]),
        Row::new(vec![
            Cell::from("IPv4: "),
            Cell::from(Line::from(iface_ip).right_aligned()),
        ]),
        Row::new(vec![
            Cell::from("Rcvd: "),
            Cell::from(Line::from(format!("{:.2} kB/s",iface_rec as f64 / 1024.0)).right_aligned()),
        ]),
        Row::new(vec![
            Cell::from("Trans: "),
            Cell::from(Line::from(format!("{:.2} kB/s",iface_tra as f64 / 1024.0)).right_aligned()),
        ]),
    ];

    let net_table = Table::new(
        net_rows,
        [Constraint::Length(13), Constraint::Min(20)],
    )
        .block(Block::default().borders(Borders::ALL))
        .column_spacing(1)
        .block(
            Block::default()
                .title(Line::from(" Network ").style(Style::default().bold()))
                .title_style(c_title)
                .borders(Borders::ALL)
                .border_style(c_border)
                .border_type(BorderType::Rounded)
                .bg(c_bg)
                .fg(c_fg)
                .title_bottom(
                    Line::from(vec![
                        Span::styled(" Update (ms):", Style::default().fg(colors.accent)),
                        Span::styled(" - ", Style::default().fg(c_hot_key)),
                        Span::styled(
                            format!("{:.0}", app.update_freq),
                            Style::default().fg(c_menu_mut),
                        ),
                        Span::styled(" + ", Style::default().fg(c_hot_key)),
                    ])
                        .right_aligned(),
                )
        );

    f.render_widget(net_table, left_panel[2]);


    ////////////////////////////////////////////////////////////////////////////////////////
    // Process List
    if app.sort_col != app.current_col {
        app.reverse = false;
    }

    match app.sort_col {
        0 => {
            process_list.sort_by(|a, b| a.pid().cmp(&b.pid()).reverse());
            app.current_col = 0;
        }
        1 => {
            process_list.sort_by(|a, b| a.name().cmp(&b.name()));
            app.current_col = 1;
        }
        2 => {
            process_list.sort_by(|a, b| a.memory().cmp(&b.memory()).reverse());
            app.current_col = 2;
        }
        3 => {
            process_list.sort_by(|a, b| a.cpu_usage().total_cmp(&b.cpu_usage()).reverse());
            app.current_col = 3;
        }
        _ => {}
    }

    if app.reverse {
        process_list.reverse();
    }

    let uptime_secs: u64 = System::uptime();
    let d = uptime_secs / 86400;
    let h = (uptime_secs / 3600) % 24;
    let m = (uptime_secs / 60) % 60;
    let s = uptime_secs % 60;

    let proc_rows: Vec<Row> = process_list
        .iter()
        .map(|p| {
            Row::new(vec![
                Cell::from(Line::from(p.pid().to_string()).right_aligned()),
                Cell::from(p.name().to_string_lossy().to_string()),
                Cell::from(Line::from(format!("{:.1} MB", p.memory() as f64 / 1_048_576.0)).right_aligned()),
                Cell::from(Line::from(format!("{:.1}%", p.cpu_usage())).right_aligned()),
            ])
        })
        .collect();

    let nrows = proc_rows.len();
    let mut srow = app.table_state.selected().unwrap_or(0);
    if nrows == 0 {
        srow = 0;
    }
    if srow < nrows {
        srow = srow + 1;
    }

    let proc_table = Table::new(
        proc_rows,
        [
            Constraint::Length(7),
            Constraint::Min(12),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
        .header(Row::new(vec![
            Line::from(vec![
                Span::styled("p", Style::default().fg(c_hot_key)),
                Span::styled("id", Style::default().fg(c_menu)),
            ])
                .right_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("n", Style::default().fg(c_hot_key)),
                Span::styled("ame", Style::default().fg(c_menu)),
            ])
                .left_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("m", Style::default().fg(c_hot_key)),
                Span::styled("emory", Style::default().fg(c_menu)),
            ])
                .right_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("c", Style::default().fg(c_hot_key)),
                Span::styled("pu", Style::default().fg(c_menu)),
            ])
                .right_aligned()
                .style(Style::default().bold()),
        ]))
        .row_highlight_style(Style::default().bg(c_row_highlight))
        .block(
            Block::default()
                .title(
                    Line::from(format!(" Processes [{}/{}] ", srow, nrows))
                        .style(Style::default().bold())
                        .left_aligned(),
                )
                .title(
                    Line::from(vec![
                        Span::styled(
                            if f.area().width >= 70 { " Uptime:" } else { "" },
                            Style::default().fg(colors.accent),
                        ),
                        Span::styled(
                            format!(" {:01}d {:02}:{:02}:{:02} ", d, h, m, s),
                            Style::default().fg(c_menu_mut),
                        ),
                    ])
                        .right_aligned(),
                )
                .borders(Borders::ALL)
                .border_style(c_border)
                .border_type(BorderType::Rounded)
                .title_style(c_title)
                .bg(c_bg)
                .fg(c_fg),
        );

    f.render_stateful_widget(proc_table, right_panel[1], &mut app.table_state);


    ////////////////////////////////////////////////////////////////////////////////////////
    // Process Details

    if app.table_state.selected().is_none() && app.process_info == 1 {
        app.process_info = 0;
    }

    if app.table_state.selected().is_some() && app.process_info == 1 {
        let users = Users::new_with_refreshed_list();

        let selected_process = app.table_state.selected().unwrap();
        let (hours, minutes, seconds) = s_to_hms(process_list[selected_process].run_time());

        let pid = process_list[selected_process].pid();
        let user_name = process_list[selected_process]
            .user_id()
            .and_then(|uid| users.get_user_by_id(uid))
            .map(|user| user.name())
            .unwrap_or("Unknown");

        let path = match process_list[selected_process].exe() {
            Some(p) => p.to_str().unwrap_or("Unknown"),
            None => "Unknown",
        };

        let selected_processes_rows = vec![
            Row::new(vec![Cell::from("PID: "), Cell::from(pid.to_string())]),
            Row::new(vec![Cell::from("User Name: "), Cell::from(user_name)]),
            Row::new(vec![Cell::from("Path: "), Cell::from(path)]),
            Row::new(vec![
                Cell::from("Command: "),
                Cell::from(format!("{:?}", process_list[selected_process].cmd())),
            ]),
            Row::new(vec![
                Cell::from("Run Time: "),
                Cell::from(format!("{:?}:{:02}:{:02}", hours, minutes, seconds)),
            ]),
        ];

        let selected_process_table = Table::new(
            selected_processes_rows,
            [Constraint::Fill(1), Constraint::Fill(3)],
        )
            .row_highlight_style(Style::default().bg(Color::Rgb(100, 100, 50))) // Visual cue for selection
            .block(
                Block::default()
                    .title(Line::from(" Process Details ").style(Style::default().bold()))
                    .borders(Borders::ALL)
                    .border_style(c_border)
                    .border_type(BorderType::Rounded)
                    .title_style(c_title)
                    .title_bottom(Line::from(vec![
                        Span::styled(" ↵ ", Style::default().fg(c_hot_key)),
                        Span::styled("Close ", Style::default().fg(c_menu)),
                    ]))
                    .bg(c_bg)
                    .fg(c_fg),
            );

        f.render_widget(selected_process_table, right_panel[2]);
    }


    // bottom menu
    let menu_layout = Layout::default()
        .direction(Direction::Horizontal)
        // Let the left side expand infinitely, pinning the right side to exactly 42 characters
        .constraints([Constraint::Fill(1), Constraint::Length(42)])
        .split(main_layout[1]);

    let show_theme_name = match app.theme_changed_time {
        Some(time) => time.elapsed().as_secs() < 3,
        None => false,
    };

    let show_notification = match app.notification_time {
        Some(time) => time.elapsed().as_secs() < 3,
        None => false,
    };

    let mut left_menu_spans = Vec::new();
    if show_notification {
        if let Some(ref msg) = app.notification {
            // A quick and easy split trick right in the UI layer
            let parts: Vec<&str> = msg.split(" u ").collect();
            if parts.len() == 2 {
                left_menu_spans.push(Span::styled(format!(" {}", parts[0]), Style::default().fg(c_menu).bold()));
                left_menu_spans.push(Span::styled(" u ", Style::default().fg(c_hot_key).bold()));
                left_menu_spans.push(Span::styled(format!("{} ", parts[1]), Style::default().fg(c_menu).bold()));
            } else {
                left_menu_spans.push(Span::styled(format!(" {} ", msg), Style::default().fg(c_menu).bold()));
            }
        }
    } else if show_theme_name {
        left_menu_spans.push(Span::styled(format!(" {} ", app.current_theme), Style::default().fg(c_menu)));
    } else {
        left_menu_spans.push(Span::raw(" "));
    }

    let left_menu = Paragraph::new(Line::from(left_menu_spans))
        .block(Block::default().bg(colors.menu_bg));

    let right_menu = Paragraph::new(Line::from(vec![
        Span::styled("↵", Style::default().fg(c_hot_key)), Span::styled(" Info ", Style::default().fg(c_menu)), Span::styled("| ", Style::default().fg(c_pipe)),
        Span::styled("s", Style::default().fg(c_hot_key)), Span::styled("earch ", Style::default().fg(c_menu)), Span::styled("| ", Style::default().fg(c_pipe)),
        Span::styled("t", Style::default().fg(c_hot_key)), Span::styled("heme ", Style::default().fg(c_menu)), Span::styled("| ", Style::default().fg(c_pipe)),
        Span::styled("q", Style::default().fg(c_hot_key)), Span::styled("uit ", Style::default().fg(c_menu)), Span::styled("| ", Style::default().fg(c_pipe)),
        Span::styled("? ", Style::default().fg(c_hot_key)), Span::styled("", Style::default().fg(c_menu)),
    ]))
        .alignment(ratatui::layout::Alignment::Right)
        .bg(colors.menu_bg); // Match the left side background

    f.render_widget(left_menu, menu_layout[0]);
    f.render_widget(right_menu, menu_layout[1]);

    // about popup
    if app.show_popup {
        let area = centered_rect(f.area());
        let help_text = vec![
            Line::from(Span::styled(" https://github.com/mabognar ", colors.fg)),
            Line::from(vec![Span::styled(
                " https://crates.io/crates/xtop ",
                colors.fg,
            )]),
        ];

        const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
        let block = Block::default()
            .title(Line::from(vec![
                Span::raw(" xtop "),
                Span::raw(format!("({}) ", PKG_VERSION)),
            ]))
            .title_bottom(Line::from(vec![
                Span::raw(" To close, type "),
                Span::styled("? ", c_hot_key),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(c_popup_border).bg(colors.bg))
            .bg(c_bg);

        let help_para = Paragraph::new(help_text)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(Clear, area); // This clears the area under the popup
        f.render_widget(help_para, area);
    }
}

fn centered_rect(r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(4),
        Constraint::Fill(1),
    ])
        .split(r);

    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(33),
        Constraint::Fill(1),
    ])
        .split(popup_layout[1])[1]
}

fn s_to_hms(secs: u64) -> (u64, u64, u64) {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    (h, m, s)
}
