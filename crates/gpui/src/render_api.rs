use crate::{PlatformWindow, Scene};

/// Backend-neutral scene view passed to a renderer.
#[derive(Clone, Copy)]
pub(crate) struct RenderScene<'a> {
    pub(crate) scene: &'a Scene,
}

impl<'a> RenderScene<'a> {
    pub(crate) fn new(scene: &'a Scene) -> Self {
        Self { scene }
    }
}

/// Target receiving one completed scene submission.
pub(crate) trait RenderTarget {
    fn submit_scene(&mut self, scene: RenderScene<'_>);
}

/// Result of submitting one completed frame to a render target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FrameSubmission {
    pub(crate) submitted: bool,
}

/// Renderer contract independent of window lifecycle and application entities.
pub(crate) trait Renderer {
    fn submit(&mut self, scene: RenderScene<'_>, target: &mut dyn RenderTarget) -> FrameSubmission;
}

struct CompatibilityRenderer;

impl Renderer for CompatibilityRenderer {
    fn submit(&mut self, scene: RenderScene<'_>, target: &mut dyn RenderTarget) -> FrameSubmission {
        target.submit_scene(scene);
        FrameSubmission { submitted: true }
    }
}

struct PlatformWindowTarget<'a> {
    window: &'a mut dyn PlatformWindow,
}

impl RenderTarget for PlatformWindowTarget<'_> {
    fn submit_scene(&mut self, scene: RenderScene<'_>) {
        self.window.draw(scene.scene);
    }
}

/// Submit through the existing `PlatformWindow::draw` adapter while the backends migrate.
pub(crate) fn submit_compat(
    window: &mut dyn PlatformWindow,
    scene: RenderScene<'_>,
) -> FrameSubmission {
    let mut target = PlatformWindowTarget { window };
    CompatibilityRenderer.submit(scene, &mut target)
}
