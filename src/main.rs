use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{backend::CrosstermBackend,
              layout::{Constraint, Direction, Layout},
              symbols,
              widgets::{Block, Table, Cell, Row, TableState, Borders},
              Terminal};
use ratatui::layout::Rect;
use ratatui::prelude::{Line, Style};
use ratatui::style::{Color};
use ratatui::widgets::{LineGauge, Paragraph};

use std::{io};
use std::time::Duration;
use sysinfo::{System, ProcessesToUpdate, ProcessRefreshKind, Users};


fn main() -> Result<(), io::Error> {

    // let mut components = Components::new_with_refreshed_list();
    // components.refresh(true);

    // let mut networks = Networks::new_with_refreshed_list();

    let mut s = System::new_all();
    // s.refresh_processes(ProcessesToUpdate::All, true);

    let mut update_freq = 1000;

    // Table state
    let mut table_state = TableState::default();
    table_state.select(Some(0)); // Start with the first row selected

    // Search state
    let mut filter_text = String::new();

    // Track processes for sorting
    let mut sort_col = 2;
    let mut current_col = 2;
    let mut reverse = false;

    let mut right_panel_rows= 0;

    let mut editing = false;
    let mut edit_integer = 0;


    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut process_info = 0;


    // Run the render loop
    loop {

        terminal.draw(|f| {

            // s.refresh_all();
            s.refresh_cpu_usage();
            s.refresh_memory();
            s.refresh_processes_specifics(ProcessesToUpdate::All, true,
                                          ProcessRefreshKind::everything().without_tasks());

            // Get raw list and apply filter
            let mut process_list: Vec<_> = s.processes().values().collect();
            if !filter_text.is_empty() {
                process_list.retain(|p| {
                    p.pid().to_string().to_lowercase().contains(&filter_text.to_lowercase()) ||
                    p.name().to_string_lossy().to_lowercase().contains(&filter_text.to_lowercase())
                });
                if process_list.is_empty() {
                    process_info = 0;
                }
            }

            // Setup terminal panels
            let size = f.area();

            let terminal_width = size.width;
            let terminal_height = size.height;
            let mut terminal_size_ok = true;
            if terminal_width < 60 || terminal_height < 16 {
                terminal_size_ok = false;
            }

            if terminal_size_ok {

                // Split the screen horizontally
                let horizontal = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(size);

                // Split the left panel vertically
                let left_panel = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Fill(1), Constraint::Length(6), Constraint::Length(0)])
                    .split(horizontal[0]);

                // Adapt right panel to process info
                match editing {
                    true => { edit_integer = 1},
                    false => { edit_integer = 0}
                }
                let right_panel = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3* edit_integer),
                        Constraint::Fill(1), Constraint::Length((process_info as u16)*8)])
                    .split(horizontal[1]);

                // Render Search Box
                let search_style = match editing {
                    false => Style::default(),
                    true => Style::default().fg(Color::White),
                };

                let search_bar = Paragraph::new(filter_text.as_str())
                    .style(search_style)
                    .block(Block::default()
                        .title_style(Color::Rgb(200,200,100))
                        .borders(Borders::ALL)
                        .border_style(Color::Rgb(200,100,100))
                        .title(Line::from(" Type to search, escape to exit ").style(Style::default().bold())));
                f.render_widget(search_bar, right_panel[0]);



                /////////
                // CPU //
                /////////
                let mut rows = Vec::new();
                for cpu in s.cpus().iter() {
                    rows.push(Row::new(vec![
                        Cell::from(Line::from(cpu.name().to_string()).right_aligned()),
                        Cell::from(Line::from(format!("{:.1}%", cpu.cpu_usage())).right_aligned()),
                    ]));
                }

                // let rows: Vec<Row> = s.cpus().iter().map(|cpu| {
                //     Row::new(vec![
                //         Cell::from(Line::from(cpu.name().to_string()).right_aligned()),
                //         Cell::from(Line::from(format!("{:.1}%", cpu.cpu_usage())).right_aligned()),
                //     ])
                // }).collect();

                let loadavg = System::load_average();

                let table = Table::new(rows,[Constraint::Length(5), Constraint::Length(6),
                    Constraint::Length(6), Constraint::Length(10)])
                    .block(Block::default().borders(Borders::ALL))
                    .header(Row::new(vec![
                        Cell::from(Line::from("CPU").right_aligned()),
                        Cell::from(Line::from("Usage").right_aligned())])
                        .style(Style::default().bold().white()))
                    .column_spacing(1)
                    .block(Block::default().title(Line::from(" Core Information ").style(Style::default().bold()))
                        .title_style(Color::Rgb(200,200,100))
                        .borders(Borders::ALL).border_style(Color::Rgb(150,150,100))
                        .title_bottom(Line::from(format!(" Load Ave: {:.2} {:.2} {:.2} ",
                                                         loadavg.one, loadavg.five, loadavg.fifteen))
                            .right_aligned().style(Style::default().fg(Color::Rgb(200,100,100)))))
                    ;

                f.render_widget(table, left_panel[0]);

                // cpu gauge
                let mut area_vec = vec![];
                for i in 1..=s.cpus().len() {

                    area_vec.push(Rect::new(14, (i+1) as u16, left_panel[0].width-16, 1));

                    if (i as u16) < (left_panel[0].height) {
                        let cpuusage = s.cpus().get(i-1).unwrap().cpu_usage();
                        let gauge = LineGauge::default()
                            // .block(Block::bordered().title("Progress"))
                            .label("")
                            .filled_style(Style::new()
                                .fg(Color::Rgb((cpuusage * 255.0/100.0) as u8, ((100.0-cpuusage) * 255.0/100.0) as u8, 0)))
                            .unfilled_style(Style::new().fg(Color::Rgb(30,30,30)))
                            .filled_symbol(symbols::line::THICK_HORIZONTAL)
                            .ratio((cpuusage/100.0) as f64);
                        f.render_widget(&gauge, area_vec[i-1]);
                    }
                }



                ////////////
                // Memory //
                ////////////
                let rows = vec!{
                    Row::new(vec![
                        Cell::from("Total: "),
                        Cell::from(Line::from(format!("{:.1}",
                                                      (s.total_memory() as f32) / (1024.0 * 1024.0 * 1024.0))).right_aligned())]),
                    Row::new(vec![
                        Cell::from("Used: "),
                        Cell::from(Line::from(format!("{:.1}",
                                                      (s.used_memory() as f32) / (1024.0 * 1024.0 * 1024.0)),
                        ).right_aligned())]),
                    Row::new(vec![
                        Cell::from("Avail: "),
                        Cell::from(Line::from(format!("{:.1}",
                                                      (s.available_memory() as f32) / (1024.0 * 1024.0 * 1024.0)),
                        ).right_aligned())]),
                    Row::new(vec![
                        Cell::from("Free: "),
                        Cell::from(Line::from(format!("{:.1}",
                                                      (s.free_memory() as f32) / (1024.0 * 1024.0 * 1024.0)),
                        ).right_aligned())]),
                };

                let mut memory_vec = vec![];
                memory_vec.push(s.total_memory() as f64);
                memory_vec.push(s.used_memory() as f64);
                memory_vec.push(s.available_memory() as f64);
                memory_vec.push(s.free_memory() as f64);

                let table = Table::new(rows,[Constraint::Length(6), Constraint::Length(5)])
                    .block(Block::default().borders(Borders::ALL))
                    .column_spacing(1)
                    .block(Block::default().title(Line::from(" Memory (GB) ").style(Style::default().bold()))
                        .title_style(Color::Rgb(200,200,100))
                        .borders(Borders::ALL).border_style(Color::Rgb(150,150,100))
                        .title_style(Color::Rgb(200,200,100))
                        .title_bottom(Line::from(format!(" Update (ms): (-) {:.0} (+) ", update_freq))
                            .right_aligned().style(Style::default().fg(Color::Rgb(200,100,100)))));

                f.render_widget(table, left_panel[1]);


                // memory gauge
                let color_memory = vec![Color::Rgb(200,200,100),Color::Rgb(200,100,100),
                                        Color::Rgb(100,200,100),Color::Rgb(50,255,255)];
                let mut area_vec = vec![];
                for i in 0..4 {
                    area_vec.push(Rect::new(14, left_panel[1].y + (i+1) as u16, left_panel[1].width-16, 1));

                    let gauge = LineGauge::default()
                        // .block(Block::bordered().title("Progress"))
                        .label("")
                        .filled_style(Style::new()
                            .fg(color_memory[i]))
                        .unfilled_style(Style::new().fg(Color::Rgb(30,30,30)))
                        .filled_symbol(symbols::line::THICK_HORIZONTAL)
                        .ratio(memory_vec[i]/memory_vec[0]); // Sets 42% progress
                    f.render_widget(&gauge, area_vec[i]);
                }



                ////////////////
                // Networking //
                ////////////////
                // networks.refresh(true);
                //
                // let up_kbps = (networks.get("en0").unwrap().transmitted() as u64) / ((update_freq as u64) /
                // (1000u64) * (1000u64)) as u64;
                // let dn_kbps = (networks.get("en0").unwrap().received() as u64) / ((update_freq as u64) /
                // (1000u64) * (1000u64)) as u64;
                //
                // let rows = vec!{
                //     Row::new(vec![
                //         Cell::from("Up: "),
                //         Cell::from(Line::from(format!("{:.1}",
                //                                       (networks.get("en0").unwrap().transmitted() as f64) /
                // ((update_freq as f64) / (1000f64) * (1000f64)))).right_aligned())]),
                //     Row::new(vec![
                //         Cell::from("Dn: "),
                //         Cell::from(Line::from(format!("{:.1}",
                //                                       (networks.get("en0").unwrap().received() as f64) /
                // ((update_freq as f64) / (1000f64) * (1000f64))),
                //         ).right_aligned())]),
                // };
                //
                // let table = Table::new(rows,[Constraint::Length(3), Constraint::Length(8)])
                //     .block(Block::default().borders(Borders::ALL))
                //     .column_spacing(1)
                //     .block(Block::default().title(Line::from(" Network (kbps) ").style(Style::default().bold()))
                //         .title_style(Color::Rgb(200,200,100))
                //         .borders(Borders::ALL).border_style(Color::Rgb(150,150,100))
                //         .title_style(Color::Rgb(200,200,100)));
                //
                // f.render_widget(table, left_panel[2]);
                //
                // sparkline_vec_up.insert(0,up_kbps);
                // sparkline_vec_dn.insert(0, dn_kbps);
                //
                // if sparkline_vec_up.len() > 100 {
                //      sparkline_vec_up.pop();
                //      sparkline_vec_dn.pop();
                // }
                //
                // let mut sparkline_area_vec = vec![];
                // for i in 0..1 {
                //     sparkline_area_vec.push(Rect::new(14, left_panel[2].y + (i+1) as u16, left_panel[2].width-16, 6));
                //
                //     let gauge = Sparkline::default()
                //         // .block(Block::bordered().title("Sparkline"))
                //         .data(&sparkline_vec_dn)
                //         .max(100)
                //         .direction(RenderDirection::LeftToRight)
                //         .style(Style::default().green())
                //         .absent_value_style(Style::default().fg(Color::Red))
                //         .absent_value_symbol(symbols::shade::FULL);
                //
                //     f.render_widget(&gauge, sparkline_area_vec[i]);
                // }



                //////////////////
                // Process List //
                //////////////////
                if sort_col != current_col {
                    reverse = false;
                }

                match sort_col {
                    0 => {
                        process_list.sort_by(|a, b| a.pid().cmp(&b.pid()).reverse());
                        current_col = 0;
                    },
                    1 => {
                        process_list.sort_by(|a, b| a.name().cmp(&b.name()));
                        current_col = 1;
                    },
                    2 => {
                        process_list.sort_by(|a, b| a.memory().cmp(&b.memory()).reverse());
                        current_col = 2;
                    },
                    3 => {
                        process_list.sort_by(|a, b| a.cpu_usage().total_cmp(&b.cpu_usage()).reverse());
                        current_col = 3;
                    },
                    _ => {}
                }

                if reverse {
                    process_list.reverse();
                }

                let rows: Vec<Row> = process_list.iter().map(|p| {
                    Row::new(vec![
                        Cell::from(Line::from(p.pid().to_string()).right_aligned()),
                        Cell::from(p.name().to_string_lossy().to_string()),
                        Cell::from(Line::from(format!("{:.1} MB", p.memory() as f64 / 1_048_576.0)).right_aligned()),
                        Cell::from(Line::from(format!("{:.1}%", p.cpu_usage())).right_aligned()),
                    ])
                }).collect();

                let table = Table::new(
                    rows,
                    [Constraint::Length(6), Constraint::Min(18), Constraint::Length(10), Constraint::Length(6)])
                    .header(Row::new(vec![" (p)id", "(n)ame", "  (m)emory", " (c)pu"])
                        .style(Style::default().bold().white()))
                    .row_highlight_style(Style::default().bg(Color::Rgb(100,100,50)))
                    .block(Block::default().title(Line::from(" System Processes ").style(Style::default().bold()))
                        .borders(Borders::ALL)
                        .border_style(Color::Rgb(150,150,100))
                        .title_style(Color::Rgb(200,200,100))
                        .title_bottom(Line::from(" (f)irst (l)ast (↵)Info (s)earch (q)uit ")
                            .style(Style::default().fg(Color::Rgb(200,100,100))))
                    );

                right_panel_rows = right_panel[1].height - 3;

                f.render_stateful_widget(table, right_panel[1], &mut table_state);



                /////////////////////
                // Process Details //
                /////////////////////
                if table_state.selected().is_none() && process_info == 1 {
                    process_info = 0;
                }

                if table_state.selected().is_some() && process_info == 1 {

                    match editing {
                        true => { edit_integer = 1},
                        false => { edit_integer = 0}
                    }

                    let users = Users::new_with_refreshed_list();

                    let selected_process = table_state.selected().unwrap();
                    let (hours, minutes, seconds) = s_to_hms(process_list[selected_process].run_time());

                    let pid = process_list[selected_process].pid();
                    let user_name = process_list[selected_process].user_id()
                        .and_then(|uid| users.get_user_by_id(uid))
                        .map(|user| user.name())
                        .unwrap_or("Unknown");

                    let path;
                    match process_list[selected_process].exe() {
                        Some(_path) => {
                            path = process_list[selected_process].exe().unwrap().to_str().unwrap();
                        }
                        None => {
                            path = "Unknown";
                        }
                    }

                    let selected_processes_rows = vec!{
                        Row::new(vec![
                            Cell::from("PID: "), Cell::from(pid.to_string()),]),
                        Row::new(vec![
                            Cell::from("User Name: "), Cell::from(user_name),]),
                        Row::new(vec![
                            Cell::from("Path: "), Cell::from(path),]),
                        Row::new(vec![
                            Cell::from("Command: "), Cell::from(format!("{:?}",
                                                                        process_list[selected_process].cmd())),]),
                        Row::new(vec![
                            Cell::from("Virt Mem: "), Cell::from(format!("{:.2} MB",
                                                                         process_list[selected_process].virtual_memory() as f64 /
                                                                             1048576.0)),]),
                        Row::new(vec![
                            Cell::from("Run Time: "), Cell::from(format!("{:?}:{:02}:{:02}",
                                                                         hours,minutes,seconds)),]),
                    };

                    let selected_process_table = Table::new(
                        selected_processes_rows,
                        [Constraint::Fill(1), Constraint::Fill(3)])
                        .row_highlight_style(Style::default().bg(Color::Rgb(100,100,50))) // Visual cue for selection
                        .block(Block::default().title(Line::from(" Process Details ").style(Style::default().bold()))
                            .borders(Borders::ALL)
                            .border_style(Color::Rgb(150,150,100))
                            .title_style(Color::Rgb(200,200,100))
                            .title_bottom(Line::from(" (↵) Close ").style(Style::default().fg(Color::Rgb(200,100,100)))));

                    right_panel_rows = right_panel[1].height -3;

                    f.render_widget(selected_process_table, right_panel[2]);

                }

            } // terminal_size_ok
            else { // terminal too small
                let horizontal = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(100)])
                    .split(size);
                let p = Paragraph::new("Terminal size must be at least\n 60 x 16\n to display 'xtop'").centered();
                f.render_widget(p, horizontal[0]);
            }
        })?;




        /////////////////////////////
        // 3. Simple input handling
        /////////////////////////////
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture
        )?;

        if event::poll(Duration::from_millis(update_freq))? {

            if let Event::Key(key) = event::read()? {

                match editing {

                    false => match key.code {

                        KeyCode::Char('q') => {
                            break
                        },

                        KeyCode::Char('s') => {
                            editing = true;
                            process_info = 0;
                        },

                        KeyCode::Char('f') => {
                            table_state.select_first()
                        },

                        KeyCode::Char('l') => {
                            table_state.select_last()
                        },

                        KeyCode::Down => {
                            let count = s.processes().len();
                            let i = match table_state.selected() {
                                Some(i) => if i >= count - 1 { 0 } else { i + 1 },
                                None => 0,
                            };
                            table_state.select(Some(i));
                        }

                        KeyCode::Up => {
                            let count = s.processes().len();
                            let i = match table_state.selected() {
                                Some(i) => if i == 0 { count - 1 } else { i - 1 },
                                None => 0,
                            };
                            table_state.select(Some(i));
                        }

                        KeyCode::Char('-') => {
                            if update_freq > 200 {
                                update_freq = update_freq - 200;
                            }
                        },

                        KeyCode::Char('+') => {
                            if update_freq < 3000 {
                                update_freq = update_freq + 200;
                            }
                        },

                        KeyCode::Char('p') => {
                            if sort_col == current_col {
                                reverse = !reverse;
                            }
                            sort_col = 0;
                        },

                        KeyCode::Char('n') => {
                            if sort_col == current_col {
                                reverse = !reverse;
                            }
                            sort_col = 1;
                        },

                        KeyCode::Char('m') => {
                            if sort_col == current_col {
                                reverse = !reverse;
                            }
                            sort_col = 2;
                        },

                        KeyCode::Char('c') => {
                            if sort_col == current_col {
                                reverse = !reverse;
                            }
                            sort_col = 3;
                        },

                        KeyCode::Enter => {
                            if !table_state.selected().is_none() {
                                process_info = if process_info == 0 { 1 } else { 0 }
                            }
                        },

                        _ => {}

                    },

                    true => match key.code {
                        
                        KeyCode::Esc => {
                            editing = false;
                            filter_text.clear();
                        },

                        KeyCode::Enter => {
                            // editing = false;
                            if table_state.selected().is_some() {
                                process_info = if process_info == 0 { 1 } else { 0 }
                            }
                        },

                        KeyCode::Char(c) => {
                            filter_text.push(c);
                            if filter_text.len() == 0 {
                                process_info = 0;
                            }
                        },

                        KeyCode::Backspace => {
                            if filter_text.len() == 0 {
                                editing = false;
                                process_info = 0;
                            } else {
                                filter_text.pop();
                            }
                        },

                        KeyCode::Down => {
                            let count = s.processes().len();
                            let i = match table_state.selected() {
                                Some(i) => if i >= count - 1 { 0 } else { i + 1 },
                                None => 0,
                            };
                            table_state.select(Some(i));
                        }

                        KeyCode::Up => {
                            let count = s.processes().len();
                            let i = match table_state.selected() {
                                Some(i) => if i == 0 { count - 1 } else { i - 1 },
                                None => 0,
                            };
                            table_state.select(Some(i));
                        }

                        _ => {}

                    }

                }

            }

        }

    } // main loop


    ////////////////////////
    // 4. Restore terminal
    ////////////////////////
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}



fn s_to_hms(secs: u64) -> (u64, u64, u64) {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    (h, m, s)
}


