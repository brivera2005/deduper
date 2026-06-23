import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import {
  Cloud,
  HardDrive,
  Settings,
  Shield,
  Smartphone,
} from "lucide-react";
import { SetupWizard } from "./components/SetupWizard";
import "./App.css";

interface DashboardStats {
  recoverable_bytes: number;
  recoverable_count: number;
  total_files: number;
  duplicate_groups: number;
  sources_connected: number;
  vault_path: string | null;
}

interface SourceRecord {
  id: string;
  source_type: string;
  name: string;
  config: { path?: string; storage_path?: string };
  status: string;
  last_scan_at: string | null;
  file_count: number;
  total_bytes: number;
}

interface ScanProgress {
  job_id: string;
  source_id: string;
  status: string;
  total_files: number;
  processed_files: number;
  hashed_files: number;
  current_file: string | null;
  error_message: string | null;
}

interface DriveAuthStatus {
  connected: boolean;
  email: string | null;
  scopes: string[];
  write_enabled: boolean;
}

interface SetupStatus {
  welcome_done: boolean;
  local_added: boolean;
  drive_connected: boolean;
  android_connected: boolean;
  first_scan_done: boolean;
  vault_set: boolean;
  wizard_completed: boolean;
  wizard_skipped: boolean;
}

interface MtpDeviceInfo {
  name: string;
  storage_name: string;
  storage_path: string;
  connected: boolean;
}

interface AuditEntry {
  id: string;
  action: string;
  details: Record<string, unknown>;
  dry_run: boolean;
  created_at: string;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 GB";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 ** 2;
  return `${mb.toFixed(1)} MB`;
}

function formatCount(n: number): string {
  return n.toLocaleString();
}

