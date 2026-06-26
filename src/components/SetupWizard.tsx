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
  onRunFirstCheck?: () => void;
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

export function SetupWizard({ onComplete, onRunFirstCheck, onSkip, forceOpen }: SetupWizardProps) {
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
  const [detectingPhone, setDetectingPhone] = useState(false);
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
    if (step === 3 && oauthStatus && !oauthStatus.configured) {
      setShowAdvancedOAuth(true);
    }
  }, [step, oauthStatus]);

  // Phone detection runs PowerShell/COM on Windows — only when the user clicks "Detect phone"
  // (auto-detect on step enter used to freeze the webview on the main thread).

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
      const da = await invoke<DriveAuthStatus>("connect_google_drive");
      setDriveAuth(da);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const connectDrive = async () => {
    if (!oauthStatus?.configured) {
      setShowAdvancedOAuth(true);
      setError(
        "Google sign-in needs your OAuth app first. Paste Client ID and Secret below, click Save credentials, then Connect again.",
      );
      return;
    }
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
    setDetectingPhone(true);
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
      setDetectingPhone(false);
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
      else if (onRunFirstCheck) onRunFirstCheck();
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
              We'll help you free up space on <strong>Google Drive</strong> (your online Google
              storage) by finding photos and videos you already have on your <strong>PC</strong> or{" "}
              <strong>phone</strong>. Nothing gets deleted without your OK.
            </p>
            <WhyBlock title="What is Google Drive?">
              Google Drive is where Google stores your files online — it shares the same space as
              Gmail and Google Photos on your account. When Deduper says &quot;Google Drive,&quot;
              we mean files saved in your Google account online, not on your computer.
            </WhyBlock>
          </>
        )}

        {step === 1 && (
          <>
            <h2>
              <Shield size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              How it works — simple and safe
            </h2>
            <ul className="wizard-list">
              <li>
                <strong>We never delete anything.</strong> Deduper only shows you a report. You
                decide what to do.
              </li>
              <li>
                <strong>Google Drive is read-only.</strong> We list your online files to compare
                them — we don't change anything unless you ask later.
              </li>
              <li>
                <strong>Your PC photo folder</strong> is where we save copies of files that only
                exist in one place.
              </li>
            </ul>
            <WhyBlock title="Why is this safe?">
              Every step is logged. We fingerprint each file (like a unique ID) so we know it's
              the same photo — not just the same name.
            </WhyBlock>
          </>
        )}

        {step === 2 && (
          <>
            <h2>
              <FolderOpen size={22} style={{ verticalAlign: "middle", marginRight: 8 }} />
              Pick your PC photo folder
            </h2>
            <p>
              Choose a folder on <strong>this computer</strong> with plenty of free space. This is
              where Deduper saves photos and videos that aren't already backed up. Example:{" "}
              <code>D:\DadPhotos</code> or your Pictures folder.
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
            <WhyBlock title="What is this folder?">
              Think of it as your master backup on this PC. After checking, Deduper saves files
              that only exist on Google Drive or your phone into this folder — one safe copy at
              home.
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
                  Click below to sign in with your Google account in your browser. Deduper only
                  reads your Google Drive file list — nothing is deleted automatically.
                </p>
                <button
                  className="btn btn-primary btn-lg"
                  onClick={connectDrive}
                  disabled={busy}
                >
                  Connect Google Drive
                </button>
                {!oauthStatus?.configured && (
                  <div className="wizard-highlight muted">
                    This build needs your Google Cloud OAuth app once. Expand Advanced below, paste
                    Client ID and Secret, click Save credentials, then Connect.
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
                          Save and connect Google Drive
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
            <button className="btn btn-secondary" onClick={detectPhone} disabled={busy || detectingPhone}>
              {detectingPhone ? "Detecting phone…" : "Detect phone"}
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
              Ready to check everything
            </h2>
            <p>
              On the main screen, click <strong>Check all my photos &amp; videos</strong> — one
              button scans Google Drive, this PC, and your phone (if plugged in). The first check
              can take a while for large accounts.
            </p>
            <div className="wizard-tip">
              <Sparkles size={16} />
              <span>
                <strong>Tip:</strong> Start before bed and let it run overnight. Deduper saves
                progress so you can stop and continue later.
              </span>
            </div>
            <WhyBlock title="Why does it take time?">
              We read every photo and video to make sure it's a true duplicate — not just the same
              file name. Google Drive is fast; reading files from your phone over USB takes longer.
            </WhyBlock>
          </>
        )}

        {step === 6 && (
          <>
            <h2>You're all set!</h2>
            <ul className="wizard-summary">
              <li className={vaultPath ? "done" : ""}>
                PC photo folder: {vaultPath ?? "Not set"}
              </li>
              <li className={driveAuth?.connected || status?.drive_connected ? "done" : ""}>
                Google Drive:{" "}
                {driveAuth?.connected || status?.drive_connected
                  ? driveAuth?.email ?? status?.drive_email ?? "Connected"
                  : "Skipped — connect later in Setup"}
              </li>
              <li className={status?.android_connected ? "done" : ""}>
                Phone: {status?.android_connected ? status.android_device_name : "Skipped — plug in later"}
              </li>
            </ul>
            <p>
              Click <strong>Check all my photos &amp; videos</strong> on the main screen. Remember
              — we never delete anything for you.
            </p>
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
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => setStep(step + 1)}
                  disabled={busy || detectingPhone}
                >
                  Skip this step
                </button>
              )}
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setStep(step + 1)}
                disabled={busy || detectingPhone || !canNext()}
              >
                Next
              </button>
            </>
          ) : (
            <button type="button" className="btn btn-primary" onClick={() => finishWizard(false)} disabled={busy}>
              Check all my photos &amp; videos
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
