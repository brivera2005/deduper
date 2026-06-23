import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  ChevronDown,
  ChevronUp,
  Cloud,
  FolderOpen,
  Moon,
  Shield,
  Smartphone,
  Sparkles,
} from "lucide-react";

export interface WizardStatus {
  completed: boolean;
  skipped: boolean;
  completed_at: string | null;
  vault_path: string | null;
  google_configured: boolean;
  drive_connected: boolean;
  drive_email: string | null;
  android_connected: boolean;
  android_device_name: string | null;
  local_source_count: number;
  first_scan_done: boolean;
}

interface GoogleOAuthConfigStatus {
  configured: boolean;
  client_id_preview: string | null;
  source: string;
}

interface DriveAuthStatus {
  connected: boolean;
  email: string | null;
}

interface MtpDeviceInfo {
  name: string;
  storage_name: string;
  storage_path: string;
  connected: boolean;
  free_bytes: number | null;
  total_bytes: number | null;
}

interface SetupWizardProps {
  onComplete: () => void;
  onSkip: () => void;
  forceOpen?: boolean;
}

const STEPS = [
  { id: "welcome", title: "Welcome" },
  { id: "how", title: "How it works" },
  { id: "vault", title: "PC vault folder" },
  { id: "drive", title: "Google Drive" },
  { id: "android", title: "Android phone" },
  { id: "scan", title: "First scan" },
  { id: "done", title: "All set" },
] as const;

function formatBytes(bytes: number | null): string {
  if (!bytes) return "Unknown";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
}

