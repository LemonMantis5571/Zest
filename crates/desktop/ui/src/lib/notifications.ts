import { isTauri } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let permissionPromise: Promise<boolean> | null = null;

export function isWindowActive() {
  return (
    typeof document !== "undefined" &&
    document.visibilityState === "visible" &&
    document.hasFocus()
  );
}

async function notificationPermission() {
  if (permissionPromise) return permissionPromise;
  permissionPromise = (async () => {
    if (isTauri()) {
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      return granted;
    }

    if (!("Notification" in window)) return false;
    if (Notification.permission === "default") {
      return (await Notification.requestPermission()) === "granted";
    }
    return Notification.permission === "granted";
  })().catch(() => false);
  return permissionPromise;
}

/** Send an OS notification when Zest is not the active window. */
export async function notifyWhenAway(title: string, body: string) {
  if (isWindowActive()) return false;
  if (!(await notificationPermission())) return false;

  try {
    if (isTauri()) {
      sendNotification({ title, body });
    } else {
      new Notification(title, { body });
    }
    return true;
  } catch {
    return false;
  }
}
