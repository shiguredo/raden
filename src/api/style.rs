use crate::pixel::premultiply_rgba;

/// RGBA カラー値。内部表現は 0xAARRGGBB。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Rgba32(pub u32);

impl Rgba32 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    #[inline]
    pub const fn r(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    #[inline]
    pub const fn g(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    #[inline]
    pub const fn b(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    #[inline]
    pub const fn a(self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }

    #[inline]
    pub const fn is_opaque(self) -> bool {
        self.0 >= 0xFF00_0000
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.0 <= 0x00FF_FFFF
    }

    /// premultiplied ARGB32 (0xAARRGGBB) に変換する。
    #[inline]
    pub const fn to_prgb32(self) -> u32 {
        premultiply_rgba(self.r(), self.g(), self.b(), self.a())
    }
}

/// ストロークの端点形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum StrokeCap {
    /// 端点で切り落とし (デフォルト)。
    #[default]
    Butt = 0,
    /// 端点を width/2 だけ延長した矩形。
    Square = 1,
    /// 端点に半円を付加する。
    Round = 2,
}

/// ストロークの接続形状。Blend2D の BLStrokeJoin と値を合わせている。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum StrokeJoin {
    /// マイター接合 (miter limit 超過時に切り落とし、Blend2D デフォルト)。
    #[default]
    MiterClip = 0,
    /// マイター接合 (miter limit 超過時に Bevel にフォールバック)。
    MiterBevel = 1,
    /// マイター接合 (miter limit 超過時に Round にフォールバック)。
    MiterRound = 2,
    /// 斜め切り落とし。
    Bevel = 3,
    /// 円弧接合。
    Round = 4,
}

/// 塗りつぶし規則。パスの内外判定アルゴリズムを決定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum FillRule {
    /// ワインディングナンバーが非ゼロなら内側 (デフォルト)。
    #[default]
    NonZero = 0,
    /// ワインディングナンバーが奇数なら内側。
    EvenOdd = 1,
}

/// 合成オペレーション。Blend2D の BLCompOp と値を合わせている。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CompOp {
    /// source を destination の上に Porter-Duff SrcOver で合成する。
    /// out = src + dst * (1 - srcA)
    #[default]
    SrcOver = 0,
    /// destination を source で完全に置き換える。
    /// out = src
    SrcCopy = 1,
    /// out = src * dstA
    SrcIn = 2,
    /// out = src * (1 - dstA)
    SrcOut = 3,
    /// out = src * dstA + dst * (1 - srcA)
    SrcAtop = 4,
    /// out = dst + src * (1 - dstA)
    DstOver = 5,
    /// out = dst (何もしない)
    DstCopy = 6,
    /// out = dst * srcA
    DstIn = 7,
    /// out = dst * (1 - srcA)
    DstOut = 8,
    /// out = dst * srcA + src * (1 - dstA)
    DstAtop = 9,
    /// out = src * (1 - dstA) + dst * (1 - srcA)
    Xor = 10,
    /// out = 0
    Clear = 11,
    /// out = min(src + dst, 1)
    Plus = 12,
    /// out = max(dst - src, 0), alpha = SrcOver
    Minus = 13,
    /// out_c = src_c * dst_c / 255 (チャネル乗算、alpha も同様)
    Modulate = 14,
    /// Multiply ブレンド。blend = src*dst, out = blend + src*(1-dstA) + dst*(1-srcA)
    Multiply = 15,
    /// Screen ブレンド。out = src + dst - src*dst
    Screen = 16,
    /// Overlay ブレンド。条件分岐 (2*Dc < Da)
    Overlay = 17,
    /// Darken ブレンド。blend = min(src*dstA, dst*srcA)
    Darken = 18,
    /// Lighten ブレンド。blend = max(src*dstA, dst*srcA)
    Lighten = 19,
    /// ColorDodge ブレンド。除算あり
    ColorDodge = 20,
    /// ColorBurn ブレンド。除算あり
    ColorBurn = 21,
    /// LinearBurn ブレンド。飽和減算
    LinearBurn = 22,
    /// LinearLight ブレンド。clamp + 条件
    LinearLight = 23,
    /// PinLight ブレンド。条件分岐 + min/max
    PinLight = 24,
    /// HardLight ブレンド。条件分岐 (2*Sc < Sa)
    HardLight = 25,
    /// SoftLight ブレンド。除算 + sqrt + 条件分岐
    SoftLight = 26,
    /// Difference ブレンド。abs 差分
    Difference = 27,
    /// Exclusion ブレンド。単純な算術
    Exclusion = 28,
}