export default function App() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [driveAuth, setDriveAuth] = useState<DriveAuthStatus | null>(null);
  const [scan, setScan] = useState<ScanProgress | null>(null);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [toast, setToast] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [androidDevices, setAndroidDevices] = useState<MtpDeviceInfo[]>([]);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 4000);
  };

  const refresh = useCallback(async () => {
    try {
      const [d, s, da, a, st] = await Promise.all([
        invoke<DashboardStats>("get_dashboard"),
        invoke<SourceRecord[]>("list_sources"),
        invoke<DriveAuthStatus>("get_drive_auth_status"),
        invoke<AuditEntry[]>("get_audit_log", { limit: 8 }),
        invoke<SetupStatus>("get_setup_status"),
      ]);
      setStats(d);
      setSources(s);
      setDriveAuth(da);
      setAudit(a);
      setSetup(st);
      setShowWizard(!st.welcome_done);
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => {
    refresh();
    invoke<MtpDeviceInfo[]>("get_android_status")
      .then(setAndroidDevices)
      .catch(() => setAndroidDevices([]));
  }, [refresh]);

  useEffect(() => {
    if (!activeJobId) return;
    const interval = setInterval(async () => {
      try {
        const progress = await invoke<ScanProgress | null>("get_scan_status", {
          jobId: activeJobId,
        });
        if (progress) {
          setScan(progress);
          if (["completed", "failed", "cancelled"].includes(progress.status)) {
            setActiveJobId(null);
            refresh();
          }
        }
      } catch (e) {
        console.error(e);
      }
    }, 800);
    return () => clearInterval(interval);
  }, [activeJobId, refresh]);

  const pickFolder = async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a folder",
    });
    return selected as string | null;
  };

  const addLocal = async () => {
    const path = await pickFolder();
    if (!path) return;
    setBusy(true);
    try {
      await invoke("add_local_source", { path });
      showToast("Local folder added");
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const addPhoneImport = async () => {
    const path = await pickFolder();
    if (!path) return;
    setBusy(true);
    try {
      await invoke("add_phone_import_folder", { path });
      showToast("Phone import folder added");
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const connectDrive = async () => {
    setBusy(true);
    try {
      const status = await invoke<DriveAuthStatus>("connect_google_drive");
      setDriveAuth(status);
      showToast(`Connected as ${status.email ?? "Google account"}`);
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const detectAndConnectPhone = async () => {
    setBusy(true);
    try {
      const devices = await invoke<MtpDeviceInfo[]>("detect_android_devices");
      setAndroidDevices(devices);
      if (devices.length === 0) {
        showToast("No phone detected — check USB file transfer mode");
        return;
      }
      const device = devices[0];
      await invoke("connect_android_device", {
        storagePath: device.storage_path,
        deviceName: `${device.name} (${device.storage_name})`,
      });
      showToast(`Connected: ${device.name}`);
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const startScan = async (sourceId: string) => {
    setBusy(true);
    try {
      const jobId = await invoke<string>("start_scan", { sourceId });
      setActiveJobId(jobId);
      setScan({
        job_id: jobId,
        source_id: sourceId,
        status: "running",
        total_files: 0,
        processed_files: 0,
        hashed_files: 0,
        current_file: null,
        error_message: null,
      });
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const copyToVault = async (dryRun: boolean) => {
    const dest =
      stats?.vault_path ??
      (await invoke<string | null>("get_vault_path")) ??
      (await pickFolder());
    if (!dest) return;
    setBusy(true);
    try {
      const result = await invoke<{ copied_count: number; skipped_count: number }>(
        "copy_uniques_to_vault",
        { destination: dest, dryRun },
      );
      showToast(
        dryRun
          ? `Dry run: would copy ${result.copied_count} files`
          : `Copied ${result.copied_count} unique files to vault`,
      );
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const rerunWizard = async () => {
    await invoke("reset_wizard");
    setShowWizard(true);
    refresh();
  };

  const localSources = sources.filter((s) => s.source_type === "local");
  const androidSource = sources.find((s) => s.source_type === "android_mtp");
  const driveSource = sources.find((s) => s.source_type === "google_drive");

  const scanPct =
    scan && scan.total_files > 0
      ? Math.round((scan.processed_files / scan.total_files) * 100)
      : scan?.processed_files
        ? 5
        : 0;

  const dashboardBlocked = showWizard && !setup?.wizard_skipped;

  return (
    <div className="app">
      <header className="header">
        <div className="logo">
          <div className="logo-mark">D</div>
          <div>
            <h1>Deduper</h1>
            <p>Safe media consolidation</p>
          </div>
        </div>
        <div className="header-actions">
          <button className="btn btn-ghost" onClick={rerunWizard} title="Run setup wizard again">
            <Settings size={16} style={{ verticalAlign: "middle", marginRight: 4 }} />
            Setup
          </button>
          <button className="btn btn-ghost" onClick={() => open("https://github.com")}>
            Help
          </button>
        </div>
      </header>

      {showWizard && (
        <SetupWizard
          forceOpen
          onComplete={() => {
            setShowWizard(false);
            refresh();
          }}
          onSkip={() => {
            setShowWizard(false);
            refresh();
          }}
        />
      )}

      {setup?.wizard_skipped && !setup.wizard_completed && (
        <div className="skip-banner">
          Setup was skipped.{" "}
          <button type="button" className="link-btn" onClick={rerunWizard}>
            Run setup again
          </button>{" "}
          to connect Google Drive and your phone.
        </div>
      )}

      <div className={dashboardBlocked ? "dashboard-dimmed" : undefined}>
        <section className="hero">
          <div className="hero-label">Drive space you can recover</div>
          <div className="hero-value">{formatBytes(stats?.recoverable_bytes ?? 0)}</div>
          <p className="hero-sub">
            <strong>{formatCount(stats?.recoverable_count ?? 0)} files</strong> verified safe to
            remove from Google Drive (duplicates already on your PC or phone). Deduper never
            auto-deletes — review the report first.
          </p>
        </section>

        <div className="safety-banner">
          <Shield size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
          <strong>Quarantine-first:</strong> All cleanup is report-only by default. Optional
          &quot;Move to Drive Trash&quot; requires separate write consent and a confirmation showing
          GB recovered + sample files.
        </div>

        {scan && activeJobId && (
          <div className="progress-panel">
            <div className="section-title">Scan in progress</div>
            <div style={{ fontSize: "0.9rem" }}>
              {scan.current_file ?? "Indexing files…"} — {scan.status}
            </div>
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${scanPct}%` }} />
            </div>
            <div style={{ fontSize: "0.8rem", color: "var(--text-muted)" }}>
              {scan.processed_files} / {scan.total_files || "…"} files hashed
            </div>
          </div>
        )}

        <div className="grid">
          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <HardDrive size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                Local PC
              </div>
              <span className={`card-badge ${localSources.length ? "connected" : ""}`}>
                {localSources.length ? "Added" : "Not set up"}
              </span>
            </div>
            <p className="card-desc">
              Scan photos, videos, and documents from folders on this computer.
            </p>
            {localSources[0] && (
              <div className="card-stats">
                <span>
                  <strong>{formatCount(localSources[0].file_count)}</strong> files
                </span>
                <span>
                  <strong>{formatBytes(localSources[0].total_bytes)}</strong> indexed
                </span>
              </div>
            )}
            <div className="card-actions">
              <button className="btn btn-secondary" onClick={addLocal} disabled={busy}>
                Add folder
              </button>
              {localSources[0] && (
                <button
                  className="btn btn-primary"
                  onClick={() => startScan(localSources[0].id)}
                  disabled={busy || !!activeJobId}
                >
                  Scan
                </button>
              )}
            </div>
          </article>

          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <Cloud size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                Google Drive
              </div>
              <span className={`card-badge ${driveAuth?.connected ? "connected" : ""}`}>
                {driveAuth?.connected ? "Connected" : "Not connected"}
              </span>
            </div>
            <p className="card-desc">
              {driveAuth?.connected
                ? `Signed in as ${driveAuth.email ?? "your account"}. Read-only scan uses md5Checksum metadata.`
                : "Connect with read-only access to inventory your Drive files."}
            </p>
            {driveSource && driveSource.file_count > 0 && (
              <div className="card-stats">
                <span>
                  <strong>{formatCount(driveSource.file_count)}</strong> files
                </span>
                <span>
                  <strong>{formatBytes(driveSource.total_bytes)}</strong> indexed
                </span>
              </div>
            )}
            <div className="card-actions">
              {!driveAuth?.connected ? (
                <button className="btn btn-primary" onClick={connectDrive} disabled={busy}>
                  Connect Drive
                </button>
              ) : (
                <>
                  <button
                    className="btn btn-primary"
                    onClick={() => driveSource && startScan(driveSource.id)}
                    disabled={busy || !!activeJobId || !driveSource}
                  >
                    Scan Drive
                  </button>
                  <button
                    className="btn btn-ghost"
                    onClick={async () => {
                      await invoke("disconnect_google_drive");
                      refresh();
                    }}
                  >
                    Disconnect
                  </button>
                </>
              )}
            </div>
          </article>

          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <Smartphone size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                Android Phone
              </div>
              <span
                className={`card-badge ${androidSource || androidDevices.length ? "connected" : ""}`}
              >
                {androidSource
                  ? "Connected"
                  : androidDevices.length
                    ? "Detected"
                    : "Not connected"}
              </span>
            </div>
            <p className="card-desc">
              {androidSource
                ? `Scanning ${androidSource.name} over USB. Set phone to File Transfer / MTP mode.`
                : androidDevices.length
                  ? `${androidDevices[0].name} detected — click Connect Phone to scan.`
                  : "Plug in your phone via USB (File Transfer mode) or use Manual Import."}
            </p>
            {androidSource && androidSource.file_count > 0 && (
              <div className="card-stats">
                <span>
                  <strong>{formatCount(androidSource.file_count)}</strong> files
                </span>
                <span>
                  <strong>{formatBytes(androidSource.total_bytes)}</strong> indexed
                </span>
              </div>
            )}
            <div className="card-actions">
              {!androidSource ? (
                <>
                  <button className="btn btn-primary" onClick={detectAndConnectPhone} disabled={busy}>
                    Connect Phone
                  </button>
                  <button className="btn btn-secondary" onClick={addPhoneImport} disabled={busy}>
                    Manual import
                  </button>
                </>
              ) : (
                <>
                  <button
                    className="btn btn-primary"
                    onClick={() => startScan(androidSource.id)}
                    disabled={busy || !!activeJobId}
                  >
                    Scan Phone
                  </button>
                  <button className="btn btn-secondary" onClick={detectAndConnectPhone} disabled={busy}>
                    Refresh
                  </button>
                </>
              )}
            </div>
          </article>
        </div>

        <section style={{ marginBottom: "1.75rem" }}>
          <div className="section-title">Vault & actions</div>
          <div className="card">
            <p className="card-desc">
              Copy unique files to a destination folder on your PC.
              {stats?.vault_path && (
                <>
                  {" "}
                  Current vault: <strong>{stats.vault_path}</strong>
                </>
              )}
            </p>
            <div className="card-actions">
              <button className="btn btn-secondary" onClick={() => copyToVault(true)} disabled={busy}>
                Dry-run copy
              </button>
              <button className="btn btn-primary" onClick={() => copyToVault(false)} disabled={busy}>
                Copy uniques to vault
              </button>
            </div>
          </div>
        </section>

        <section>
          <div className="section-title">Audit log</div>
          <ul className="audit-list">
            {audit.length === 0 && (
              <li className="audit-item">No actions yet — your safety trail starts here.</li>
            )}
            {audit.map((entry) => (
              <li key={entry.id} className="audit-item">
                <span>
                  <strong>{entry.action.replace(/_/g, " ")}</strong>
                  {entry.dry_run && " (dry run)"}
                </span>
                <span>{new Date(entry.created_at).toLocaleString()}</span>
              </li>
            ))}
          </ul>
        </section>
      </div>

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
