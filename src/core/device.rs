use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crossbeam_channel::Sender;
use libnspire::VID;
use rusb::{GlobalContext, Hotplug, UsbContext};
use thiserror::Error;

// Re-exportamos tipos útiles
pub use libnspire::info::Info;
pub use libnspire::info::Battery;
pub use libnspire::dir::EntryType;

#[derive(Clone, Debug)]
pub struct FileInfo {
    name: String,
    size: u64,
    entry_type: EntryType,
}

impl FileInfo {
    pub fn name(&self) -> &Path {
        Path::new(&self.name)
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn entry_type(&self) -> EntryType {
        self.entry_type
    }
    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("USB: {0}")]
    Usb(#[from] rusb::Error),
    #[error("Nspire: {0}")]
    Nspire(#[from] libnspire::Error),
    #[error("No encontrado")]
    DeviceNotFound,
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Connected(DeviceMetadata),
    Disconnected,
    DirSizeComputed {
        name: String,
        size: u64,
        generation_id: usize,
    },
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct DeviceId {
    pub bus: u8,
    pub address: u8,
}

#[derive(Debug, Clone)]
pub struct DeviceMetadata {
    pub id: DeviceId,
    pub name: String,
}

struct DeviceSession {
    handle: Arc<Mutex<libnspire::Handle<GlobalContext>>>,
    #[allow(dead_code)]
    metadata: DeviceMetadata,
}

pub struct DeviceManager {
    active_device: Arc<RwLock<Option<DeviceSession>>>,
    event_sender: Sender<DeviceEvent>,
}

impl DeviceManager {
    pub fn new(sender: Sender<DeviceEvent>) -> Self {
        Self {
            active_device: Arc::new(RwLock::new(None)),
            event_sender: sender,
        }
    }

    pub fn start_hotplug_monitor(&self) -> AppResult<()> {
        if !rusb::has_hotplug() {
            return Err(AppError::Usb(rusb::Error::NotSupported));
        }

        let monitor = UsbMonitor {
            sender: self.event_sender.clone(),
        };

        let _ = GlobalContext::default().register_callback(
            Some(VID),
            None,
            None,
            Box::new(monitor),
        )?;

        std::thread::spawn(|| loop {
            if let Err(e) = GlobalContext::default().handle_events(None) {
                eprintln!("USB Event Error: {}", e);
                std::thread::sleep(Duration::from_secs(1));
            }
        });

        Ok(())
    }

    pub fn connect(&self, id: DeviceId) -> AppResult<Info> {
        let devices = rusb::devices()?;
        let usb_dev = devices.iter().find(|d| {
            d.bus_number() == id.bus && d.address() == id.address
        }).ok_or(AppError::DeviceNotFound)?;

        let handle = libnspire::Handle::new(usb_dev.open()?)?;
        let info = handle.info()?;

        let metadata = DeviceMetadata {
            id,
            name: info.name.clone(),
        };

        let mut session = self.active_device.write().unwrap();
        *session = Some(DeviceSession {
            handle: Arc::new(Mutex::new(handle)),
            metadata,
        });

        Ok(info)
    }

    pub fn disconnect(&self) {
        let mut session = self.active_device.write().unwrap();
        *session = None;
    }

    pub fn list_dir(&self, path: &str) -> AppResult<Vec<FileInfo>> {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();

        let dir_list = handle.list_dir(path)?;
        let files = dir_list.iter().map(|item| FileInfo {
            name: item.name().to_string_lossy().into_owned(),
            size: item.size(),
            entry_type: item.entry_type(),
        }).collect();

        Ok(files)
    }

    pub fn compute_directory_sizes(&self, current_path: String, targets: Vec<String>, generation_id: usize) {
        let session_guard = match self.active_device.read() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        
        let session = match session_guard.as_ref() {
            Some(s) => s,
            None => return,
        };
        let handle_mutex = session.handle.clone();
        let sender = self.event_sender.clone();

        std::thread::spawn(move || {
            for name in targets {
                let full_path = if current_path == "/" { 
                    format!("/{}", name) 
                } else { 
                    format!("{}/{}", current_path, name) 
                };

                let size = Self::calc_recursive_static(&handle_mutex, &full_path);
                let _ = sender.send(DeviceEvent::DirSizeComputed {
                    name,
                    size,
                    generation_id,
                });
            }
        });
    }

    fn calc_recursive_static(handle_mutex: &Arc<Mutex<libnspire::Handle<GlobalContext>>>, path: &str) -> u64 {
        let mut total = 0;
        // Scope para el lock: obtener lista y soltar rapido
        let entries_opt = {
            let handle = match handle_mutex.lock() {
                Ok(h) => h,
                Err(_) => return 0,
            };
            handle.list_dir(path).ok() // Usamos ok() en lugar de unwrap_or_default
        }; 

        if let Some(entries) = entries_opt {
            for entry in entries.iter() {
                if entry.entry_type() == EntryType::Directory {
                    let sub_path = if path == "/" { 
                        format!("/{}", entry.name().to_string_lossy()) 
                    } else { 
                        format!("{}/{}", path, entry.name().to_string_lossy()) 
                    };
                    total += Self::calc_recursive_static(handle_mutex, &sub_path);
                } else {
                    total += entry.size();
                }
            }
        }
        total
    }

    pub fn delete(&self, path: &str) -> AppResult<()> {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();
        
        if handle.delete_file(path).is_err() {
            handle.delete_dir(path)?;
        }
        Ok(())
    }

    pub fn create_dir(&self, path: &str) -> AppResult<()> {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();
        handle.create_dir(path)?;
        Ok(())
    }

    pub fn rename(&self, old_path: &str, new_path: &str) -> AppResult<()> {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();
        handle.move_file(old_path, new_path)?;
        Ok(())
    }

    pub fn upload_file<F>(&self, local_path: &Path, remote_path: &str, mut progress_cb: F) -> AppResult<()>
    where F: FnMut(usize)
    {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();

        let mut file = File::open(local_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        handle.write_file(remote_path, &buffer, &mut progress_cb)?;
        Ok(())
    }

    pub fn download_file<F>(&self, remote_path: &str, local_path: &Path, size: u64, mut progress_cb: F) -> AppResult<()>
    where F: FnMut(usize)
    {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();

        let mut buffer = vec![0u8; size as usize];
        handle.read_file(remote_path, &mut buffer, &mut progress_cb)?;

        let file = File::create(local_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&buffer)?;
        
        Ok(())
    }

    pub fn take_screenshot(&self, output_path: &Path) -> AppResult<()> {
        let session_guard = self.active_device.read().unwrap();
        let session = session_guard.as_ref().ok_or(AppError::DeviceNotFound)?;
        let handle = session.handle.lock().unwrap();

        let image = handle.screenshot()?;
        
        let format = if image.bpp == 16 {
            image::ExtendedColorType::Rgb8
        } else {
            image::ExtendedColorType::L8
        };

        // Guardar con mayor eficiencia
        image::save_buffer(
            output_path,
            &image.data,
            image.width as u32,
            image.height as u32,
            format,
        ).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Error al guardar imagen: {}", e)))?;

        Ok(())
    }
}

struct UsbMonitor {
    sender: Sender<DeviceEvent>,
}

impl Hotplug<GlobalContext> for UsbMonitor {
    fn device_arrived(&mut self, device: rusb::Device<GlobalContext>) {
        let id = DeviceId {
            bus: device.bus_number(),
            address: device.address(),
        };
        let metadata = DeviceMetadata {
            id: id.clone(),
            name: "TI-Nspire".into(),
        };
        let _ = self.sender.send(DeviceEvent::Connected(metadata));
    }

    fn device_left(&mut self, _device: rusb::Device<GlobalContext>) {
        let _ = self.sender.send(DeviceEvent::Disconnected);
    }
}