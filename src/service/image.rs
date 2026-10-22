//! Completed render storage held until the protocol releases the source.
use crate::protocol::ContentPixels;

/// One immutable image from the renderer, before Sophia takes a snapshot.
pub enum RenderImage {
    /// Packed pixels for the existing owned-ring path.
    Cpu(ContentPixels),
    /// Producer-completed GPU storage, retained through SourceReleased.
    Dma(sophia_shell_gpu::DmaFrame),
}

impl std::fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpu(pixels) => f.debug_tuple("Cpu").field(pixels).finish(),
            Self::Dma(frame) => f
                .debug_struct("Dma")
                .field("width", &frame.width())
                .field("height", &frame.height())
                .finish(),
        }
    }
}

impl From<ContentPixels> for RenderImage {
    fn from(pixels: ContentPixels) -> Self {
        Self::Cpu(pixels)
    }
}

impl RenderImage {
    /// Logical pixel width.
    pub fn width(&self) -> u32 {
        match self {
            Self::Cpu(pixels) => pixels.width(),
            Self::Dma(frame) => frame.width(),
        }
    }

    /// Logical pixel height.
    pub fn height(&self) -> u32 {
        match self {
            Self::Cpu(pixels) => pixels.height(),
            Self::Dma(frame) => frame.height(),
        }
    }

    pub(super) fn byte_len(&self) -> usize {
        self.width() as usize * self.height() as usize * 4
    }

    pub(super) fn checksum(&self) -> Option<u64> {
        match self {
            Self::Cpu(pixels) => Some(super::pixel_checksum(pixels.bytes())),
            Self::Dma(_) => None,
        }
    }
}
