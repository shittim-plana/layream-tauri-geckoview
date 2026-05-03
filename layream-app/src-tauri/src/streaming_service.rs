use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.shittimplana.layream";

#[cfg(target_os = "android")]
pub struct StreamingServiceHandle<R: Runtime>(pub PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R, ()>::new("streaming-service")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin(PLUGIN_IDENTIFIER, "StreamingServicePlugin")?;
                app.manage(StreamingServiceHandle(handle));
            }
            let _ = app;
            let _ = api;
            Ok(())
        })
        .build()
}
