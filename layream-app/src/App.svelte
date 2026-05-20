<script>
  import "./app.css";
  import { onMount } from "svelte";
  import { invoke, isTauri } from "./lib/tauri.js";
  import PresetView from "./views/PresetView.svelte";
  import CharacterView from "./views/CharacterView.svelte";
  import LibraryView from "./views/LibraryView.svelte";
  import TestView from "./views/TestView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import WorkspaceSelector from "./components/WorkspaceSelector.svelte";

  // Event name for broadcasting flush-on-close to all views.
  // Views (ChatView, HypaView, etc.) listen for this and persist pending state.
  const FLUSH_EVENT = "app-flush";
  // Grace period to allow listeners to complete their flush before window.destroy().
  // This is a soft bound: view listeners are fire-and-forget from App's perspective
  // (view files are not modified in this change), so we wait a short window for
  // their async work to settle. A future change can replace this with explicit
  // ack events from each view to remove the magic timeout.
  const FLUSH_GRACE_MS = 500;

  let activeTab = $state("preset");
  let oauthMessage = $state("");

  async function requestPermissions() {
    try { await invoke("request_storage_permission"); } catch (_) {}
    try { await invoke("request_notification_permission"); } catch (_) {}
  }

  async function checkPendingOAuth() {
    try {
      const result = await invoke("get_pending_oauth");
      if (result?.scheme && result?.code) {
        await processOAuthCode(result.scheme, result.code);
      }
    } catch (_) {}
  }

  async function processOAuthCode(scheme, code) {
    activeTab = "settings";
    oauthMessage = "Exchanging token...";
    try {
      let result;
      if (scheme.startsWith(VERTEX_SCHEME_PREFIX)) {
        result = await invoke("vertex_oauth_callback", { code });
        window.dispatchEvent(new CustomEvent("oauth-complete", { detail: "vertex" }));
      } else if (scheme.startsWith(GCA_SCHEME_PREFIX)) {
        result = await invoke("gca_oauth_callback", { code });
        window.dispatchEvent(new CustomEvent("oauth-complete", { detail: "gca" }));
      } else {
        oauthMessage = "Unknown OAuth scheme";
        setTimeout(() => { oauthMessage = ""; }, 3000);
        return;
      }
      oauthMessage = result || "Connected!";
    } catch (e) {
      oauthMessage = `OAuth failed: ${e}`;
    }
    setTimeout(() => { oauthMessage = ""; }, 5000);
  }

  onMount(async () => {
    requestPermissions();
    checkPendingOAuth();

    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") checkPendingOAuth();
    });

    window.__oauthCallback = (scheme, code) => processOAuthCode(scheme, code);

    try {
      const { onOpenUrl } = await import("@tauri-apps/plugin-deep-link");
      await onOpenUrl(async (urls) => {
        for (const url of urls) {
          await handleOAuthCallback(url);
        }
      });
    } catch (e) {
      console.warn("Deep link plugin not available:", e);
    }

    // Desktop close-requested handler: broadcast flush, wait briefly, then destroy.
    // Guarded by isTauri() — web has no window lifecycle to hook; Android either
    // returns no-op or the listen() call throws (caught below, default close preserved).
    if (!isTauri()) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const { emit } = await import("@tauri-apps/api/event");
      const win = getCurrentWindow();
      await win.listen("tauri://close-requested", async () => {
        try {
          await emit(FLUSH_EVENT);
          // Yield to let listeners start their flush, then give them a grace
          // window to complete before tearing the window down.
          await new Promise((r) => setTimeout(r, FLUSH_GRACE_MS));
        } catch (e) {
          console.error("Flush broadcast failed:", e);
        } finally {
          // Always destroy — failing to destroy would trap the user with a
          // window that won't close. Data loss on flush failure is logged above.
          await win.destroy();
        }
      });
    } catch (e) {
      console.warn("Close-requested listener not available:", e);
    }
  });

  const VERTEX_SCHEME_PREFIX = "com.googleusercontent.apps.317210024447";
  const GCA_SCHEME_PREFIX = "com.googleusercontent.apps.681255809395";

  async function handleOAuthCallback(url) {
    try {
      const code = extractCodeFromUrl(url);
      if (!code) return;
      const scheme = url.split(":")[0] || "";
      await processOAuthCode(scheme, code);
    } catch (e) {
      console.error("OAuth callback error:", e);
    }
  }

  function extractCodeFromUrl(url) {
    // Deep link URLs with custom schemes (e.g. com.googleusercontent.apps.xxx:/oauth2callback?code=...)
    // may not parse correctly with new URL() because the scheme contains dots.
    // Extract the code parameter manually from the query string.
    const queryStart = url.indexOf("?");
    if (queryStart === -1) return null;
    const query = url.substring(queryStart + 1);
    const params = new URLSearchParams(query);
    return params.get("code");
  }

  const tabs = [
    { id: "preset", label: "Preset", icon: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" },
    { id: "character", label: "Character", icon: "M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" },
    { id: "library", label: "Library", icon: "M4 19V5a2 2 0 012-2h12a2 2 0 012 2v14M4 19l4-4h12M4 19h16" },
    { id: "test", label: "Test", icon: "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" },
    { id: "settings", label: "Settings", icon: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z" },
  ];
</script>

<div class="header">
  <div style="display: flex; align-items: center; gap: 10px;">
    <h1 style="font-size: 18px; font-weight: 600;">Layream</h1>
    <span style="font-size: 12px; color: var(--fg3);">v0.3.1</span>
  </div>
  <WorkspaceSelector />
</div>

{#if oauthMessage}
<div style="padding: 8px 16px; background: var(--accent); color: #fff; font-size: 13px; text-align: center;">
  {oauthMessage}
</div>
{/if}

<div class="content">
  <div style:display={activeTab === "preset" ? "block" : "none"}>
    <PresetView />
  </div>
  <div style:display={activeTab === "character" ? "block" : "none"}>
    <CharacterView />
  </div>
  <div style:display={activeTab === "library" ? "block" : "none"}>
    <LibraryView />
  </div>
  <div style:display={activeTab === "test" ? "block" : "none"}>
    <TestView />
  </div>
  <div style:display={activeTab === "settings" ? "block" : "none"}>
    <SettingsView />
  </div>
</div>

<nav class="nav-bar">
  {#each tabs as tab}
    <button
      class="nav-item"
      class:active={activeTab === tab.id}
      onclick={() => (activeTab = tab.id)}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d={tab.icon} />
      </svg>
      {tab.label}
    </button>
  {/each}
</nav>
