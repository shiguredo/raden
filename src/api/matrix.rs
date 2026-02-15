/// 2D アフィン変換行列。Blend2D の `BLMatrix2D` と同一レイアウト。
///
/// ```text
/// | m00  m01 |    x' = m00*x + m10*y + m20
/// | m10  m11 |    y' = m01*x + m11*y + m21
/// | m20  m21 |
/// ```
///
/// - `m00`, `m11`: スケール
/// - `m01`, `m10`: せん断
/// - `m20`, `m21`: 平行移動
/// - 合成: 後乗算 (`matrix = matrix * T`) — Blend2D / SVG / Canvas 互換
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix2D {
    pub m00: f64,
    pub m01: f64,
    pub m10: f64,
    pub m11: f64,
    pub m20: f64,
    pub m21: f64,
}

impl Matrix2D {
    /// 単位行列。
    pub const IDENTITY: Self = Self {
        m00: 1.0,
        m01: 0.0,
        m10: 0.0,
        m11: 1.0,
        m20: 0.0,
        m21: 0.0,
    };

    /// 全要素を指定して行列を生成する。
    pub fn new(m00: f64, m01: f64, m10: f64, m11: f64, m20: f64, m21: f64) -> Self {
        Self {
            m00,
            m01,
            m10,
            m11,
            m20,
            m21,
        }
    }

    /// 平行移動行列を生成する。
    pub fn translation(tx: f64, ty: f64) -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            m20: tx,
            m21: ty,
        }
    }

    /// スケーリング行列を生成する。
    pub fn scaling(sx: f64, sy: f64) -> Self {
        Self {
            m00: sx,
            m01: 0.0,
            m10: 0.0,
            m11: sy,
            m20: 0.0,
            m21: 0.0,
        }
    }

    /// 回転行列を生成する。角度はラジアン。
    pub fn rotation(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            m00: cos,
            m01: sin,
            m10: -sin,
            m11: cos,
            m20: 0.0,
            m21: 0.0,
        }
    }

    /// 単位行列かどうか判定する。
    pub fn is_identity(&self) -> bool {
        self.m00 == 1.0
            && self.m01 == 0.0
            && self.m10 == 0.0
            && self.m11 == 1.0
            && self.m20 == 0.0
            && self.m21 == 0.0
    }

    /// 後乗算で行列を合成する: `self * other`。
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            m00: self.m00 * other.m00 + self.m01 * other.m10,
            m01: self.m00 * other.m01 + self.m01 * other.m11,
            m10: self.m10 * other.m00 + self.m11 * other.m10,
            m11: self.m10 * other.m01 + self.m11 * other.m11,
            m20: self.m20 * other.m00 + self.m21 * other.m10 + other.m20,
            m21: self.m20 * other.m01 + self.m21 * other.m11 + other.m21,
        }
    }

    /// 点 (x, y) を変換する。
    pub fn map_point(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.m00 * x + self.m10 * y + self.m20,
            self.m01 * x + self.m11 * y + self.m21,
        )
    }

    /// 平行移動を後乗算で適用する。
    pub fn translate(&mut self, tx: f64, ty: f64) {
        *self = self.multiply(&Self::translation(tx, ty));
    }

    /// スケーリングを後乗算で適用する。
    pub fn scale(&mut self, sx: f64, sy: f64) {
        *self = self.multiply(&Self::scaling(sx, sy));
    }

    /// 回転を後乗算で適用する。角度はラジアン。
    pub fn rotate(&mut self, angle: f64) {
        *self = self.multiply(&Self::rotation(angle));
    }

    /// 単位行列にリセットする。
    pub fn reset(&mut self) {
        *self = Self::IDENTITY;
    }
}

/// リファレンス実装: エッジ座標を行列で変換する。PBT で JIT 版との比較に使用。
pub fn transform_edges_reference(edges: &mut [(f64, f64, f64, f64)], m: &Matrix2D) {
    for edge in edges.iter_mut() {
        let (x0p, y0p) = m.map_point(edge.0, edge.1);
        let (x1p, y1p) = m.map_point(edge.2, edge.3);
        *edge = (x0p, y0p, x1p, y1p);
    }
}
