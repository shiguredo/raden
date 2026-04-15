#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PixelFormat {
    /// 32-bit premultiplied ARGB。u32 で 0xAARRGGBB。
    Prgb32 = 1,
    /// 32-bit XRGB。アルファは未使用 (通常 0xFF)。ストライドとメモリ幅は `Prgb32` と同じ。
    Xrgb32 = 2,
    /// アルファのみ 8bit/ピクセル。合成は `pipeline::a8` のスカラ実装。
    A8 = 3,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Prgb32 | Self::Xrgb32 => 4,
            Self::A8 => 1,
        }
    }
}
