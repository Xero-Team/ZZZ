use std::time::Duration;

/// A single decoded video frame in tightly packed BGRA8.
///
/// BGRA is the byte order GPUI's [`gpui::RenderImage`] consumes, which keeps
/// the render path allocation free apart from the frame itself.
#[derive(Clone, PartialEq, Eq)]
pub struct VideoFrame {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
    pts: Duration,
}

impl VideoFrame {
    /// Build a frame from tightly packed BGRA bytes.
    pub fn new(width: u32, height: u32, bgra: Vec<u8>, pts: Duration) -> Self {
        Self {
            width,
            height,
            bgra,
            pts,
        }
    }

    /// Frame width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Frame height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Presentation timestamp of the frame relative to the start of the file.
    pub fn pts(&self) -> Duration {
        self.pts
    }

    /// Number of bytes in one row of the pixel buffer.
    pub fn stride(&self) -> usize {
        self.width as usize * 4
    }

    /// The packed BGRA pixel buffer.
    pub fn bgra(&self) -> &[u8] {
        &self.bgra
    }

    /// Consume the frame, returning the packed BGRA pixel buffer.
    pub fn into_bgra(self) -> Vec<u8> {
        self.bgra
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pts", &self.pts)
            .finish_non_exhaustive()
    }
}
