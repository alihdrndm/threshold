import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

/**
 * The only module that talks to the Rust core. Keeping `invoke` calls behind
 * typed functions here means command-name typos surface at one call site
 * instead of being scattered string literals across the UI.
 */

export function ping(): Promise<string> {
  return invoke<string>("ping");
}

export function appVersion(): Promise<string> {
  return getVersion();
}

/** Close the intention window. The honourable exit must never be more than this. */
export function dismissPopup(): Promise<void> {
  return invoke<void>("dismiss_popup");
}

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