function WhyBlock({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="why-block">
      <button type="button" className="why-toggle" onClick={() => setOpen(!open)}>
        {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        Why? {title}
      </button>
      {open && <div className="why-content">{children}</div>}
    </div>
  );
}

export function SetupWizard({ onComplete, onSkip, forceOpen }: SetupWizardProps) {
  const [step, setStep] = useState(0);
  const [status, setStatus] = useState<WizardStatus | null>(null);
  const [googleId, setGoogleId] = useState("");
  const [googleSecret, setGoogleSecret] = useState("");
  const [showAdvancedOAuth, setShowAdvancedOAuth] = useState(false);
  const [oauthStatus, setOauthStatus] = useState<GoogleOAuthConfigStatus | null>(null);
  const [driveAuth, setDriveAuth] = useState<DriveAuthStatus | null>(null);
  const [androidDevices, setAndroidDevices] = useState<MtpDeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [vaultPath, setVaultPath] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [ws, oauth, da] = await Promise.all([
      invoke<WizardStatus>("get_wizard_status"),
      invoke<GoogleOAuthConfigStatus>("get_google_oauth_config"),
      invoke<DriveAuthStatus>("get_drive_auth_status"),
    ]);
    setStatus(ws);
    setOauthStatus(oauth);
    setDriveAuth(da);
    setVaultPath(ws.vault_path);
  }, []);

  useEffect(() => {
    refresh().catch(console.error);
  }, [refresh]);

  useEffect(() => {
    if (step === 4) {
      invoke<MtpDeviceInfo[]>("detect_android_devices")
        .then(setAndroidDevices)
        .catch(() => setAndroidDevices([]));
    }
  }, [step]);

  if (!forceOpen && status?.completed) return null;

  const stepNum = step + 1;
  const progress = (stepNum / STEPS.length) * 100;

  const pickVault = async () => {
    setBusy(true);
    setError(null);
    try {
      const selected = await invoke<string | null>("pick_vault_folder");
      if (selected) {
        setVaultPath(selected);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveGoogleCreds = async () => {
    if (!googleId.trim() || !googleSecret.trim()) {
      setError("Please paste both Client ID and Client Secret.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await invoke("save_google_oauth_config", {
        clientId: googleId.trim(),
        clientSecret: googleSecret.trim(),
      });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const connectDrive = async () => {
    setBusy(true);
    setError(null);
    try {
      const da = await invoke<DriveAuthStatus>("connect_google_drive");
      setDriveAuth(da);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const detectPhone = async () => {
    setBusy(true);
    setError(null);
    try {
      const devices = await invoke<MtpDeviceInfo[]>("detect_android_devices");
      setAndroidDevices(devices);
      if (devices.length === 1) setSelectedDevice(devices[0].storage_path);
    } catch (e) {
      setError(String(e));
      setAndroidDevices([]);
    } finally {
      setBusy(false);
    }
  };

  const connectPhone = async () => {
    const device = androidDevices.find((d) => d.storage_path === selectedDevice);
    if (!device) {
      setError("Select your phone from the list first.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await invoke("connect_android_device", {
        storagePath: device.storage_path,
        deviceName: `${device.name} (${device.storage_name})`,
      });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const finishWizard = async (skipped: boolean) => {
    setBusy(true);
    try {
      await invoke("complete_wizard", { skipped });
      if (skipped) onSkip();
      else onComplete();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const canNext = () => {
    switch (step) {
      case 2:
        return !!vaultPath;
      case 3:
        return driveAuth?.connected || status?.drive_connected;
      default:
        return true;
    }
  };

  return (
    <div className="wizard-overlay">
      <div className="wizard wizard-wide">
        <div className="wizard-progress-head">
          <span>
            Step {stepNum} of {STEPS.length} — {STEPS[step].title}
          </span>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => finishWizard(true)}>
            Skip setup
          </button>
        </div>
        <div className="wizard-progress-bar">
          <div className="wizard-progress-fill" style={{ width: `${progress}%` }} />
        </div>

        {step === 0 && (
          <>
            <h2>Welcome to Deduper</h2>
            <p>
              We'll walk you through a few quick steps to find duplicate photos and videos
              across your phone, Google Drive, and PC — then safely copy the unique ones to
              one folder. Nothing gets deleted without your OK.
            </p>
            <WhyBlock title="What does Deduper do?">
              It compares your files by content (not just names) and shows you what's duplicated
              and what's safe to remove from Google Drive to free up space.
            </WhyBlock>
          </>
        )}

        {step === 1 && (
          <>
            <h2>
              <Shield size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              How it works — and what we never do
            </h2>
            <ul className="wizard-list">
              <li>
                <strong>We never auto-delete.</strong> Deduper only shows you a report. You decide
                what happens.
              </li>
              <li>
                <strong>Read-only Google access.</strong> We list your Drive files to compare them —
                we don't change anything unless you explicitly ask later.
              </li>
              <li>
                <strong>Your vault folder</strong> is where unique files get copied on your PC.
              </li>
            </ul>
            <WhyBlock title="Why is this safe?">
              Every action is logged. Cleanup is report-only by default. Even optional Drive trash
              moves require a separate confirmation step.
            </WhyBlock>
          </>
        )}

        {step === 2 && (
          <>
            <h2>
              <FolderOpen size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              Pick your PC vault folder
            </h2>
            <p>
              Choose a folder on this computer where Deduper will copy your unique photos and
              videos. Pick something with plenty of free space — like{" "}
              <code>D:\PhotoVault</code> or your Pictures folder.
            </p>
            {vaultPath ? (
              <div className="wizard-highlight">
                Selected: <strong>{vaultPath}</strong>
              </div>
            ) : (
              <div className="wizard-highlight muted">No folder selected yet</div>
            )}
            <button className="btn btn-secondary" onClick={pickVault} disabled={busy}>
              Choose folder…
            </button>
            <WhyBlock title="What is the vault?">
              It's your master backup folder. After scanning, Deduper copies files that exist
              nowhere else into this folder so you have one safe copy on your PC.
            </WhyBlock>
          </>
        )}

        {step === 3 && (
          <>
            <h2>
              <Cloud size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              Connect Google Drive
            </h2>
            {(driveAuth?.connected || status?.drive_connected) ? (
              <div className="wizard-highlight success">
                Connected as <strong>{driveAuth?.email ?? status?.drive_email ?? "your account"}</strong>
              </div>
            ) : (
              <>
                <p>
                  Click <strong>Connect Google Drive</strong> to sign in with your Google account in
                  your browser. Deduper only gets read-only access to compare your photos — nothing
                  is deleted automatically.
                </p>
                <button
                  className="btn btn-primary btn-lg"
                  onClick={connectDrive}
                  disabled={busy || !oauthStatus?.configured}
                >
                  Connect Google Drive
                </button>
                {!oauthStatus?.configured && (
                  <div className="wizard-highlight muted">
                    Sign-in is not bundled in this build. Expand Advanced below if you are building
                    from source with your own Google Cloud app.
                  </div>
                )}
                <div className="why-block">
                  <button
                    type="button"
                    className="why-toggle"
                    onClick={() => setShowAdvancedOAuth(!showAdvancedOAuth)}
                  >
                    {showAdvancedOAuth ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    Advanced: use your own Google Cloud app
                  </button>
                  {showAdvancedOAuth && (
                    <div className="why-content">
                      <p className="wizard-advanced-note">
                        For developers and open-source builds only. Paste a Desktop OAuth client from{" "}
                        <a
                          href="https://console.cloud.google.com/apis/credentials"
                          target="_blank"
                          rel="noreferrer"
                        >
                          Google Cloud Console
                        </a>
                        . Redirect URI:{" "}
                        <code>http://127.0.0.1:8888/oauth/callback</code>
                      </p>
                      {oauthStatus?.configured && oauthStatus.source === "config" && (
                        <div className="wizard-highlight">
                          Using your credentials ({oauthStatus.client_id_preview})
                        </div>
                      )}
                      <div className="wizard-form">
                        <label>
                          Client ID
                          <input
                            type="text"
                            value={googleId}
                            onChange={(e) => setGoogleId(e.target.value)}
                            placeholder="123456.apps.googleusercontent.com"
                          />
                        </label>
                        <label>
                          Client Secret
                          <input
                            type="password"
                            value={googleSecret}
                            onChange={(e) => setGoogleSecret(e.target.value)}
                            placeholder="Paste from Google Cloud Console"
                          />
                        </label>
                        <button
                          className="btn btn-secondary"
                          onClick={saveGoogleCreds}
                          disabled={busy}
                        >
                          Save credentials
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </>
            )}
            <WhyBlock title="Google shows an unverified app warning?">
              That is normal until the app is verified. Choose Advanced, then Continue to Deduper.
              You are signing in with your own Google account — Deduper never sees your password.
            </WhyBlock>
          </>
        )}

        {step === 4 && (
          <>
            <h2>
              <Smartphone size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              Connect your Android phone
            </h2>
            <ol className="wizard-list numbered">
              <li>Plug your phone into this PC with a USB cable.</li>
              <li>Unlock your phone.</li>
              <li>
                On the phone notification, tap <strong>USB</strong> and choose{" "}
                <strong>File transfer</strong> or <strong>Transfer files</strong> (not "Charge only").
              </li>
              <li>Click "Detect phone" below.</li>
            </ol>
            <button className="btn btn-secondary" onClick={detectPhone} disabled={busy}>
              Detect phone
            </button>
            {androidDevices.length > 0 && (
              <div className="wizard-device-list">
                {androidDevices.map((d) => (
                  <label key={d.storage_path} className="wizard-device-option">
                    <input
                      type="radio"
                      name="device"
                      checked={selectedDevice === d.storage_path}
                      onChange={() => setSelectedDevice(d.storage_path)}
                    />
                    <span>
                      <strong>{d.name}</strong> — {d.storage_name}
                      {d.total_bytes && (
                        <> ({formatBytes(d.free_bytes)} free of {formatBytes(d.total_bytes)})</>
                      )}
                    </span>
                  </label>
                ))}
                <button className="btn btn-primary" onClick={connectPhone} disabled={busy || !selectedDevice}>
                  Connect phone
                </button>
              </div>
            )}
            {status?.android_connected && (
              <div className="wizard-highlight success">
                Phone connected: {status.android_device_name}
              </div>
            )}
            <WhyBlock title="Can I skip this?">
              Yes — you can always copy photos from your phone to a folder manually and add that
              folder on the dashboard instead.
            </WhyBlock>
          </>
        )}

        {step === 5 && (
          <>
            <h2>
              <Moon size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              Your first scan
            </h2>
            <p>
              After setup, go to the dashboard and click <strong>Scan</strong> on each source
              (PC folder, Google Drive, phone). The first scan can take a while — especially
              for large Google Drive accounts or phones with lots of photos.
            </p>
            <div className="wizard-tip">
              <Sparkles size={16} />
              <span>
                <strong>Tip:</strong> Start a scan before bed and let it run overnight. Deduper
                saves progress, so you can pause and resume if needed.
              </span>
            </div>
            <WhyBlock title="Why does scanning take time?">
              Deduper reads every file's content to find true duplicates — not just matching
              filenames. Google Drive scans use file fingerprints from Google; phone scans read
              files over USB which can be slower.
            </WhyBlock>
          </>
        )}

        {step === 6 && (
          <>
            <h2>You're all set!</h2>
            <ul className="wizard-summary">
              <li className={vaultPath ? "done" : ""}>
                Vault folder: {vaultPath ?? "Not set"}
              </li>
              <li className={driveAuth?.connected || status?.drive_connected ? "done" : ""}>
                Google Drive:{" "}
                {driveAuth?.connected || status?.drive_connected
                  ? driveAuth?.email ?? status?.drive_email ?? "Connected"
                  : "Skipped"}
              </li>
              <li className={status?.android_connected ? "done" : ""}>
                Android phone: {status?.android_connected ? status.android_device_name : "Skipped"}
              </li>
            </ul>
            <p>Head to the dashboard to start scanning. Remember — nothing gets deleted automatically.</p>
          </>
        )}

        {error && <div className="wizard-error">{error}</div>}

        <div className="wizard-nav">
          {step > 0 && (
            <button type="button" className="btn btn-secondary" onClick={() => setStep(step - 1)} disabled={busy}>
              Back
            </button>
          )}
          <div className="wizard-nav-spacer" />
          {step < STEPS.length - 1 ? (
            <>
              {[3, 4].includes(step) && (
                <button type="button" className="btn btn-ghost" onClick={() => setStep(step + 1)} disabled={busy}>
                  Skip this step
                </button>
              )}
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setStep(step + 1)}
                disabled={busy || !canNext()}
              >
                Next
              </button>
            </>
          ) : (
            <button type="button" className="btn btn-primary" onClick={() => finishWizard(false)} disabled={busy}>
              Go to dashboard
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
