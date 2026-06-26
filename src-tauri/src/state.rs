use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::db::Database;
use crate::scanner::{full_audit::FullAuditProgress, ScanProgress};

pub struct AppState {
    pub db: Mutex<Database>,
    pub data_dir: PathBuf,
    pub scan_cancel: AtomicBool,
    pub active_scan: Mutex<Option<ScanProgress>>,
    pub full_audit_cancel: AtomicBool,
    pub active_full_audit: Mutex<Option<FullAuditProgress>>,
}

impl AppState {
    pub fn new(db: Database, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            scan_cancel: AtomicBool::new(false),
            active_scan: Mutex::new(None),
            full_audit_cancel: AtomicBool::new(false),
            active_full_audit: Mutex::new(None),
        }
    }

    pub fn request_scan_cancel(&self) {
        self.scan_cancel.store(true, Ordering::SeqCst);
        self.full_audit_cancel.store(true, Ordering::SeqCst);
    }

    pub fn reset_scan_cancel(&self) {
        self.scan_cancel.store(false, Ordering::SeqCst);
    }

    pub fn is_scan_cancelled(&self) -> bool {
        self.scan_cancel.load(Ordering::SeqCst) || self.full_audit_cancel.load(Ordering::SeqCst)
    }

    pub fn request_full_audit_cancel(&self) {
        self.full_audit_cancel.store(true, Ordering::SeqCst);
        self.scan_cancel.store(true, Ordering::SeqCst);
    }

    pub fn reset_full_audit_cancel(&self) {
        self.full_audit_cancel.store(false, Ordering::SeqCst);
    }

    pub fn is_full_audit_cancelled(&self) -> bool {
        self.full_audit_cancel.load(Ordering::SeqCst)
    }
}

pub type SharedState = Arc<AppState>;
