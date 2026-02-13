use std::sync::atomic::Ordering;
use std::path::Path;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Table, Row, Cell, BorderType, Clear},
};
use crate::app::{App, InputMode};
use crate::core::device::{EntryType, Battery};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Layout principal: Header | Middle | Footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Header con Logo
            Constraint::Min(0),    // Cuerpo (Sidebar + Tabla)
            Constraint::Length(3), // Barra de comandos / Busqueda
        ])
        .split(area);

    draw_header(f, main_chunks[0]);

    // Middle: Sidebar | Browser
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30), // Sidebar informativa
            Constraint::Min(0),     // Explorador
        ])
        .split(main_chunks[1]);

    draw_sidebar(f, app, body_chunks[0]);
    draw_browser(f, app, body_chunks[1]);

    // Footer: Logs, Busqueda o Mkdir
    match app.input_mode {
        InputMode::Editing | InputMode::Mkdir | InputMode::Rename => {
            f.render_widget(&app.text_area, main_chunks[2]);
        }
        InputMode::Normal  => {
            if app.is_transferring {
                draw_transfer_bar(f, app, main_chunks[2]);
            } else {
                draw_footer_info(f, app, main_chunks[2]);
            }
        }
    }

    if app.show_confirm_delete {
        draw_confirm_delete_popup(f, app);
    }

    if app.show_help {
        draw_help_popup(f);
    }

    if let Some(msg) = &app.error_message {
        draw_error_popup(f, msg);
    }

    if app.show_details {
        draw_file_details(f, app);
    }
}

fn draw_error_popup(f: &mut Frame, msg: &str) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let p = Paragraph::new(format!("\n FATAL ERROR:\n\n {}", msg))
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" [ SYSTEM ALERT ] ")
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black).fg(Color::LightRed));

    f.render_widget(p, area);
}

