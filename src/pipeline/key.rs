use crate::api::style::CompOp;
use crate::pixel::PixelFormat;

/// パイプラインで使用する fill タイプ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FillType {
    /// 矩形塗りつぶし。
    BoxA = 0,
    /// マスク (カバレッジ) 付き塗りつぶし。
    Mask = 1,
}

/// パイプラインで使用する fetch タイプ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FetchType {
    /// 単色。
    Solid = 0,
}

/// パイプラインを一意に識別するキー。
///
/// `dst_format(4bit) | comp_op(6bit) | fill_type(2bit) | fetch_type(5bit)` を
/// u32 にパックする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineKey(u32);

impl PipelineKey {
    pub fn new(
        dst_format: PixelFormat,
        comp_op: CompOp,
        fill_type: FillType,
        fetch_type: FetchType,
    ) -> Self {
        let value = ((dst_format as u32) << 13)
            | ((comp_op as u32) << 7)
            | ((fill_type as u32) << 5)
            | (fetch_type as u32);
        Self(value)
    }

    pub fn value(self) -> u32 {
        self.0
    }
}
