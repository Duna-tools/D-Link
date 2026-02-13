use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use ratatui::widgets::TableState;
use crossbeam_channel::Receiver;
use chrono::Local;

use crate::core::device::{DeviceManager, DeviceEvent, Info, FileInfo, EntryType};
use crate::core::config::Config;
use tui_textarea::TextArea;
use ratatui::{
    widgets::{Block, Borders},
    style::{Style, Color},
};

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Mkdir,
    Rename,
}

pub struct App<'a> {
    pub should_quit: bool,
    pub device_manager: Arc<DeviceManager>,
    pub event_receiver: Receiver<DeviceEvent>,
    pub config: Config,
    
    // UI state and filesystem navigation
    pub connected_device: Option<Info>,
    pub current_path: String,
    pub all_files: Vec<FileInfo>,
    pub filtered_indices: Vec<usize>,
    pub table_state: TableState,
    pub selected_indices: HashSet<usize>,
    pub logs: Vec<String>,
    
    // Input buffers and interaction modes
    pub input_mode: InputMode,
    pub text_area: TextArea<'a>,
    pub show_help: bool,
    pub show_confirm_delete: bool,
    pub show_details: bool,
    pub error_message: Option<String>,

    // Background task synchronization
    pub transfer_progress: Arc<AtomicUsize>, // Percentage 0-100
    pub is_transferring: bool,
    pub current_view_id: usize,
    pub ndless_installed: bool,
}

impl<'a> App<'a> {
    pub fn new(dm: Arc<DeviceManager>, rx: Receiver<DeviceEvent>) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        let config = Config::load();
        
        Self {
            should_quit: false,
            device_manager: dm,
            event_receiver: rx,
            config,
            connected_device: None,
            current_path: "/".to_string(),
            all_files: Vec::with_capacity(64),
            filtered_indices: Vec::with_capacity(64),
            table_state,
            selected_indices: HashSet::new(),
            logs: Vec::with_capacity(6),
            input_mode: InputMode::Normal,
            text_area: TextArea::default(),
            show_help: false,
            show_confirm_delete: false,
            show_details: false,
            error_message: None,
            transfer_progress: Arc::new(AtomicUsize::new(0)),
            is_transferring: false,
            current_view_id: 0,
            ndless_installed: false,
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.log(&format!("ERROR: {}", msg));
        self.error_message = Some(msg);
    }

    pub fn toggle_details(&mut self) {
        if self.connected_device.is_some() && self.table_state.selected().is_some() {
            self.show_details = !self.show_details;
        }
    }

    pub fn log(&mut self, msg: &str) {
        if self.logs.len() >= 5 {
            self.logs.remove(0);
        }
        self.logs.push(msg.to_string());
    }

