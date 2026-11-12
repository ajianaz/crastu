//! YCbCr to RGB matrices for `surface()`.
//!
//! Crastu patch (issue #39): upstream hard-codes BT.601 full range in `surface_fragment`. This
//! module builds the matrix per surface from the `CVPixelBuffer`'s own colour tags. It has no
//! dependencies, so Crastu's `xtask-colour` includes this very file to test it without a GPU.

/// The colour matrix of the video, from the `kCVImageBufferYCbCrMatrixKey` attachment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    Bt601,
    Bt709,
    Bt2020,
}

impl Matrix {
    /// Maps the value of `kCVImageBufferYCbCrMatrixKey`. `None` for anything else, including
    /// no tag at all; the caller then keeps upstream's behaviour (BT.601).
    pub fn from_cv_name(name: &str) -> Option<Self> {
        match name {
            "ITU_R_601_4" => Some(Self::Bt601),
            "ITU_R_709_2" => Some(Self::Bt709),
            "ITU_R_2020" => Some(Self::Bt2020),
            _ => None,
        }
    }

    /// The luma weights (Kr, Kb) of the standard.
    fn weights(self) -> (f64, f64) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }
}

/// A 4x4 matrix in Metal's layout: four columns, applied to `[y, cb, cr, 1]` with each sample
/// normalised to 0..1 as the texture sampler returns it.
pub type Transform = [[f32; 4]; 4];

/// The matrix for 8-bit samples: full range (Y, Cb and Cr 0..255, `420f`) or video range
/// (Y 16..235, Cb and Cr 16..240, `420v`).
pub fn transform(matrix: Matrix, full_range: bool) -> Transform {
    let (kr, kb) = matrix.weights();
    let kg = 1.0 - kr - kb;
    let rv = 2.0 * (1.0 - kr);
    let bu = 2.0 * (1.0 - kb);
    let gu = -(kb / kg) * bu;
    let gv = -(kr / kg) * rv;
    // Scale and offset that bring the samples to Y 0..1 and Cb, Cr -0.5..0.5.
    let (y_scale, c_scale, y_offset) = if full_range {
        (1.0, 1.0, 0.0)
    } else {
        (255.0 / 219.0, 255.0 / 224.0, 16.0 / 255.0)
    };
    let c_offset = 128.0 / 255.0;
    let column = |r: f64, g: f64, b: f64| [r as f32, g as f32, b as f32, 0.0];
    let shift = |gain: f64| -y_scale * y_offset - gain * c_scale * c_offset;
    [
        column(y_scale, y_scale, y_scale),
        column(0.0, gu * c_scale, bu * c_scale),
        column(rv * c_scale, gv * c_scale, 0.0),
        [
            shift(rv) as f32,
            shift(gu + gv) as f32,
            shift(bu) as f32,
            1.0,
        ],
    ]
}

/// What `surface()` draws for a buffer that has no usable tag: the upstream matrix.
pub const DEFAULT_MATRIX: Matrix = Matrix::Bt601;
