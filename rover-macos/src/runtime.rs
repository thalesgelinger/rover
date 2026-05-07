use crate::abi::HostCallbacks;
use crate::renderer::MacosRenderer;
use anyhow::Result;
use rover_ui::app::App;
use rover_ui::events::UiEvent;
use rover_ui::ui::NodeId;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

pub struct MacosRuntime {
    app: App<MacosRenderer>,
}

impl MacosRuntime {
    pub fn new(callbacks: HostCallbacks) -> mlua::Result<Self> {
        let renderer = MacosRenderer::new(callbacks);
        let app = App::new(renderer)?;
        Ok(Self { app })
    }

    pub fn load_lua(&mut self, source: &str) -> mlua::Result<()> {
        self.app.run_script(source)?;
        self.app.mount()
    }

    pub fn tick(&mut self) -> mlua::Result<bool> {
        self.app.tick()
    }

    pub fn next_wake_ms(&self) -> i32 {
        self.app
            .next_wake_time()
            .map(|wake| {
                let now = Instant::now();
                if wake <= now {
                    0
                } else {
                    wake.saturating_duration_since(now)
                        .as_millis()
                        .min(i32::MAX as u128) as i32
                }
            })
            .unwrap_or(-1)
    }

    pub fn dispatch_click(&mut self, id: u32) {
        self.app.push_event(UiEvent::Click {
            node_id: NodeId::from_u32(id),
        });
    }

    pub fn dispatch_input(&mut self, id: u32, value: String) {
        self.app.push_event(UiEvent::Change {
            node_id: NodeId::from_u32(id),
            value,
        });
    }

    pub fn dispatch_submit(&mut self, id: u32, value: String) {
        self.app.push_event(UiEvent::Submit {
            node_id: NodeId::from_u32(id),
            value,
        });
    }

    pub fn dispatch_toggle(&mut self, id: u32, checked: bool) {
        self.app.push_event(UiEvent::Toggle {
            node_id: NodeId::from_u32(id),
            checked,
        });
    }

    pub fn set_viewport(&mut self, width: u16, height: u16) {
        self.app.set_viewport_size(width, height);
        self.app
            .renderer()
            .set_viewport_size(width as f32, height as f32);
    }
}

pub fn run_file(file: &Path, _args: &[String]) -> Result<()> {
    let source = std::fs::read_to_string(file)?;
    let mut runtime = MacosRuntime::new(HostCallbacks::default())?;
    runtime.load_lua(&source)?;

    runtime.tick()?;
    println!("macOS runtime mounted. AppKit host bridge scaffold ready.");

    Ok(())
}

pub fn build_host() -> Result<PathBuf> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve workspace root"))?;
    let host_path = workspace_root.join("target/debug/rover-macos-host");
    let script = crate_dir.join("swift/build.sh");

    let status = Command::new(&script)
        .arg(&host_path)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run {}: {}", script.display(), e))?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "failed to build macOS host with status {}",
            status
        ));
    }

    Ok(host_path)
}

pub fn launch_file(file: &Path, _args: &[String]) -> Result<()> {
    let host = build_host()?;
    let status = Command::new(&host)
        .arg(file)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to launch {}: {}", host.display(), e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("macOS host exited with status {}", status));
    }

    Ok(())
}
