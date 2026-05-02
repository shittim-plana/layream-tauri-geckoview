<script>
  import "./app.css";
  import PresetView from "./views/PresetView.svelte";
  import CharacterView from "./views/CharacterView.svelte";
  import TestView from "./views/TestView.svelte";
  import SettingsView from "./views/SettingsView.svelte";

  let activeTab = $state("preset");

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
