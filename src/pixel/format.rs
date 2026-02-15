#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PixelFormat {
    /// 32-bit premultiplied ARGB。u32 で 0xAARRGGBB。
    Prgb32 = 1,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Prgb32 => 4,
        }
    }
}
