use crate::layout::LayoutEngine;
use crate::node::{NodeArena, NodeId, RenderCommand};
use std::io;

pub trait Renderer {
    fn apply(&mut self, cmd: &RenderCommand, arena: &NodeArena, layout: &LayoutEngine);
    fn render_frame(
        &mut self,
        root: NodeId,
        arena: &NodeArena,
        layout: &LayoutEngine,
        runtime: &crate::SharedSignalRuntime,
    ) -> io::Result<()>;
}
