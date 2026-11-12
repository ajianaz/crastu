//! Colour difference: sRGB to CIELAB (D65), CIE76 and CIEDE2000.

/// CIELAB colour: lightness, green-red, blue-yellow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

fn srgb_to_linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// An 8-bit sRGB colour as CIELAB under the D65 white point.
pub fn lab_from_srgb8(rgb: [u8; 3]) -> Lab {
    let [r, g, b] = rgb.map(|v| srgb_to_linear(f64::from(v) / 255.0));
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
    let f = |t: f64| {
        const D: f64 = 6.0 / 29.0;
        if t > D * D * D {
            t.cbrt()
        } else {
            t / (3.0 * D * D) + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(x / 0.95047), f(y), f(z / 1.08883));
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIE76: the Euclidean distance in CIELAB.
pub fn delta_e76(x: Lab, y: Lab) -> f64 {
    ((x.l - y.l).powi(2) + (x.a - y.a).powi(2) + (x.b - y.b).powi(2)).sqrt()
}

/// CIEDE2000 (Sharma, Wu and Dalal, 2005), with kL = kC = kH = 1.
pub fn delta_e2000(x: Lab, y: Lab) -> f64 {
    let c1 = x.a.hypot(x.b);
    let c2 = y.a.hypot(y.b);
    let c_bar7 = ((c1 + c2) / 2.0).powi(7);
    let g = 0.5 * (1.0 - (c_bar7 / (c_bar7 + 25f64.powi(7))).sqrt());
    let (a1, a2) = ((1.0 + g) * x.a, (1.0 + g) * y.a);
    let (c1, c2) = (a1.hypot(x.b), a2.hypot(y.b));
    let hue = |b: f64, a: f64| {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            b.atan2(a).to_degrees().rem_euclid(360.0)
        }
    };
    let (h1, h2) = (hue(x.b, a1), hue(y.b, a2));

    let dl = y.l - x.l;
    let dc = c2 - c1;
    let dh_deg = if c1 * c2 == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 - h1 > 180.0 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let dh = 2.0 * (c1 * c2).sqrt() * (dh_deg.to_radians() / 2.0).sin();

    let l_bar = (x.l + y.l) / 2.0;
    let c_bar = (c1 + c2) / 2.0;
    let h_bar = if c1 * c2 == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (h_bar - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar).to_radians().cos()
        + 0.32 * (3.0 * h_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar - 63.0).to_radians().cos();
    let d_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let c_bar7 = c_bar.powi(7);
    let rc = 2.0 * (c_bar7 / (c_bar7 + 25f64.powi(7))).sqrt();
    let l50 = (l_bar - 50.0).powi(2);
    let sl = 1.0 + 0.015 * l50 / (20.0 + l50).sqrt();
    let sc = 1.0 + 0.045 * c_bar;
    let sh = 1.0 + 0.015 * c_bar * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;
    let (el, ec, eh) = (dl / sl, dc / sc, dh / sh);
    (el * el + ec * ec + eh * eh + rt * ec * eh).sqrt()
}
