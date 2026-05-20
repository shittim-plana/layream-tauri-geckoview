# Tauri 2.0 + GeckoView (Firefox Engine) — Android Integration Guide

> First known integration of GeckoView with Tauri 2.0 on Android.

## Why GeckoView?

- **OAuth**: Google blocks OAuth in Android's system WebView. GeckoView is a real browser — OAuth works.
- **Consistency**: Same Firefox engine on every device, no dependency on system WebView version.
- **Independence**: No Google/Chrome dependency for rendering.

## Architecture

```
┌─────────────────────────────────┐
│  Tauri 2.0 App                  │
│  ┌───────────┐  ┌─────────────┐ │
│  │ Rust .so  │  │ GeckoView   │ │
│  │ (backend) │  │ (Firefox)   │ │
│  │           │  │             │ │
│  │ commands  │  │ Svelte app  │ │
│  │ OAuth     │←→│ loaded via  │ │
│  │ API calls │  │ AssetServer │ │
│  └───────────┘  └─────────────┘ │
│        ↕              ↕         │
│  ┌───────────┐  ┌─────────────┐ │
│  │ Rust IPC  │  │ WebExtension│ │
│  │ (Rust.kt) │←→│ IPC bridge  │ │
│  └───────────┘  └─────────────┘ │
└─────────────────────────────────┘
```

Tauri's system WebView still initializes (for Rust IPC), but GeckoView is the visible rendering engine.

## Files

### Non-generated (survive `tauri android build`):

| File | Purpose |
|------|---------|
| `MainActivity.kt` | GeckoView creation, AssetServer start, IPC WebExtension registration |
| `AssetServer.kt` | Localhost HTTP server serving built frontend from Android assets |

### Build config (may need re-applying after `tauri android init`):

| File | Changes |
|------|---------|
| `build.gradle.kts` | GeckoView dependency, Mozilla Maven repo, abiFilters |
| `proguard-rules.pro` | GeckoView keep rules |
| `gradle.properties` | Memory limits for OOM prevention |

### Assets (survive rebuild):

| File | Purpose |
|------|---------|
| `assets/ipc-extension/content.js` | Exposes `window.ipc.postMessage()` via `cloneInto`/`exportFunction` |
| `assets/ipc-extension/manifest.json` | WebExtension manifest with `nativeMessagingFromContent` |

## Setup Steps

### 1. Add GeckoView dependency

`app/build.gradle.kts`:
```kotlin
repositories {
    maven { url = uri("https://maven.mozilla.org/maven2") }
}

dependencies {
    implementation("org.mozilla.geckoview:geckoview-arm64-v8a:128.0.20240725162350")
}

// Restrict to arm64 only (GeckoView is architecture-specific)
android {
    defaultConfig {
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }
}
```

### 2. Create AssetServer.kt

Place in the non-generated package directory (e.g., `com/yourpackage/`), NOT in `generated/`.

Minimal localhost HTTP server that serves files from Android's `assets/` directory.

Key points:
- Bind to `127.0.0.1:0` (localhost only, random port)
- Serve built frontend files (index.html, JS, CSS)
- `@Volatile` on the `running` flag
- Path traversal prevention (reject `..`)

### 3. Modify MainActivity.kt

```kotlin
class MainActivity : TauriActivity() {
    private var geckoView: GeckoView? = null
    private var geckoSession: GeckoSession? = null
    private var assetServer: AssetServer? = null

    companion object {
        @Volatile private var sRuntime: GeckoRuntime? = null
        // ... singleton pattern for GeckoRuntime
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState) // Tauri initializes Rust backend
        initGeckoView()                   // Then we overlay GeckoView
    }

    private fun initGeckoView() {
        // 1. Start AssetServer
        val server = AssetServer(applicationContext)
        val port = server.start()

        // 2. Create GeckoRuntime (singleton)
        val runtime = getOrCreateRuntime(this)

        // 3. Create GeckoSession + GeckoView
        val session = GeckoSession(...)
        session.open(runtime)
        val view = GeckoView(this)
        view.setSession(session)

        // 4. Replace content view (GeckoView on top of Tauri's WebView)
        val container = FrameLayout(this)
        container.fitsSystemWindows = true
        container.addView(view, ...)
        setContentView(container)

        // 5. Register IPC WebExtension
        runtime.webExtensionController
            .ensureBuiltIn("resource://android/assets/ipc-extension/", "tauri-ipc@app")
            .accept({ extension ->
                extension?.setMessageDelegate(messageDelegate, "ipc")
            }, { error -> ... })

        // 6. Load app from AssetServer
        session.loadUri("http://localhost:$port/")
    }
}
```

### 4. Create IPC WebExtension

`assets/ipc-extension/manifest.json`:
```json
{
    "manifest_version": 2,
    "name": "Tauri IPC Bridge",
    "version": "1.0",
    "content_scripts": [{
        "matches": ["<all_urls>"],
        "js": ["content.js"],
        "run_at": "document_start",
        "all_frames": true
    }],
    "permissions": ["nativeMessaging", "nativeMessagingFromContent"]
}
```

`assets/ipc-extension/content.js`:
```javascript
(function() {
    var ipcObject = cloneInto({ postMessage: null }, window, { cloneFunctions: true });
    exportFunction(function(message) {
        browser.runtime.sendNativeMessage("ipc", {
            payload: message,
            url: window.location.href
        });
    }, ipcObject, { defineAs: "postMessage" });
    Object.defineProperty(window.wrappedJSObject, "ipc", {
        value: ipcObject, writable: false, configurable: false, enumerable: true
    });
})();
```

### 5. ProGuard rules

```
-keep class org.mozilla.geckoview.** { *; }
-keep class org.mozilla.gecko.** { *; }
-dontwarn java.beans.**
```

### 6. Copy frontend to assets

After `npm run build`, copy `dist/*` to `app/src/main/assets/`.

## Build

```bash
# 1. Build frontend
cd layream-app && npm run build

# 2. Copy to Android assets
cp -r dist/* src-tauri/gen/android/app/src/main/assets/

# 3. Build APK (Tauri CLI handles Rust + Gradle)
CARGO_BUILD_JOBS=2 npm run tauri android build -- --apk --target aarch64
```

## Versions

- GeckoView 128 ESR (`geckoview-arm64-v8a:128.0.20240725162350`) — stable, minSdk 21
- Tauri 2.x
- Tested on Android 12+ (arm64)

## Known Limitations

- APK size ~184MB (Firefox engine included)
- `getCookies()` returns empty (GeckoView cookie API is async)
- Tauri's `generated/` files get overwritten on build — custom code must be in non-generated directories
- GeckoView is arm64-only in this setup (add other architecture artifacts for multi-ABI)

## Future

Replace GeckoView with [Servo/Verso](https://github.com/nickel-org/nickel.rs) — a Rust-native browser engine — for full Rust stack rendering.
