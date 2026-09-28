//! Owned CPU wire pixels produced by GPU readback, not a CPU raster fallback.

/// One immutable r5 resource. No padding, compression or hidden delta base.
#[derive(Debug, Eq, PartialEq)]
pub struct ContentPixels {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

/// A canonical whole-row chunk borrowed from its resource.
pub struct PixelChunk<'a> {
    /// Dense ordinal starting at zero.
    pub ordinal: u32,
    /// Byte offset in the immutable resource.
    pub offset: u64,
    /// No transport prefix is included in these pixel bytes.
    pub bytes: &'a [u8],
}

/// Borrowed iterator: producing upload records does not duplicate the image.
pub struct PixelChunks<'a> {
    bytes: &'a [u8],
    chunk_bytes: usize,
    offset: usize,
    ordinal: u32,
}

impl ContentPixels {
    /// The initial content ADR caps each resource at 4 MiB, even when both
    /// dimension limits would individually admit a larger image.
    pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

    /// Validate before rendering or allocating a readback target.
    pub fn byte_len(width: u32, height: u32) -> Result<usize, String> {
        if width == 0 || height == 0 || width > 8192 || height > 4096 {
            return Err("content dimensions exceed the r5 resource limits".into());
        }
        let bytes = u64::from(width) * u64::from(height) * 4;
        if bytes > Self::MAX_BYTES {
            return Err("content resource exceeds 4 MiB".into());
        }
        usize::try_from(bytes).map_err(|_| "content byte length is not representable".into())
    }

    /// Consume straight-alpha RGBA8 from the pinned Vello readback path.
    /// Convert in place to B,G,R,A with round-to-nearest premultiplication in
    /// sRGB non-linear channel space. There is no second full-image CPU copy.
    /// Alpha zero becomes canonical transparent black.
    pub fn from_rgba8(width: u32, height: u32, mut bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() != Self::byte_len(width, height)? {
            return Err("RGBA readback length does not equal width * height * 4".into());
        }
        for pixel in bytes.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            let premultiply = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
            let red = premultiply(pixel[0]);
            pixel[0] = premultiply(pixel[2]);
            pixel[1] = premultiply(pixel[1]);
            pixel[2] = red;
        }
        Ok(Self {
            width,
            height,
            bytes,
        })
    }

    /// Physical pixel width; stride is implicitly this value times four.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Physical pixel height.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Immutable wire bytes. Local ownership does not imply Engine acceptance.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Split into upload chunks of whole rows within the negotiated
    /// `max_chunk_bytes`, the file wire's canonical upload chunk. Non-final
    /// chunks fill every available whole row; an arbitrarily fragmented row is
    /// never produced. The SDK validates the Limits object this comes from.
    pub fn chunks(&self, max_chunk_bytes: u32) -> Result<PixelChunks<'_>, String> {
        let row = self.width * 4;
        let rows = max_chunk_bytes / row;
        if rows == 0 {
            return Err("content chunk cannot hold one complete row".into());
        }
        Ok(PixelChunks {
            bytes: &self.bytes,
            chunk_bytes: (rows * row) as usize,
            offset: 0,
            ordinal: 0,
        })
    }
}

impl<'a> Iterator for PixelChunks<'a> {
    type Item = PixelChunk<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.bytes.len() {
            return None;
        }
        let end = self.bytes.len().min(self.offset + self.chunk_bytes);
        let result = PixelChunk {
            ordinal: self.ordinal,
            offset: self.offset as u64,
            bytes: &self.bytes[self.offset..end],
        };
        self.offset = end;
        self.ordinal += 1;
        Some(result)
    }
}
