use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyEventKind},
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
use std::time::Duration;
use ratatui::prelude::Stylize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, Users, Networks};
use default_net;
use default_net::get_default_interface;

/// Application state
struct App {
    s: System,
    networks: Networks,
    update_freq: u64,
    table_state: TableState,
    filter_text: String,
    sort_col: u8,
    current_col: u8,
    reverse: bool,
    editing: bool,
    show_popup: bool,
    process_info: u8,
}

impl App {
    fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0)); // Start with first row selected

        Self {
            s: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            update_freq: 1000,
            table_state,
            filter_text: String::new(),
            sort_col: 2,
            current_col: 2,
            reverse: false,
            editing: false,
            show_popup: false,
            process_info: 0,
        }
    }
}

fn main() -> Result<(), io::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state and run the main loop
    let mut app = App::new();
    let res = main_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn main_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        // Refresh system data before drawing
        app.s.refresh_cpu_usage();
        app.s.refresh_memory();
        app.s.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything().without_tasks(),
        );

        // Refresh network data using the persistent state
        app.networks.refresh(true);

        terminal.draw(|f| ui(f, app)).expect("xtop panic");

        // Input handling
        if event::poll(Duration::from_millis(app.update_freq))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.editing {
                        false => match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('s') => {
                                app.editing = true;
                                app.process_info = 0;
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
                                if app.sort_col == app.current_col {
                                    app.reverse = !app.reverse;
                                }
                                app.sort_col = 0;
                            }
                            KeyCode::Char('n') => {
                                if app.sort_col == app.current_col {
                                    app.reverse = !app.reverse;
                                }
                                app.sort_col = 1;
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
                            _ => {}
                        },

                        true => match key.code {
                            KeyCode::Esc => {
                                app.editing = false;
                                app.filter_text.clear();
                            }
                            KeyCode::Enter => {
                                if app.table_state.selected().is_some() {
                                    app.process_info = if app.process_info == 0 { 1 } else { 0 };
                                }
                            }
                            KeyCode::Char(c) => {
                                app.filter_text.push(c);
                                if app.filter_text.is_empty() {
                                    app.process_info = 0;
                                }
                            }
                            KeyCode::Backspace => {
                                if app.filter_text.is_empty() {
                                    app.editing = false;
                                    app.process_info = 0;
                                } else {
                                    app.filter_text.pop();
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
    // colors used in app
    let c_border = Color::Rgb(100, 150, 100);
    let c_border_search = Color::Rgb(200, 100, 100);
    let c_title = Color::Rgb(200, 200, 100);
    let c_menu = Color::Rgb(200, 200, 100);
    let c_menu_mut = Color::Rgb(200, 100, 100);
    let c_pipe = Color::Rgb(60, 60, 60);
    let c_hot_key = Color::LightRed;
    let c_table_header = Color::Rgb(200, 200, 100);
    let c_row_highlight = Color::Rgb(100, 100, 50);
    let c_mem_total = Color::Rgb(200, 200, 100);
    let c_mem_used = Color::Rgb(200, 100, 100);
    let c_mem_avail = Color::Rgb(100, 200, 100);
    let c_mem_free = Color::Rgb(50, 255, 255);
    let c_popup_border = Color::Rgb(200, 150, 100);
    let c_bg = Color::Rgb(0, 0, 0);
    let c_fg = Color::Rgb(230, 230, 230);

    // Get raw list and apply filter
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

    // Setup terminal panels
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

    // Split the screen horizontally
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(size);

    // Split the left panel vertically
    let left_panel = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(6), Constraint::Length(6)])
        .split(horizontal[0]);

    // Adapt right panel to process info
    let right_panel = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3 * if app.editing { 1 } else { 0 }),
            Constraint::Fill(1),
            Constraint::Length((app.process_info as u16) * 7),
        ])
        .split(horizontal[1]);

    // Render Search Box
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
                .title(
                    Line::from(" Type to search, escape to exit ")
                        .style(Style::default().bold()),
                ),
        )
        .bg(c_bg)
        .fg(c_fg);
    f.render_widget(search_bar, right_panel[0]);


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
                .title_bottom(
                    Line::from(vec![
                        Span::styled(" Load Ave: ", Style::default().fg(c_menu)),
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
                .unfilled_style(Style::new().fg(Color::Rgb(30, 30, 30)))
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
            .unfilled_style(Style::new().fg(Color::Rgb(30, 30, 30)))
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
            // Cell::from(Line::from(format_speed(iface_rec)).right_aligned()),
            Cell::from(Line::from(format!("{:.2} kB/s",iface_rec as f64 / 1024.0)).right_aligned()),
        ]),
        Row::new(vec![
            Cell::from("Trans: "),
            // Cell::from(Line::from(format_speed(iface_tra)).right_aligned()),
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
                .bg(c_bg)
                .fg(c_fg)
                .title_bottom(
                    Line::from(vec![
                        Span::styled(" Update (ms):", Style::default().fg(c_menu)),
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
                Span::styled("id", Style::default().fg(c_table_header)),
            ])
                .right_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("n", Style::default().fg(c_hot_key)),
                Span::styled("ame", Style::default().fg(c_table_header)),
            ])
                .left_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("m", Style::default().fg(c_hot_key)),
                Span::styled("emory", Style::default().fg(c_table_header)),
            ])
                .right_aligned()
                .style(Style::default().bold()),
            Line::from(vec![
                Span::styled("c", Style::default().fg(c_hot_key)),
                Span::styled("pu", Style::default().fg(c_table_header)),
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
                            Style::default().fg(c_menu),
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
                .title_style(c_title)
                .title_bottom(Line::from(vec![
                    Span::styled(" f", Style::default().fg(c_hot_key)),
                    Span::styled("irst", Style::default().fg(c_menu)),
                    Span::styled(" | ", Style::default().fg(c_pipe)),
                    Span::styled("l", Style::default().fg(c_hot_key)),
                    Span::styled("ast", Style::default().fg(c_menu)),
                    Span::styled(" | ", Style::default().fg(c_pipe)),
                    Span::styled("↵", Style::default().fg(c_hot_key)),
                    Span::styled("Info", Style::default().fg(c_menu)),
                    Span::styled(" | ", Style::default().fg(c_pipe)),
                    Span::styled("s", Style::default().fg(c_hot_key)),
                    Span::styled("earch", Style::default().fg(c_menu)),
                    Span::styled(" | ", Style::default().fg(c_pipe)),
                    Span::styled("q", Style::default().fg(c_hot_key)),
                    Span::styled("uit", Style::default().fg(c_menu)),
                    Span::styled(" | ", Style::default().fg(c_pipe)),
                    Span::styled("? ", Style::default().fg(c_hot_key)),
                ]))
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
                    .title_style(c_title)
                    .title_bottom(Line::from(vec![
                        Span::styled(" ↵", Style::default().fg(c_hot_key)),
                        Span::styled("Close ", Style::default().fg(c_menu)),
                    ]))
                    .bg(c_bg)
                    .fg(c_fg),
            );

        f.render_widget(selected_process_table, right_panel[2]);
    }

    // about popup
    if app.show_popup {
        let area = centered_rect(f.area());
        let help_text = vec![
            Line::from(Span::styled(" https://github.com/mabognar ", Color::White)),
            Line::from(vec![Span::styled(
                " https://crates.io/crates/xtop ",
                Color::White,
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
            .border_style(Style::default().fg(c_popup_border).bg(Color::Black))
            .bg(c_bg);

        let help_para = Paragraph::new(help_text)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(Clear, area); // This clears the area under the popup
        f.render_widget(help_para, area);
    }
}

// Helpers
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


