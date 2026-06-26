import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  FolderOpen,
  HardDrive,
  Play,
  Settings,
  Shield,
  Smartphone,
  Sparkles,
  Trash2,
  FileText,
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

interface DriveAuthStatus {
  connected: boolean;
  email: string | null;
  scopes: string[];
  cleanup_enabled: boolean;
  photos_enabled: boolean;
  gmail_enabled: boolean;
}

interface StorageQuota {
  limit_bytes: number;
  usage_bytes: number;
  usage_in_drive_bytes: number;
  free_bytes: number;
  usage_display: string;
  limit_display: string;
  free_display: string;
  percent_used: number;
}

interface TrashResult {
  trashed_count: number;
  skipped_count: number;
  failed_count: number;
  bytes_freed: number;
  dry_run: boolean;
  quota_after: StorageQuota | null;
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

interface RecoverySample {
  filename: string;
  size_bytes: number;
  drive_file_id: string;
  copy_already_on_pc: string | null;
  copy_location_label: string | null;
}

interface AuditRecommendations {
  google_drive_duplicate_bytes: number;
  google_drive_duplicate_count: number;
  google_drive_only_bytes: number;
  google_drive_only_count: number;
  phone_only_bytes: number;
  phone_only_count: number;
  google_photos_count: number;
  gmail_attachment_bytes: number;
  gmail_attachment_count: number;
  total_files_checked: number;
  proof_samples: RecoverySample[];
  summary_plain: string;
}

interface FullAuditProgress {
  job_id: string;
  status: string;
  phase: string;
  message: string;
  sources_total: number;
  sources_done: number;
  current_source_name: string | null;
  current_file: string | null;
  files_processed: number;
  files_total: number;
  error_message: string | null;
}

interface CopyResult {
  copied_count: number;
  skipped_count: number;
  verified_count: number;
  failed_count: number;
  dry_run: boolean;
  destination: string;
}

interface AuditOptions {
  includeGoogleDrive: boolean;
  includeGooglePhotos: boolean;
  includeGmail: boolean;
  includeThisPc: boolean;
  includePhone: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 GB";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = bytes / 1024 ** 2;
  return `${mb.toFixed(0)} MB`;
}

function formatCount(n: number): string {
  return n.toLocaleString();
}

export default function App() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [sources, setSources] = useState<SourceRecord[]>([]);
  const [driveAuth, setDriveAuth] = useState<DriveAuthStatus | null>(null);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [recommendations, setRecommendations] = useState<AuditRecommendations | null>(null);
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [toast, setToast] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [androidDevices, setAndroidDevices] = useState<MtpDeviceInfo[]>([]);
  const [fullAuditJobId, setFullAuditJobId] = useState<string | null>(null);
  const [fullAudit, setFullAudit] = useState<FullAuditProgress | null>(null);
  const [auditOptions, setAuditOptions] = useState<AuditOptions>({
    includeGoogleDrive: true,
    includeGooglePhotos: true,
    includeGmail: true,
    includeThisPc: true,
    includePhone: true,
  });
  const [quota, setQuota] = useState<StorageQuota | null>(null);
  const [showTrashModal, setShowTrashModal] = useState(false);
  const [trashConfirm, setTrashConfirm] = useState("");
  const [copyOptions, setCopyOptions] = useState({
    googleDrive: true,
    phone: true,
    thisPc: true,
  });

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 5000);
  };

  const refresh = useCallback(async () => {
    try {
      const [d, s, da, a, st, rec, q] = await Promise.all([
        invoke<DashboardStats>("get_dashboard"),
        invoke<SourceRecord[]>("list_sources"),
        invoke<DriveAuthStatus>("get_drive_auth_status"),
        invoke<AuditEntry[]>("get_audit_log", { limit: 10 }),
        invoke<SetupStatus>("get_setup_status"),
        invoke<AuditRecommendations>("get_audit_recommendations").catch(() => null),
        invoke<StorageQuota>("get_google_storage_quota").catch(() => null),
      ]);
      setStats(d);
      setSources(s);
      setDriveAuth(da);
      setAudit(a);
      setSetup(st);
      setRecommendations(rec);
      setQuota(q);
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
    if (!fullAuditJobId) return;
    const interval = setInterval(async () => {
      try {
        const progress = await invoke<FullAuditProgress | null>("get_full_audit_status", {
          jobId: fullAuditJobId,
        });
        if (progress) {
          setFullAudit(progress);
          if (["completed", "failed", "cancelled"].includes(progress.status)) {
            setFullAuditJobId(null);
            setBusy(false);
            refresh();
            if (progress.status === "completed") {
              showToast("All done! Scroll down to see your results.");
            } else if (progress.status === "failed") {
              showToast(progress.error_message ?? "Something went wrong.");
            }
          }
        }
      } catch (e) {
        console.error(e);
      }
    }, 700);
    return () => clearInterval(interval);
  }, [fullAuditJobId, refresh]);

  const pickFolder = async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose your photo folder on this PC",
    });
    return selected as string | null;
  };

  const connectDrive = async () => {
    setBusy(true);
    try {
      const status = await invoke<DriveAuthStatus>("connect_google_drive");
      setDriveAuth(status);
      showToast(`Connected to Google Drive as ${status.email ?? "your account"}`);
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
        showToast('No phone found. Plug it in and choose "File transfer" on your phone.');
        return;
      }
      const device = devices[0];
      await invoke("connect_android_device", {
        storagePath: device.storage_path,
        deviceName: `${device.name} (${device.storage_name})`,
      });
      showToast(`Phone connected: ${device.name}`);
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runFullAudit = async () => {
    if (!stats?.vault_path && !setup?.vault_set) {
      showToast("Pick your PC photo folder first — click Setup.");
      setShowWizard(true);
      return;
    }
    setBusy(true);
    setFullAudit(null);
    try {
      const jobId = await invoke<string>("start_full_audit", {
        includeGoogleDrive: auditOptions.includeGoogleDrive,
        includeGooglePhotos: auditOptions.includeGooglePhotos,
        includeGmail: auditOptions.includeGmail,
        includeThisPc: auditOptions.includeThisPc,
        includePhone: auditOptions.includePhone,
      });
      setFullAuditJobId(jobId);
      setFullAudit({
        job_id: jobId,
        status: "running",
        phase: "starting",
        message: "Starting…",
        sources_total: 0,
        sources_done: 0,
        current_source_name: null,
        current_file: null,
        files_processed: 0,
        files_total: 0,
        error_message: null,
      });
    } catch (e) {
      showToast(String(e));
      setBusy(false);
    }
  };

  const cancelAudit = async () => {
    await invoke("cancel_full_audit");
    setFullAuditJobId(null);
    setBusy(false);
    showToast("Check stopped.");
  };

  const copyToVault = async (dryRun: boolean) => {
    const dest =
      stats?.vault_path ??
      (await invoke<string | null>("get_vault_path")) ??
      (await pickFolder());
    if (!dest) return;
    setBusy(true);
    try {
      const result = await invoke<CopyResult>("copy_uniques_to_vault", {
        destination: dest,
        dryRun,
        includeGoogleDrive: copyOptions.googleDrive,
        includePhone: copyOptions.phone,
        includeThisPc: copyOptions.thisPc,
      });
      if (dryRun) {
        showToast(
          `Preview: would save ${result.copied_count} files to your PC folder (${formatBytes(0)} — run for real when ready)`,
        );
      } else {
        showToast(
          `Saved ${result.copied_count} files to your PC folder. ${result.verified_count} verified — same file fingerprint before and after.`,
        );
      }
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const exportReceipt = async () => {
    setBusy(true);
    try {
      await invoke<{ html_path: string; json_path: string }>("export_audit_receipt");
      showToast("Report saved — opening reports folder.");
      await invoke("open_receipt_folder");
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const enableCleanup = async () => {
    setBusy(true);
    try {
      const status = await invoke<DriveAuthStatus>("connect_google_cleanup");
      setDriveAuth(status);
      showToast("Cleanup enabled — you can move verified duplicates to Google Drive Trash.");
      refresh();
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const previewTrash = async () => {
    setBusy(true);
    try {
      const result = await invoke<TrashResult>("move_duplicates_to_trash", {
        dryRun: true,
        confirmation: "",
      });
      showToast(
        `Preview: would move ${result.trashed_count} files to Google Drive Trash (${formatBytes(result.bytes_freed)}).`,
      );
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
    }
  };

  const confirmTrash = async () => {
    setBusy(true);
    try {
      const result = await invoke<TrashResult>("move_duplicates_to_trash", {
        dryRun: false,
        confirmation: trashConfirm,
      });
      setShowTrashModal(false);
      setTrashConfirm("");
      showToast(
        `Moved ${result.trashed_count} files to Google Drive Trash. Freed about ${formatBytes(result.bytes_freed)}.`,
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

  const androidSource = sources.find((s) => s.source_type === "android_mtp");
  const auditRunning = !!fullAuditJobId;
  const auditPct =
    fullAudit && fullAudit.sources_total > 0
      ? Math.round((fullAudit.sources_done / fullAudit.sources_total) * 100)
      : fullAudit?.status === "running"
        ? 8
        : 0;

  const dashboardBlocked = showWizard && !setup?.wizard_skipped;

  return (
    <div className="app">
      <header className="header">
        <div className="logo">
          <div className="logo-mark">D</div>
          <div>
            <h1>Deduper</h1>
            <p>Free up Google Drive — keep one safe copy on your PC</p>
          </div>
        </div>
        <div className="header-actions">
          <button className="btn btn-ghost" onClick={rerunWizard} title="Setup again">
            <Settings size={16} style={{ verticalAlign: "middle", marginRight: 4 }} />
            Setup
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
          onRunFirstCheck={() => {
            setShowWizard(false);
            runFullAudit();
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
            Finish setup
          </button>{" "}
          to connect Google Drive and your phone.
        </div>
      )}

      <div className={dashboardBlocked ? "dashboard-dimmed" : undefined}>
        {/* Hero — plain language */}
        <section className="hero">
          <div className="hero-label">Google Drive space you can free up</div>
          <div className="hero-value">{formatBytes(stats?.recoverable_bytes ?? 0)}</div>
          <p className="hero-sub">
            {recommendations?.summary_plain ??
              (stats?.recoverable_count
                ? `${formatCount(stats.recoverable_count)} files on Google Drive are already on your PC or phone — same photo, saved twice.`
                : "Run a full check below to see how much Google Drive space you can get back.")}
          </p>
        </section>

        {quota && driveAuth?.connected && (
          <section className="quota-panel">
            <div className="quota-head">
              <span>Your Google storage (Drive + Photos + Gmail share this space)</span>
              <strong>
                {quota.usage_display} used of {quota.limit_display}
              </strong>
            </div>
            <div className="progress-bar quota-bar">
              <div
                className="progress-fill quota-fill"
                style={{ width: `${Math.min(quota.percent_used, 100)}%` }}
              />
            </div>
            <p className="quota-free">{quota.free_display} free</p>
          </section>
        )}

        {/* One-button magic */}
        <section className="magic-panel">
          <div className="magic-head">
            <Sparkles size={22} className="magic-icon" />
            <div>
              <h2 className="magic-title">Check everything</h2>
              <p className="magic-desc">
                One button scans <strong>Google Drive</strong>, <strong>this PC</strong>, and{" "}
                <strong>your phone</strong> (if plugged in). We compare every photo and video to
                find duplicates — nothing gets deleted.
              </p>
            </div>
          </div>

          <div className="options-grid">
            <label className="option-chip">
              <input
                type="checkbox"
                checked={auditOptions.includeGoogleDrive}
                onChange={(e) =>
                  setAuditOptions((o) => ({ ...o, includeGoogleDrive: e.target.checked }))
                }
              />
              <span>
                <strong>Google Drive</strong>
                <small>files stored online in Google Drive</small>
              </span>
            </label>
            <label className="option-chip">
              <input
                type="checkbox"
                checked={auditOptions.includeGooglePhotos}
                onChange={(e) =>
                  setAuditOptions((o) => ({ ...o, includeGooglePhotos: e.target.checked }))
                }
              />
              <span>
                <strong>Google Photos</strong>
                <small>photos backed up to your Google account</small>
              </span>
            </label>
            <label className="option-chip">
              <input
                type="checkbox"
                checked={auditOptions.includeGmail}
                onChange={(e) =>
                  setAuditOptions((o) => ({ ...o, includeGmail: e.target.checked }))
                }
              />
              <span>
                <strong>Gmail attachments</strong>
                <small>large files attached to emails (5 MB+)</small>
              </span>
            </label>
            <label className="option-chip">
              <input
                type="checkbox"
                checked={auditOptions.includeThisPc}
                onChange={(e) =>
                  setAuditOptions((o) => ({ ...o, includeThisPc: e.target.checked }))
                }
              />
              <span>
                <strong>This PC</strong>
                <small>your photo folder on this computer</small>
              </span>
            </label>
            <label className="option-chip">
              <input
                type="checkbox"
                checked={auditOptions.includePhone}
                onChange={(e) =>
                  setAuditOptions((o) => ({ ...o, includePhone: e.target.checked }))
                }
              />
              <span>
                <strong>Your phone</strong>
                <small>Android, USB cable, File transfer mode</small>
              </span>
            </label>
          </div>

          <div className="magic-actions">
            <button
              className="btn btn-magic"
              onClick={runFullAudit}
              disabled={busy || auditRunning}
            >
              <Play size={18} style={{ marginRight: 8, verticalAlign: "middle" }} />
              {auditRunning ? "Checking…" : "Check all my photos & videos"}
            </button>
            {auditRunning && (
              <button className="btn btn-ghost" onClick={cancelAudit}>
                Stop
              </button>
            )}
          </div>

          {fullAudit && auditRunning && (
            <div className="progress-panel inline-progress">
              <div style={{ fontSize: "0.95rem", marginBottom: "0.35rem" }}>{fullAudit.message}</div>
              {fullAudit.current_source_name && (
                <div style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
                  Now: {fullAudit.current_source_name}
                </div>
              )}
              <div className="progress-bar">
                <div className="progress-fill" style={{ width: `${auditPct}%` }} />
              </div>
            </div>
          )}
        </section>

        <div className="safety-banner">
          <Shield size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
          <strong>We never delete anything for you.</strong> Deduper only shows what matches and
          what you can safely copy. You stay in control.
        </div>

        {/* Recommendations after scan */}
        {recommendations && recommendations.total_files_checked > 0 && (
          <section className="results-section">
            <div className="section-title">Your results</div>
            <div className="results-grid">
              <article className="result-card highlight">
                <div className="result-label">Already on your PC or phone</div>
                <div className="result-value">{formatBytes(recommendations.google_drive_duplicate_bytes)}</div>
                <p className="result-desc">
                  {formatCount(recommendations.google_drive_duplicate_count)} files on{" "}
                  <strong>Google Drive</strong> are exact copies of files you already have at home.
                  You can remove them from Google Drive to free space.
                </p>
              </article>
              <article className="result-card">
                <div className="result-label">Only on Google Drive</div>
                <div className="result-value">{formatBytes(recommendations.google_drive_only_bytes)}</div>
                <p className="result-desc">
                  {formatCount(recommendations.google_drive_only_count)} files exist online but not
                  on your PC or phone — copy them to your PC folder first.
                </p>
              </article>
              <article className="result-card">
                <div className="result-label">Only on your phone</div>
                <div className="result-value">{formatBytes(recommendations.phone_only_bytes)}</div>
                <p className="result-desc">
                  {formatCount(recommendations.phone_only_count)} files on your phone aren&apos;t
                  saved elsewhere yet.
                </p>
              </article>
              <article className="result-card">
                <div className="result-label">Google Photos</div>
                <div className="result-value">{formatCount(recommendations.google_photos_count)}</div>
                <p className="result-desc">items in your Google Photos library</p>
              </article>
              <article className="result-card">
                <div className="result-label">Gmail attachments</div>
                <div className="result-value">{formatBytes(recommendations.gmail_attachment_bytes)}</div>
                <p className="result-desc">
                  {formatCount(recommendations.gmail_attachment_count)} large attachments in Gmail
                </p>
              </article>
            </div>

            {recommendations.proof_samples.length > 0 && (
              <div className="proof-panel">
                <div className="section-title">
                  <CheckCircle2 size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
                  Proof — we checked these files
                </div>
                <p className="proof-intro">
                  Each file below is on <strong>Google Drive</strong> and also saved on your PC or
                  phone (same fingerprint — not just the same name).
                </p>
                <ul className="proof-list">
                  {recommendations.proof_samples.map((s) => (
                    <li key={s.drive_file_id} className="proof-item">
                      <div className="proof-file">
                        <strong>{s.filename}</strong>
                        <span className="proof-size">{formatBytes(s.size_bytes)}</span>
                      </div>
                      {s.copy_already_on_pc && (
                        <div className="proof-path">
                          Already saved at: {s.copy_already_on_pc}
                          {s.copy_location_label && (
                            <span className="proof-tag">{s.copy_location_label}</span>
                          )}
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </section>
        )}

        {/* Connection cards — simplified */}
        <div className="section-title">Your devices & accounts</div>
        <div className="grid">
          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <HardDrive size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                This PC
              </div>
              <span className={`card-badge ${stats?.vault_path ? "connected" : ""}`}>
                {stats?.vault_path ? "Folder set" : "Not set"}
              </span>
            </div>
            <p className="card-desc">
              Your main photo folder on this computer
              {stats?.vault_path && (
                <>
                  : <strong>{stats.vault_path}</strong>
                </>
              )}
            </p>
            <div className="card-actions">
              <button className="btn btn-secondary" onClick={rerunWizard} disabled={busy}>
                Change folder
              </button>
            </div>
          </article>

          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <FolderOpen size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                Google Drive
              </div>
              <span className={`card-badge ${driveAuth?.connected ? "connected" : ""}`}>
                {driveAuth?.connected ? "Connected" : "Not connected"}
              </span>
            </div>
            <p className="card-desc">
              {driveAuth?.connected
                ? `Signed in as ${driveAuth.email ?? "your Google account"}. This is the online storage that counts toward your Google space.`
                : "Connect your Google account so we can list photos and videos stored on Google Drive."}
            </p>
            <div className="card-actions">
              {!driveAuth?.connected ? (
                <button className="btn btn-primary" onClick={connectDrive} disabled={busy}>
                  Connect Google Drive
                </button>
              ) : (
                <button
                  className="btn btn-ghost"
                  onClick={async () => {
                    await invoke("disconnect_google_drive");
                    refresh();
                  }}
                >
                  Disconnect
                </button>
              )}
            </div>
          </article>

          <article className="card">
            <div className="card-head">
              <div className="card-title">
                <Smartphone size={18} style={{ verticalAlign: "middle", marginRight: 6 }} />
                Your phone
              </div>
              <span
                className={`card-badge ${androidSource || androidDevices.length ? "connected" : ""}`}
              >
                {androidSource ? "Connected" : androidDevices.length ? "Detected" : "Not connected"}
              </span>
            </div>
            <p className="card-desc">
              {androidSource
                ? `${androidSource.name} — plug in with USB and choose File transfer on your phone.`
                : "Android phone over USB. iPhone: copy photos to a folder on your PC and pick that folder in Setup."}
            </p>
            <div className="card-actions">
              <button className="btn btn-primary" onClick={detectAndConnectPhone} disabled={busy}>
                {androidSource ? "Refresh phone" : "Connect phone"}
              </button>
            </div>
          </article>
        </div>

        {/* Free up Google Drive — Phase 2 cleanup */}
        {(stats?.recoverable_count ?? 0) > 0 && (
          <section style={{ marginBottom: "1.75rem" }}>
            <div className="section-title">
              <Trash2 size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
              Free up Google Drive space
            </div>
            <div className="card cleanup-card">
              <p className="card-desc">
                Move files to <strong>Google Drive Trash</strong> only when they are already saved on
                your PC or phone (verified duplicates). You can restore from Trash in Google Drive for
                30 days.
              </p>
              {!driveAuth?.cleanup_enabled ? (
                <div className="card-actions">
                  <button className="btn btn-secondary" onClick={enableCleanup} disabled={busy}>
                    Enable Google Drive cleanup
                  </button>
                </div>
              ) : (
                <div className="card-actions">
                  <button className="btn btn-secondary" onClick={previewTrash} disabled={busy}>
                    Preview cleanup ({formatBytes(stats?.recoverable_bytes ?? 0)})
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={() => setShowTrashModal(true)}
                    disabled={busy}
                  >
                    Move duplicates to Trash
                  </button>
                </div>
              )}
            </div>
          </section>
        )}

        {/* Copy actions */}
        <section style={{ marginBottom: "1.75rem" }}>
          <div className="section-title">Save missing files to your PC folder</div>
          <div className="card">
            <p className="card-desc">
              Copy photos and videos that only exist in one place into your PC folder
              {stats?.vault_path && (
                <>
                  {" "}
                  (<strong>{stats.vault_path}</strong>)
                </>
              )}
              . We verify every copy — same file before and after.
            </p>
            <div className="options-grid compact">
              <label className="option-chip small">
                <input
                  type="checkbox"
                  checked={copyOptions.googleDrive}
                  onChange={(e) => setCopyOptions((o) => ({ ...o, googleDrive: e.target.checked }))}
                />
                <span>From Google Drive</span>
              </label>
              <label className="option-chip small">
                <input
                  type="checkbox"
                  checked={copyOptions.phone}
                  onChange={(e) => setCopyOptions((o) => ({ ...o, phone: e.target.checked }))}
                />
                <span>From your phone</span>
              </label>
              <label className="option-chip small">
                <input
                  type="checkbox"
                  checked={copyOptions.thisPc}
                  onChange={(e) => setCopyOptions((o) => ({ ...o, thisPc: e.target.checked }))}
                />
                <span>From other PC folders</span>
              </label>
            </div>
            <div className="card-actions">
              <button className="btn btn-secondary" onClick={() => copyToVault(true)} disabled={busy}>
                Preview first (no files changed)
              </button>
              <button className="btn btn-primary" onClick={() => copyToVault(false)} disabled={busy}>
                Save to my PC folder
              </button>
            </div>
          </div>
        </section>

        <section style={{ marginBottom: "1.75rem" }}>
          <div className="section-title">
            <FileText size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
            Proof report
          </div>
          <div className="card">
            <p className="card-desc">
              Save a report you can open in your browser and print to PDF — shows what Deduper found
              and sample files with proof paths.
            </p>
            <div className="card-actions">
              <button className="btn btn-secondary" onClick={exportReceipt} disabled={busy}>
                Save report (HTML + JSON)
              </button>
              <button
                className="btn btn-ghost"
                onClick={() => invoke("open_receipt_folder")}
                disabled={busy}
              >
                Open reports folder
              </button>
            </div>
          </div>
        </section>

        <section>
          <div className="section-title">Activity log</div>
          <p className="card-desc" style={{ marginBottom: "0.75rem" }}>
            Everything Deduper does is recorded here so you can see what happened and when.
          </p>
          <ul className="audit-list">
            {audit.length === 0 && (
              <li className="audit-item">No activity yet — run a check to get started.</li>
            )}
            {audit.map((entry) => (
              <li key={entry.id} className="audit-item">
                <span>
                  <strong>{humanAction(entry.action)}</strong>
                  {entry.dry_run && " (preview only)"}
                </span>
                <span>{new Date(entry.created_at).toLocaleString()}</span>
              </li>
            ))}
          </ul>
        </section>
      </div>

      {showTrashModal && (
        <div className="modal-overlay">
          <div className="modal">
            <h3>Move duplicates to Google Drive Trash?</h3>
            <p>
              This moves <strong>{formatCount(stats?.recoverable_count ?? 0)} files</strong> (
              {formatBytes(stats?.recoverable_bytes ?? 0)}) to Trash. Each file is already saved on
              your PC or phone. Type <strong>MOVE TO TRASH</strong> to confirm.
            </p>
            <input
              className="modal-input"
              value={trashConfirm}
              onChange={(e) => setTrashConfirm(e.target.value)}
              placeholder="MOVE TO TRASH"
            />
            <div className="modal-actions">
              <button className="btn btn-ghost" onClick={() => setShowTrashModal(false)}>
                Cancel
              </button>
              <button className="btn btn-danger" onClick={confirmTrash} disabled={busy}>
                Move to Trash
              </button>
            </div>
          </div>
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function humanAction(action: string): string {
  const map: Record<string, string> = {
    full_audit_started: "Started full check",
    full_audit_completed: "Finished full check",
    full_audit_cancelled: "Stopped full check",
    scan_started: "Started scanning",
    scan_completed: "Finished scanning",
    scan_cancelled: "Stopped scanning",
    copy_uniques_to_vault: "Saved files to PC folder",
    drive_connected: "Connected Google Drive",
    drive_disconnected: "Disconnected Google Drive",
    android_connected: "Connected phone",
    vault_path_set: "Set PC photo folder",
    wizard_completed: "Finished setup",
    wizard_skipped: "Skipped setup",
    phone_scan_skipped: "Skipped optional check",
    drive_trash_move: "Moved duplicates to Google Drive Trash",
    receipt_exported: "Saved proof report",
    cleanup_enabled: "Enabled Google Drive cleanup",
  };
  return map[action] ?? action.replace(/_/g, " ");
}