fn draw_file_details(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 45, f.area());
    f.render_widget(Clear, area);

    if app.connected_device.is_none() {
        let mut help_msg = vec![
            Line::from(vec![Span::styled(" ⚠️ NO DEVICE CONNECTED ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
            Line::from(""),
            Line::from(" Troubleshooting by OS:"),
        ];

        if cfg!(target_os = "windows") {
            help_msg.push(Line::from(" - Use ZADIG to install WinUSB driver"));
        } else if cfg!(target_os = "linux") {
            help_msg.push(Line::from(" - Check udev rules or use sudo"));
        } else {
            help_msg.push(Line::from(" - Ensure libusb is installed via Homebrew"));
        }

        let p = Paragraph::new(help_msg)
            .block(Block::default().borders(Borders::ALL).title(" [ CONNECTION GUIDE ] "))
            .alignment(Alignment::Center);
        f.render_widget(p, area);
        return;
    }

    if let Some(i) = app.table_state.selected() {
        if let Some(&real_idx) = app.filtered_indices.get(i) {
            if let Some(file) = app.all_files.get(real_idx) {
                let is_dir = file.entry_type() == EntryType::Directory;
                let name = file.name().to_string_lossy();
                let extension = Path::new(&*name).extension().and_then(|s: &std::ffi::OsStr| s.to_str()).unwrap_or("N/A").to_uppercase();

                let mut rows = vec![
                    Row::new(vec![Cell::from(" NAME"), Cell::from(name.to_string())]),
                    Row::new(vec![Cell::from(" TYPE"), Cell::from(if is_dir { "📂 Directory" } else { "📄 File" })]),
                    Row::new(vec![Cell::from(" SIZE"), Cell::from(format_size(file.size()))]),
                    Row::new(vec![Cell::from(" EXTENSION"), Cell::from(extension)]),
                    Row::new(vec![Cell::from(" LOCATION"), Cell::from(format!("{}/...", app.current_path))]),
                ];

                if name.ends_with(".tns") {
                    rows.push(Row::new(vec![
                        Cell::from(" FORMAT").style(Style::default().fg(Color::Yellow)),
                        Cell::from("TI-Nspire Document").style(Style::default().fg(Color::Yellow))
                    ]));
                }

                if name.ends_with(".py") {
                    rows.push(Row::new(vec![
                        Cell::from(" ENV").style(Style::default().fg(Color::Green)),
                        Cell::from("MicroPython (TI-Nspire)").style(Style::default().fg(Color::Green))
                    ]));
                    rows.push(Row::new(vec![
                        Cell::from(" LIBS").style(Style::default().fg(Color::DarkGray)),
                        Cell::from("ti_system, ti_draw, ti_image, ti_runtime").style(Style::default().fg(Color::DarkGray))
                    ]));
                }

                if name.ends_with(".lua") {
                    rows.push(Row::new(vec![
                        Cell::from(" ENV").style(Style::default().fg(Color::Blue)),
                        Cell::from("TI-Nspire Lua (v5.1 API)").style(Style::default().fg(Color::Blue))
                    ]));
                    rows.push(Row::new(vec![
                        Cell::from(" EVENT-DRIVEN").style(Style::default().fg(Color::DarkGray)),
                        Cell::from("on.paint, on.construct, on.timer").style(Style::default().fg(Color::DarkGray))
                    ]));
                }

                if name.ends_with(".c") || name.ends_with(".h") || name.ends_with(".cpp") {
                    rows.push(Row::new(vec![
                        Cell::from(" ENV").style(Style::default().fg(Color::Magenta)),
                        Cell::from("Ndless C/C++ SDK").style(Style::default().fg(Color::Magenta))
                    ]));
                    rows.push(Row::new(vec![
                        Cell::from(" TARGET").style(Style::default().fg(Color::DarkGray)),
                        Cell::from("ARM Native Binary (.tns)").style(Style::default().fg(Color::DarkGray))
                    ]));
                }

                let table = Table::new(rows, [
                    Constraint::Length(12),
                    Constraint::Min(20),
                ])
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(" 💡 FILE PROPERTIES ")
                    .border_type(BorderType::Double)
                    .border_style(Style::default().fg(Color::Cyan)))
                .style(Style::default().bg(Color::Black).fg(Color::White))
                .column_spacing(2);

                f.render_widget(table, area);

                // Botón de cerrar implícito
                let close_hint = Paragraph::new(" [ ESC ] TO CLOSE ")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                let hint_area = Rect {
                    x: area.x,
                    y: area.y + area.height - 2,
                    width: area.width,
                    height: 1,
                };
                f.render_widget(close_hint, hint_area);
            }
        }
    }
}

fn draw_header(f: &mut Frame, area: Rect) {
    let logo = vec![
        " ____       _      ___  _   _  _  __ ",
        "|  _ \\     | |    |_ _|| \\ | || |/ / ",
        "| | | |____| |     | | |  \\| || ' /  ",
        "| |_| |____| |___  | | | |\\  || . \\  ",
        "|____/     |_____||___||_| \\_||_|\\_\\ ",
    ].join("\n");

    let header = Paragraph::new(logo)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Left);
    
    f.render_widget(header, area);

    let version = Paragraph::new("v0.1.0")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Right);
    f.render_widget(version, area);
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" [ SYSTEM STATUS ] ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Connection
            Constraint::Length(2), // Model & OS
            Constraint::Length(1), // Ndless Badge
            Constraint::Length(3), // Battery
            Constraint::Length(3), // Storage
            Constraint::Min(0),    // Logs
        ])
        .split(inner_area);

    // Connection status indicator
    let conn_status = if app.connected_device.is_some() {
        Span::styled(" ONLINE ", Style::default().bg(Color::Green).fg(Color::Black))
    } else {
        Span::styled(" OFFLINE ", Style::default().bg(Color::Red).fg(Color::White))
    };
    f.render_widget(Paragraph::new(Line::from(vec![Span::raw(" STATE: "), conn_status])), chunks[0]);

    // Hardware metadata display
    if let Some(info) = &app.connected_device {
        let model_text = format!(" MODEL: {}\n OS:    {}", info.name, info.version);
        f.render_widget(Paragraph::new(model_text).style(Style::default().fg(Color::White)), chunks[1]);
        
        // Native execution environment badge
        if app.ndless_installed {
            let badge = Paragraph::new(" [ NDLESS ACTIVE ] ")
                .style(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Left);
            f.render_widget(badge, chunks[2]);
        }
    } else {
        f.render_widget(Paragraph::new(" MODEL: N/A\n OS:    N/A").style(Style::default().fg(Color::DarkGray)), chunks[1]);
    }

    // Power management monitoring
    if let Some(info) = &app.connected_device {
        let bat_color = match info.battery {
            Battery::Low => Color::Red,
            Battery::Ok => Color::Yellow,
            Battery::Powered => Color::Blue,
            _ => Color::Green,
        };
        let bat_meter = match info.battery {
            Battery::Low => "[#---------]",
            Battery::Ok  => "[#####-----]",
            Battery::Powered => "[CONNECTED ]",
            _ => "[##########]",
        };
        f.render_widget(Paragraph::new(format!(" BATTERY: {}", bat_meter)).style(Style::default().fg(bat_color)), chunks[3]);

        // Non-volatile storage metrics
        let used = info.total_storage.saturating_sub(info.free_storage);
        let ratio = if info.total_storage > 0 { used as f64 / info.total_storage as f64 } else { 0.0 };
        let storage_bar = format!(" STORAGE: [{}{}]", "#".repeat((ratio * 10.0) as usize), "-".repeat(10 - (ratio * 10.0) as usize));
        f.render_widget(Paragraph::new(storage_bar).style(Style::default().fg(Color::Cyan)), chunks[4]);
    }

    // Transaction and system log buffer
    let logs: Vec<ListItem> = app.logs.iter().rev().take(5)
        .map(|m| ListItem::new(format!("> {}", m)).style(Style::default().fg(Color::Gray)))
        .collect();
    f.render_widget(List::new(logs).block(Block::default().title(" [ LOGS ] ")), chunks[5]);
}

