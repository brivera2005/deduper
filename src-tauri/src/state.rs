use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::db::Database;
use crate::scanner::ScanProgress;

pub struct AppState {
    pub db: Mutex<Database>,
    pub data_dir: PathBuf,
    pub scan_cancel: AtomicBool,
    pub active_scan: Mutex<Option<ScanProgress>>,
}

impl AppState {
    pub fn new(db: Database, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            scan_cancel: AtomicBool::new(false),
            active_scan: Mutex::new(None),
        }
    }

    pub fn request_scan_cancel(&self) {
        self.scan_cancel.store(true, Ordering::SeqCst);
    }

    pub fn reset_scan_cancel(&self) {
        self.scan_cancel.store(false, Ordering::SeqCst);
    }

    pub fn is_scan_cancelled(&self) -> bool {
        self.scan_cancel.load(Ordering::SeqCst)
    }
}

pub type SharedState = Arc<AppState>;
