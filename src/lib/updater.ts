import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface UpdateCheckResult {
  available: boolean;
  version?: string;
  update?: Update;
}

/** Check GitHub Releases for a newer signed build. No-op in dev builds. */
export async function checkForAppUpdate(): Promise<UpdateCheckResult> {
  try {
    const update = await check();
    if (update) {
      return { available: true, version: update.version, update };
    }
    return { available: false };
  } catch {
    return { available: false };
  }
}

export async function installAppUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
