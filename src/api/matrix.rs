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
        self.m20 += tx;
        self.m21 += ty;
    }

    /// スケーリングを後乗算で適用する。
    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.m00 *= sx;
        self.m01 *= sy;
        self.m10 *= sx;
        self.m11 *= sy;
        self.m20 *= sx;
        self.m21 *= sy;
    }

    /// 回転を後乗算で適用する。角度はラジアン。
    pub fn rotate(&mut self, angle: f64) {
        let (sin, cos) = angle.sin_cos();
        let m00 = self.m00 * cos - self.m01 * sin;
        let m01 = self.m00 * sin + self.m01 * cos;
        let m10 = self.m10 * cos - self.m11 * sin;
        let m11 = self.m10 * sin + self.m11 * cos;
        let m20 = self.m20 * cos - self.m21 * sin;
        let m21 = self.m20 * sin + self.m21 * cos;
        self.m00 = m00;
        self.m01 = m01;
        self.m10 = m10;
        self.m11 = m11;
        self.m20 = m20;
        self.m21 = m21;
    }

    /// 指定した中心点まわりの回転を後乗算で適用する。角度はラジアン。
    /// Blend2D の `rotate(angle, cx, cy)` に相当する。
    /// `self = self * T(-cx,-cy) * R(angle) * T(cx,cy)` と等価。
    pub fn rotate_around(&mut self, angle: f64, cx: f64, cy: f64) {
        let (sin, cos) = angle.sin_cos();
        let tx = cx * (1.0 - cos) + cy * sin;
        let ty = cy * (1.0 - cos) - cx * sin;
        let m00 = self.m00 * cos - self.m01 * sin;
        let m01 = self.m00 * sin + self.m01 * cos;
        let m10 = self.m10 * cos - self.m11 * sin;
        let m11 = self.m10 * sin + self.m11 * cos;
        let m20 = self.m20 * cos - self.m21 * sin + tx;
        let m21 = self.m20 * sin + self.m21 * cos + ty;
        self.m00 = m00;
        self.m01 = m01;
        self.m10 = m10;
        self.m11 = m11;
        self.m20 = m20;
        self.m21 = m21;
    }

    /// 平行移動を前乗算で適用する。
    /// Blend2D の `postTranslate(tx, ty)` に相当する。
    /// `self = T(tx,ty) * self` と等価。
    pub fn post_translate(&mut self, tx: f64, ty: f64) {
        self.m20 += tx * self.m00 + ty * self.m10;
        self.m21 += tx * self.m01 + ty * self.m11;
    }

    /// スケーリングを前乗算で適用する。
    /// Blend2D の `postScale(sx, sy)` に相当する。
    /// `self = S(sx,sy) * self` と等価。
    pub fn post_scale(&mut self, sx: f64, sy: f64) {
        self.m00 *= sx;
        self.m01 *= sx;
        self.m10 *= sy;
        self.m11 *= sy;
    }

    /// 回転を前乗算で適用する。角度はラジアン。
    /// Blend2D の `postRotate(angle)` に相当する。
    /// `self = R(angle) * self` と等価。
    pub fn post_rotate(&mut self, angle: f64) {
        *self = Self::rotation(angle).multiply(self);
    }

    /// 任意の行列を前乗算で適用する。
    /// Blend2D の `postTransform(m)` に相当する。
    /// `self = m * self` と等価。
    pub fn post_transform(&mut self, m: &Self) {
        *self = m.multiply(self);
    }

    /// 単位行列にリセットする。
    pub fn reset(&mut self) {
        *self = Self::IDENTITY;
    }

    /// 逆行列を計算する。行列式がゼロの場合は None を返す。
    pub fn invert(&self) -> Option<Self> {
        let det = self.m00 * self.m11 - self.m01 * self.m10;
        if det.abs() < 1e-10 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            m00: self.m11 * inv_det,
            m01: -self.m01 * inv_det,
            m10: -self.m10 * inv_det,
            m11: self.m00 * inv_det,
            m20: (self.m10 * self.m21 - self.m11 * self.m20) * inv_det,
            m21: (self.m01 * self.m20 - self.m00 * self.m21) * inv_det,
        })
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
