use crate::SharedSignalRuntime;
use crate::node::NodeId;
use crate::renderer::TuiRenderer;
use std::io;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum KeyModifier {
    Shift,
    Control,
    Alt,
    Meta,
    Super,
}

#[derive(Debug, Clone)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    KeyDown {
        key: String,
        modifiers: Vec<KeyModifier>,
    },
    KeyUp {
        key: String,
    },
    MouseDown {
        button: MouseButton,
        x: u16,
        y: u16,
    },
    MouseUp {
        button: MouseButton,
        x: u16,
        y: u16,
    },
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseScroll {
        delta: i32,
    },
    Resize {
        width: u16,
        height: u16,
    },
    FocusGained,
    FocusLost,
    Tick {
        delta_ms: u64,
    },
    Quit,
}

pub trait PlatformHandler {
    fn init(&mut self) -> io::Result<()>;
    fn wait_for_event(&mut self, timeout: Duration) -> io::Result<Option<PlatformEvent>>;
    fn render(&mut self) -> io::Result<()>;
    fn cleanup(&mut self) -> io::Result<()>;
}

pub struct TuiPlatform {
    renderer: TuiRenderer,
    runtime: SharedSignalRuntime,
    last_tick: std::time::Instant,
}

impl TuiPlatform {
    pub fn new(root_node_id: NodeId, runtime: SharedSignalRuntime) -> io::Result<Self> {
        let renderer = TuiRenderer::new(root_node_id, runtime.clone())?;
        Ok(Self {
            renderer,
            runtime,
            last_tick: std::time::Instant::now(),
        })
    }
}

impl PlatformHandler for TuiPlatform {
    fn init(&mut self) -> io::Result<()> {
        self.renderer.init()?;
        Ok(())
    }

    fn wait_for_event(&mut self, timeout: Duration) -> io::Result<Option<PlatformEvent>> {
        use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

        let now = std::time::Instant::now();

        while now.elapsed() < timeout {
            if self.runtime.tick() {
                let delta_ms = self.last_tick.elapsed().as_millis() as u64;
                self.last_tick = std::time::Instant::now();
                return Ok(Some(PlatformEvent::Tick { delta_ms }));
            }

            if event::poll(Duration::from_millis(10))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        return Ok(Some(PlatformEvent::KeyDown {
                            key: format!("{:?}", key.code),
                            modifiers: vec![],
                        }));
                    }
                    Event::Resize(cols, rows) => {
                        self.renderer.resize(cols, rows);
                        return Ok(Some(PlatformEvent::Resize {
                            width: cols,
                            height: rows,
                        }));
                    }
                    Event::FocusGained => {
                        return Ok(Some(PlatformEvent::FocusGained));
                    }
                    Event::FocusLost => {
                        return Ok(Some(PlatformEvent::FocusLost));
                    }
                    _ => {}
                }
            }
        }

        Ok(None)
    }

    fn render(&mut self) -> io::Result<()> {
        self.renderer.render()?;
        Ok(())
    }

    fn cleanup(&mut self) -> io::Result<()> {
        self.renderer.cleanup()?;
        Ok(())
    }
}