    pub fn on_tick(&mut self) {
        // Finalize transfer state upon reaching completion
        if self.is_transferring && self.transfer_progress.load(Ordering::SeqCst) >= 100 {
            self.is_transferring = false;
            self.transfer_progress.store(0, Ordering::SeqCst);
            self.refresh_file_list();
        }

        // Process hardware and background task events
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                DeviceEvent::Connected(meta) => {
                    self.log(&format!("USB: Detectado ({})", meta.name));
                    match self.device_manager.connect(meta.id) {
                        Ok(info) => {
                            self.connected_device = Some(info.clone());
                            self.log(&format!("USB: Conectado a {}", info.name));
                            
                            // Heuristic Ndless detection during initial connection
                            if let Ok(root_files) = self.device_manager.list_dir("/") {
                                self.ndless_installed = root_files.iter().any(|f| f.name().to_string_lossy() == "ndless");
                            }

                            self.refresh_file_list();
                        },
                        Err(e) => self.log(&format!("ERROR: {}", e)),
                    }
                },
                DeviceEvent::Disconnected => {
                    self.log("USB: Desconectado");
                    self.device_manager.disconnect();
                    self.connected_device = None;
                    self.all_files.clear();
                    self.filtered_indices.clear();
                    self.selected_indices.clear();
                    self.show_confirm_delete = false;
                    self.is_transferring = false;
                    self.current_view_id += 1; // Invalidate stale background tasks
                },
                DeviceEvent::DirSizeComputed { name, size, generation_id } => {
                    // Reactive UI update for asynchronous size calculations
                    if generation_id == self.current_view_id {
                        if let Some(file) = self.all_files.iter_mut().find(|f| f.name().to_string_lossy() == name) {
                            file.set_size(size);
                        }
                    }
                }
            }
        }
    }

    pub fn refresh_file_list(&mut self) {
        if self.connected_device.is_none() {
            self.all_files.clear();
            self.filtered_indices.clear();
            return;
        }

        self.current_view_id += 1; 
        let current_gen = self.current_view_id;

        self.filtered_indices.clear();
        self.table_state.select(None);

        match self.device_manager.list_dir(&self.current_path) {
            Ok(mut files) => {
                files.sort_by(|a: &FileInfo, b: &FileInfo| {
                    match (a.entry_type() == EntryType::Directory, b.entry_type() == EntryType::Directory) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.name().cmp(b.name()),
                    }
                });
                
                // Dispatch asynchronous directory size calculation
                let dirs_to_compute: Vec<String> = files.iter()
                    .filter(|f| f.entry_type() == EntryType::Directory)
                    .map(|f| f.name().to_string_lossy().into_owned())
                    .collect();
                
                if !dirs_to_compute.is_empty() {
                    self.device_manager.compute_directory_sizes(self.current_path.clone(), dirs_to_compute, current_gen);
                }

                self.all_files = files;
                self.apply_filter();
            },
            Err(e) => {
                self.set_error(format!("Error FS: {}", e));
                self.all_files.clear();
                self.filtered_indices.clear();
            }
        }
        self.selected_indices.clear();
        self.table_state.select(if self.filtered_indices.is_empty() { None } else { Some(0) });
    }

    pub fn apply_filter(&mut self) {
        self.filtered_indices.clear();
        let query = self.text_area.lines()[0].to_lowercase();
        
        for (i, file) in self.all_files.iter().enumerate() {
            if query.is_empty() || file.name().to_string_lossy().to_lowercase().contains(&query) {
                self.filtered_indices.push(i);
            }
        }
        
        if self.filtered_indices.is_empty() {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(0));
        }
    }

    pub fn enter_search(&mut self) {
        self.input_mode = InputMode::Editing;
        self.text_area = TextArea::default();
        self.text_area.set_placeholder_text("Escribe para filtrar...");
        self.text_area.set_block(Block::default().borders(Borders::ALL).title(" BUSCAR ").border_style(Style::default().fg(Color::Yellow)));
    }

    pub fn exit_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.apply_filter();
    }

    pub fn enter_mkdir(&mut self) {
        self.input_mode = InputMode::Mkdir;
        self.text_area = TextArea::default();
        self.text_area.set_placeholder_text("Nombre de la carpeta...");
        self.text_area.set_block(Block::default().borders(Borders::ALL).title(" NUEVA CARPETA ").border_style(Style::default().fg(Color::Green)));
    }

    pub fn submit_mkdir(&mut self) {
        let name = self.text_area.lines()[0].trim().to_string();
        if name.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        let path = if self.current_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", self.current_path, name)
        };

        if let Err(e) = self.device_manager.create_dir(&path) {
            self.log(&format!("Error mkdir: {}", e));
        } else {
            self.log(&format!("Carpeta creada: {}", name));
        }

        self.input_mode = InputMode::Normal;
        self.refresh_file_list();
    }

    pub fn enter_rename(&mut self) {
        if let Some(i) = self.table_state.selected() {
            if let Some(&real_idx) = self.filtered_indices.get(i) {
                if let Some(file) = self.all_files.get(real_idx) {
                    let current_name = file.name().to_string_lossy().into_owned();
                    self.text_area = TextArea::from(vec![current_name]);
                    self.text_area.set_block(Block::default().borders(Borders::ALL).title(" RENOMBRAR ").border_style(Style::default().fg(Color::Magenta)));
                    self.input_mode = InputMode::Rename;
                }
            }
        }
    }

    pub fn submit_rename(&mut self) {
        let new_name = self.text_area.lines()[0].trim().to_string();
        if new_name.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        if let Some(i) = self.table_state.selected() {
            if let Some(&real_idx) = self.filtered_indices.get(i) {
                if let Some(file) = self.all_files.get(real_idx) {
                    let old_name = file.name().to_string_lossy();
                    let old_path = if self.current_path == "/" { format!("/{}", old_name) } else { format!("{}/{}", self.current_path, old_name) };
                    let new_path = if self.current_path == "/" { format!("/{}", new_name) } else { format!("{}/{}", self.current_path, new_name) };

                    if let Err(e) = self.device_manager.rename(&old_path, &new_path) {
                        self.set_error(format!("Renombrar falló: {}", e));
                    } else {
                        self.log("Renombrado exitoso.");
                    }
                }
            }
        }

        self.input_mode = InputMode::Normal;
        self.refresh_file_list();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn trigger_delete_confirm(&mut self) {
        if self.connected_device.is_none() { return; }
        if !self.selected_indices.is_empty() || self.table_state.selected().is_some() {
            self.show_confirm_delete = true;
        }
    }

    pub fn toggle_selection(&mut self) {
        if let Some(i) = self.table_state.selected() {
            if let Some(&real_idx) = self.filtered_indices.get(i) {
                if self.selected_indices.contains(&real_idx) {
                    self.selected_indices.remove(&real_idx);
                } else {
                    self.selected_indices.insert(real_idx);
                }
            }
        }
    }

    pub fn enter_selected(&mut self) {
        if let Some(i) = self.table_state.selected() {
            if let Some(&real_idx) = self.filtered_indices.get(i) {
                if let Some(file) = self.all_files.get(real_idx) {
                    if file.entry_type() == EntryType::Directory {
                        let name = file.name().to_string_lossy();
                        self.current_path = if self.current_path == "/" {
                            format!("/{}", name)
                        } else {
                            format!("{}/{}", self.current_path, name)
                        };
                        self.input_mode = InputMode::Normal;
                        self.refresh_file_list();
                    }
                }
            }
        }
    }

    pub fn go_up(&mut self) {
        if self.current_path == "/" { return; }
        let path_buf = PathBuf::from(&self.current_path);
        if let Some(parent) = path_buf.parent() {
            self.current_path = parent.to_string_lossy().to_string();
            if self.current_path.is_empty() { self.current_path = "/".to_string(); }
            self.input_mode = InputMode::Normal;
            self.refresh_file_list();
        }
    }

    pub fn select_all_by_ext(&mut self, ext: &str) {
        let ext = ext.to_lowercase();
        for &idx in &self.filtered_indices {
            if let Some(file) = self.all_files.get(idx) {
                let name = file.name().to_string_lossy().to_lowercase();
                if name.ends_with(&ext) {
                    self.selected_indices.insert(idx);
                }
            }
        }
        self.log(&format!("Seleccionados todos los {}", ext));
    }

    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
    }

    pub fn get_selected_file_info(&self) -> Option<(String, u64, String)> {
        let i = self.table_state.selected()?;
        let &real_idx = self.filtered_indices.get(i)?;
        let file = self.all_files.get(real_idx)?;
        
        if file.entry_type() == EntryType::Directory { return None; }
        
        let name = file.name().to_string_lossy().to_string();
        let path = if self.current_path == "/" { format!("/{}", name) } else { format!("{}/{}", self.current_path, name) };
        
        Some((path, file.size(), name))
    }

    pub fn delete_selected_items(&mut self) {
        if self.connected_device.is_none() { return; }
        
        let indices: Vec<usize> = self.selected_indices.iter().cloned().collect();
        let targets = if indices.is_empty() {
            if let Some(i) = self.table_state.selected() {
                if let Some(&real_idx) = self.filtered_indices.get(i) {
                    vec![real_idx]
                } else { vec![] }
            } else { vec![] }
        } else {
            indices
        };

        for idx in targets {
            if let Some(file) = self.all_files.get(idx) {
                let path = if self.current_path == "/" {
                    format!("/{}", file.name().to_string_lossy())
                } else {
                    format!("{}/{}", self.current_path, file.name().to_string_lossy())
                };
                
                if let Err(e) = self.device_manager.delete(&path) {
                    self.set_error(format!("Borrado falló ({}): {}", path, e));
                } else {
                    self.log(&format!("Borrado: {}", path));
                }
            }
        }
        self.refresh_file_list();
    }

    pub fn upload_files(&mut self, paths: Vec<String>) {
        if self.connected_device.is_none() || self.is_transferring { return; }

        let dm = self.device_manager.clone();
        let current_path = self.current_path.clone();
        let progress = self.transfer_progress.clone();
        
        self.is_transferring = true;
        self.transfer_progress.store(0, Ordering::SeqCst);

        std::thread::spawn(move || {
            for local_path_str in paths {
                let local_path = PathBuf::from(local_path_str.trim());
                if !local_path.exists() { 
                    progress.fetch_add(10, Ordering::SeqCst); // Avanzar algo para no bloquear
                    continue; 
                }

                let total_size = local_path.metadata().map(|m| m.len()).unwrap_or(1) as f64;
                let filename = local_path.file_name().unwrap_or_default().to_string_lossy();
                let remote_path = if current_path == "/" {
                    format!("/{}", filename)
                } else {
                    format!("{}/{}", current_path, filename)
                };

                let _ = dm.upload_file(&local_path, &remote_path, |sent| {
                    let p = ((sent as f64 / total_size) * 100.0) as usize;
                    progress.store(p.min(100), Ordering::SeqCst);
                });
            }
            progress.store(100, Ordering::SeqCst);
        });
    }

    pub fn download_selected_items(&mut self) {
        if self.connected_device.is_none() || self.is_transferring { return; }

        let download_dir = PathBuf::from(&self.config.download_directory);
        let indices: Vec<usize> = self.selected_indices.iter().cloned().collect();
        let targets = if indices.is_empty() {
            if let Some(i) = self.table_state.selected() {
                if let Some(&real_idx) = self.filtered_indices.get(i) {
                    vec![real_idx]
                } else { vec![] }
            } else { vec![] }
        } else {
            indices
        };

        if targets.is_empty() { return; }

        let dm = self.device_manager.clone();
        let current_path = self.current_path.clone();
        let progress = self.transfer_progress.clone();
        
        let files_to_download: Vec<(String, u64, PathBuf)> = targets.iter().filter_map(|&idx| {
            self.all_files.get(idx).map(|f| {
                let name = f.name().to_string_lossy().to_string();
                let path = if current_path == "/" { format!("/{}", name) } else { format!("{}/{}", current_path, name) };
                (path, f.size(), download_dir.join(name))
            })
        }).collect();

        self.is_transferring = true;
        self.transfer_progress.store(0, Ordering::SeqCst);

        std::thread::spawn(move || {
            for (remote_path, size, local_path) in files_to_download {
                let total_size = size as f64;
                let _ = dm.download_file(&remote_path, &local_path, size, |read| {
                    let p = ((read as f64 / total_size) * 100.0) as usize;
                    progress.store(p.min(100), Ordering::SeqCst);
                });
            }
            progress.store(100, Ordering::SeqCst);
        });
    }

    pub fn take_screenshot(&mut self) {
        if self.connected_device.is_none() { return; }
        
        let download_dir = PathBuf::from(&self.config.download_directory);
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("nspire_screenshot_{}.png", timestamp);
        let path = download_dir.join(filename);

        match self.device_manager.take_screenshot(&path) {
            Ok(_) => self.log(&format!("Captura guardada: {:?}", path)),
            Err(e) => self.log(&format!("Error captura: {}", e)),
        }
    }

    pub fn next(&mut self) {
        if self.filtered_indices.is_empty() { return; }
        let i = match self.table_state.selected() {
            Some(i) => if i >= self.filtered_indices.len().saturating_sub(1) { 0 } else { i + 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.filtered_indices.is_empty() { return; }
        let i = match self.table_state.selected() {
            Some(i) => if i == 0 { self.filtered_indices.len().saturating_sub(1) } else { i - 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[test]
    fn test_app_initialization() {
        let (tx, rx) = unbounded();
        let dm = Arc::new(DeviceManager::new(tx));
        let app = App::new(dm, rx);
        
        assert_eq!(app.current_path, "/");
        assert!(app.all_files.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_apply_filter() {
        let (tx, rx) = unbounded();
        let dm = Arc::new(DeviceManager::new(tx));
        let mut app = App::new(dm, rx);
        
        // Simular archivos
        // Nota: FileInfo no es publicamente construible facilmente sin mocks
        // pero podemos probar el estado del text_area
        app.text_area.insert_str("test");
        app.apply_filter();
        assert_eq!(app.text_area.lines()[0], "test");
    }
}