fn draw_browser(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["", " FILE NAME", " TYPE", " SIZE"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Black).bg(Color::Cyan)));
    let header = Row::new(header_cells).height(1);

    let rows = app.filtered_indices.iter().map(|&real_idx| {
        let file = &app.all_files[real_idx];
        let is_dir = file.entry_type() == EntryType::Directory;
        let is_selected = app.selected_indices.contains(&real_idx);
        
        let select_marker = if is_selected { "[X]" } else { "   " };
        let (type_label, color, size_style) = if is_dir { 
            ("DIR ", Color::Blue, Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC)) 
        } else { 
            ("FILE", Color::White, Style::default().fg(Color::DarkGray)) 
        };

        let cells = vec![
            Cell::from(select_marker),
            Cell::from(format!(" {}", file.name().to_string_lossy())),
            Cell::from(type_label),
            Cell::from(format_size(file.size())).style(size_style),
        ];
        
        Row::new(cells).style(Style::default().fg(color))
    });

    let table = Table::new(rows, [
        Constraint::Length(4),
        Constraint::Min(30),
        Constraint::Length(8),
        Constraint::Length(12),
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title(format!(" [ FS: {} ] ", app.current_path))
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan)))
    .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
    .highlight_symbol(">>");

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_confirm_delete_popup(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 25, f.area());
    f.render_widget(Clear, area);

    let count = if app.selected_indices.is_empty() { 1 } else { app.selected_indices.len() };
    let text = vec![
        "",
        &format!(" DELETE {} ITEM(S)? ", count),
        "",
        " [y] CONFIRM  |  [n] CANCEL ",
    ].join("\n");

    let p = Paragraph::new(text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" [ WARNING ] ")
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Red)))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black).fg(Color::White));

    f.render_widget(p, area);
}

