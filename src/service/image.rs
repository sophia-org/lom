//! Completed render storage held until the protocol releases the source.
use crate::protocol::ContentPixels;

/// One immutable image from the renderer, before Sophia takes a snapshot.
pub enum RenderImage {
    /// Packed pixels for the existing owned-ring path.
    Cpu(ContentPixels),
    /// Producer-completed GPU storage, retained through SourceReleased.
    Dma(sophia_shell_gpu::DmaFrame),
    /// Premultiplied BGRA rendered directly into an owner-acquired ring loan.
    /// The owner rechecks `identity` before committing or aborting the loan.
    RingFill {
        /// The exact payload loan; dropping it live faults the ring.
        fill: sophia_shell_client::OwnedRingSlotFill,
        /// Allocation identity the job was rendered for.
        identity: super::RenderIdentity,
        /// Rendered pixel width.
        width: u32,
        /// Rendered pixel height.
        height: u32,
        /// Checksum of the converted wire bytes.
        checksum: u64,
    },
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
            Self::RingFill {
                identity,
                width,
                height,
                ..
            } => f
                .debug_struct("RingFill")
                .field("identity", identity)
                .field("width", width)
                .field("height", height)
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
            Self::RingFill { width, .. } => *width,
        }
    }

    /// Logical pixel height.
    pub fn height(&self) -> u32 {
        match self {
            Self::Cpu(pixels) => pixels.height(),
            Self::Dma(frame) => frame.height(),
            Self::RingFill { height, .. } => *height,
        }
    }

    pub(super) fn byte_len(&self) -> usize {
        self.width() as usize * self.height() as usize * 4
    }

    pub(super) fn checksum(&self) -> Option<u64> {
        match self {
            Self::Cpu(pixels) => Some(super::pixel_checksum(pixels.bytes())),
            Self::Dma(_) => None,
            Self::RingFill { checksum, .. } => Some(*checksum),
        }
    }
}
