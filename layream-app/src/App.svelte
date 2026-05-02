<script>
  import "./app.css";
  import { onMount } from "svelte";
  import { invoke } from "./lib/tauri.js";
  import PresetView from "./views/PresetView.svelte";
  import CharacterView from "./views/CharacterView.svelte";
  import TestView from "./views/TestView.svelte";
  import SettingsView from "./views/SettingsView.svelte";

  let activeTab = $state("preset");
  let oauthMessage = $state("");

  onMount(async () => {
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
  });

  async function handleOAuthCallback(url) {
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get("code");
      if (!code) return;

      activeTab = "settings";
      oauthMessage = "Exchanging token...";

      try {
        const result = await invoke("vertex_oauth_callback", { code });
        oauthMessage = result;
      } catch {
        try {
          const result = await invoke("gca_oauth_callback", { code });
          oauthMessage = result;
        } catch (e) {
          oauthMessage = `OAuth failed: ${e}`;
        }
      }

      setTimeout(() => { oauthMessage = ""; }, 3000);
    } catch (e) {
      console.error("OAuth callback error:", e);
    }
  }

  const tabs = [
    { id: "preset", label: "Preset" },
    { id: "character", label: "Character" },
    { id: "test", label: "Test" },
    { id: "settings", label: "Settings" },
  ];
</script>

<div class="header">
  <h1 style="font-size: 18px; font-weight: 600;">Layream</h1>
  <span style="font-size: 12px; color: var(--text-dim);">v0.1.0</span>
</div>

<div class="tabs">
  {#each tabs as tab}
    <button
      class="tab"
      class:active={activeTab === tab.id}
      onclick={() => (activeTab = tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</div>

{#if oauthMessage}
<div style="padding: 8px 16px; background: var(--primary); font-size: 13px; text-align: center;">
  {oauthMessage}
</div>
{/if}

<div class="content">
  {#if activeTab === "preset"}
    <PresetView />
  {:else if activeTab === "character"}
    <CharacterView />
  {:else if activeTab === "test"}
    <TestView />
  {:else if activeTab === "settings"}
    <SettingsView />
  {/if}
</div>