fn draw_transfer_bar(f: &mut Frame, app: &App, area: Rect) {
    let pval = app.transfer_progress.load(Ordering::SeqCst);
    let bar = format!(" TRANSFERRING: [{}{}] {}%", "#".repeat(pval/10), "-".repeat(10-(pval/10)), pval);
    let p = Paragraph::new(bar)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)))
        .style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
    f.render_widget(p, area);
}

fn draw_footer_info(f: &mut Frame, app: &App, area: Rect) {
    let mut shortcuts = vec![
        Span::styled(" [?] ", Style::default().fg(Color::Yellow)),
        Span::raw("HELP "),
        Span::styled(" [/] ", Style::default().fg(Color::Yellow)),
        Span::raw("SEARCH "),
    ];

    if let Some(i) = app.table_state.selected() {
        if let Some(&real_idx) = app.filtered_indices.get(i) {
            if let Some(file) = app.all_files.get(real_idx) {
                let name = file.name().to_string_lossy().to_lowercase();
                
                shortcuts.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                
                if file.entry_type() == EntryType::Directory {
                    shortcuts.push(Span::styled(" [ENTER] ", Style::default().fg(Color::Cyan)));
                    shortcuts.push(Span::raw("OPEN "));
                } else if name.ends_with(".tns") || name.ends_with(".luax") {
                    shortcuts.push(Span::styled(" [d] ", Style::default().fg(Color::Green)));
                    shortcuts.push(Span::raw("PULL "));
                    shortcuts.push(Span::styled(" [i] ", Style::default().fg(Color::Magenta)));
                    shortcuts.push(Span::raw("DETAILS "));
                } else {
                    shortcuts.push(Span::styled(" [SPACE] ", Style::default().fg(Color::Yellow)));
                    shortcuts.push(Span::raw("SELECT "));
                }
            }
        }
    }

    shortcuts.extend(vec![
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(" [n] ", Style::default().fg(Color::Yellow)),
        Span::raw("MKDIR "),
        Span::styled(" [Supr] ", Style::default().fg(Color::Red)),
        Span::raw("DELETE "),
    ]);

    let p = Paragraph::new(Line::from(shortcuts))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)));
    
    f.render_widget(p, area);
}

fn draw_help_popup(f: &mut Frame) {
    let area = centered_rect(65, 80, f.area());
    f.render_widget(Clear, area);

    let help_text = vec![
        " [ D-LINK COMMAND CENTER ] ",
        "--------------------------------------",
        "",
        " [ NAVIGATION ] ",
        " MOVE       : [UP/DOWN] or [j/k]",
        " ENTER/OPEN : [ENTER] or [l]",
        " BACK/UP    : [BACKSPACE] or [h]",
        " SEARCH     : [/] (Real-time filter)",
        "",
        " [ FILE OPERATIONS ] ",
        " LIVE EDIT  : [e] (Edit script in host editor)",
        " SELECT     : [SPACE]",
        " SMART SEL. : [a] (Select all .tns files)",
        " CLEAR SEL. : [c]",
        " DOWNLOAD   : [d] (To Downloads folder)",
        " DELETE     : [DELETE] (With confirmation)",
        " MKDIR      : [n] | RENAME: [r]",
        "",
        " [ TOOLS & SYSTEM ] ",
        " INSPECTOR  : [i] (Detailed properties)",
        " SCREENSHOT : [s] (Capture device screen)",
        " HELP       : [?] | QUIT: [q] or [ESC]",
        "",
        " [ INPUTS ] ",
        " DRAG & DROP supported for UPLOAD ",
        "--------------------------------------",
        " PRESS [?] OR CLICK TO CLOSE ",
    ].join("\n");

    let p = Paragraph::new(help_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" [ KEYBOARD MANIFEST ] ")
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan)))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black).fg(Color::White));

    f.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.1} KB", bytes as f64 / KB as f64) }
    else { format!("{} B", bytes) }
}
