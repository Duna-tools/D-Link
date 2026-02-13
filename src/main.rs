mod app;
mod core;
mod ui;

use std::io;

use std::sync::Arc;

use std::time::Duration;

use std::process::Command;

use std::fs;

use std::path::Path;



use crossterm::{

    event::{self, Event, KeyCode, KeyEventKind, MouseEvent, MouseEventKind, MouseButton, EnableMouseCapture, DisableMouseCapture, EnableBracketedPaste, DisableBracketedPaste},

    execute,

    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},

};

use ratatui::{backend::CrosstermBackend, Terminal};

use crossbeam_channel::unbounded;



use crate::app::{App, InputMode};

use crate::core::device::DeviceManager;



// Altura del header (3) + Borde tabla (1) + Header tabla (1) + Margen (1)

const TABLE_AREA_TOP: u16 = 6;



fn main() -> io::Result<()> {



    // Terminal raw mode initialization and event capture setup



    enable_raw_mode()?;



    let mut stdout = io::stdout();



    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;



    let backend = CrosstermBackend::new(stdout);



    let mut terminal = Terminal::new(backend)?;







    // Core device management initialization with cross-thread communication



    let (tx, rx) = unbounded();



    let dm = Arc::new(DeviceManager::new(tx));



    let _ = dm.start_hotplug_monitor();







    // Application state state container



    let mut app = App::new(dm, rx);







    // Main application rendering and event dispatching loop



    loop {



        terminal.draw(|f| ui::draw(f, &mut app))?;







        if event::poll(Duration::from_millis(50))? {



            match event::read()? {



                Event::Key(key) => {



                    if key.kind == KeyEventKind::Press {



                        if let Some(_) = app.error_message {



                            match key.code {



                                KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => app.error_message = None,



                                _ => {}



                            }



                        } else if app.show_help {



                            match key.code {



                                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => app.toggle_help(),



                                _ => {}



                            }



                        } else if app.show_details {



                            match key.code {



                                KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('q') => app.show_details = false,



                                _ => {}



                            }



                        } else if app.show_confirm_delete {



                            match key.code {



                                KeyCode::Char('y') | KeyCode::Enter => {



                                    app.delete_selected_items();



                                    app.show_confirm_delete = false;



                                },



                                KeyCode::Char('n') | KeyCode::Esc => app.show_confirm_delete = false,



                                _ => {}



                            }



                        } else {



                            match app.input_mode {



                                InputMode::Normal => match key.code {



                                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,



                                    KeyCode::Down | KeyCode::Char('j') => app.next(),



                                    KeyCode::Up | KeyCode::Char('k') => app.previous(),



                                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter_selected(),



                                    KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => app.go_up(),



                                    KeyCode::Char(' ') => app.toggle_selection(),



                                    KeyCode::Char('r') => app.enter_rename(),



                                    KeyCode::Char('?') => app.toggle_help(),



                                    KeyCode::Delete => app.trigger_delete_confirm(),



                                    KeyCode::Char('n') => app.enter_mkdir(),



                                    KeyCode::Char('d') => app.download_selected_items(),



                                    KeyCode::Char('s') => app.take_screenshot(),



                                    KeyCode::Char('/') => app.enter_search(),



                                    KeyCode::Char('i') => app.toggle_details(),



                                    KeyCode::Char('a') => app.select_all_by_ext(".tns"),



                                    KeyCode::Char('c') => app.clear_selection(),



                                    KeyCode::Char('e') => {



                                        if let Some((remote_path, size, name)) = app.get_selected_file_info() {



                                            if name.to_lowercase().ends_with(".tns") {



                                                app.set_error("No se pueden editar archivos binarios .tns directamente.".into());



                                                continue;



                                            }







                                            // Atomic download to temporary storage for external modification



                                            let extension = Path::new(&name).extension().and_then(|s: &std::ffi::OsStr| s.to_str()).unwrap_or("txt");



                                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();



                                            let safe_temp_name = format!("nspire_edit_{}.{}", timestamp, extension);



                                            let temp_path = std::env::temp_dir().join(safe_temp_name);



                                            



                                            if extension == "py" {



                                                app.log("AVISO: MicroPython detectado. Solo módulos ti_* permitidos.");



                                            } else if extension == "lua" {



                                                app.log("AVISO: Nspire Lua detectado. API de eventos activa.");



                                            } else if extension == "c" || extension == "h" || extension == "cpp" {



                                                app.log("AVISO: Código C/C++. Requiere Ndless SDK para generar .tns.");



                                            }



                                            app.log(&format!("Editando: {}", name));



                                            



                                            if let Err(e) = app.device_manager.download_file(&remote_path, &temp_path, size, |_| {}) {



                                                app.set_error(format!("Fallo descarga para edición: {}", e));



                                                continue;



                                            }







                                            // Interface suspension for external process execution



                                            disable_raw_mode()?;



                                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;



                                            



                                            let editor = std::env::var("EDITOR").unwrap_or_else(|_| {



                                                if cfg!(target_os = "windows") {



                                                    "notepad.exe".into()



                                                } else {



                                                    "nano".into()



                                                }



                                            });



                                            let _ = Command::new(editor).arg(&temp_path).status();







                                            // Terminal restoration following external command exit



                                            enable_raw_mode()?;



                                            execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;



                                            terminal.clear()?;







                                            // Automated synchronization of local modifications back to hardware



                                            if let Err(e) = app.device_manager.upload_file(&temp_path, &remote_path, |_| {}) {



                                                app.set_error(format!("Error al subir cambios: {}", e));



                                            } else {



                                                app.log("Cambios guardados en la calculadora.");



                                            }



                                            let _ = fs::remove_file(temp_path);



                                            app.refresh_file_list();



                                        }



                                    }



                                    _ => {}



                                },                                InputMode::Editing | InputMode::Mkdir | InputMode::Rename => {



                                    match key.code {



                                        KeyCode::Enter => {



                                            match app.input_mode {



                                                InputMode::Editing => app.exit_search(),



                                                InputMode::Mkdir => app.submit_mkdir(),



                                                InputMode::Rename => app.submit_rename(),



                                                _ => {}



                                            }



                                        }



                                        KeyCode::Esc => app.input_mode = InputMode::Normal,



                                        _ => {



                                            app.text_area.input(key);



                                            if app.input_mode == InputMode::Editing {



                                                app.apply_filter();



                                            }



                                        }



                                    }



                                }



                            }



                        }



                    }



                },



                Event::Mouse(mouse_event) => handle_mouse(mouse_event, &mut app),



                Event::Paste(data) => {



                    let paths: Vec<String> = data.lines()



                        .map(|s| s.trim().to_string())



                        .filter(|s| !s.is_empty())



                        .collect();



                    app.upload_files(paths);



                },



                _ => {}



            }



        }







        app.on_tick();







        if app.should_quit {



            break;



        }



    }







    // Graceful terminal restoration



    disable_raw_mode()?;



    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)?;



    terminal.show_cursor()?;







    Ok(())



}

fn handle_mouse(event: MouseEvent, app: &mut App) {
    if app.show_help {
        if let MouseEventKind::Down(_) = event.kind {
            app.toggle_help();
        }
        return;
    }

    match event.kind {
        MouseEventKind::ScrollDown => app.next(),
        MouseEventKind::ScrollUp => app.previous(),
        MouseEventKind::Down(MouseButton::Left) => {
             // Solo si el click es en la zona de la tabla (despues de la sidebar de 30)
             if event.column > 30 {
                app.enter_selected();
             }
        },
        MouseEventKind::Moved => {
            let row = event.row;
            let col = event.column;
            if row >= TABLE_AREA_TOP && col > 30 {
                let visual_index = (row - TABLE_AREA_TOP) as usize;
                let offset = app.table_state.offset();
                if visual_index < 50 { 
                    let real_index = offset + visual_index;
                    if real_index < app.filtered_indices.len() {
                        app.table_state.select(Some(real_index));
                    }
                }
            }
        }
        _ => {}
    }
}
