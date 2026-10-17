//! The arbitrary axis algorithm and the angle helpers both emitters share.
//!
//! A DXF normal implies an object coordinate system (OCS), and schema 1 puts
//! every record that carries a normal in it (`docs/CANONICAL_SCHEMA.md`,
//! Coordinates). acadrust hands over OCS coordinates already; the wire acadsharp-rs
//! decodes is world space with the normal beside it, so that adapter has to
//! project. Both go through the one rule here, which is the one libviprs-dep's
//! `docs/WIRE.md` spells out and the oracle runs.

use std::f64::consts::TAU;

/// A 3D vector.
pub type V3 = [f64; 3];

pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 {
        [0.0; 3]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// The OCS axes a normal implies: `(x axis, y axis, unit normal)`.
///
/// The normal is normalised first, even though a producer promises a unit one:
/// `1/64` bounds a direction cosine, so it only means anything on a unit vector,
/// and a normal of `(1/64 + 1e-9, 0, 1)` is outside the band as written and
/// inside it once normalised, ninety degrees apart. A zero normal is `+Z`.
/// `1/64` is a real number, not an integer division.
pub fn axes(normal: V3) -> (V3, V3, V3) {
    let mut n = normalize(normal);
    if n == [0.0; 3] {
        n = [0.0, 0.0, 1.0];
    }
    let pick = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let ax = normalize(cross(pick, n));
    let ay = normalize(cross(n, ax));
    (ax, ay, n)
}

/// A world point in the OCS of `normal`.
pub fn point_to_ocs(normal: V3, p: V3) -> V3 {
    let (ax, ay, n) = axes(normal);
    [dot(p, ax), dot(p, ay), dot(p, n)]
}

/// An angle reduced into `[0, 2*pi)`. One already in range comes back
/// bit-identical, which keeps an untransformed arc carrying the digits the file
/// stores.
pub fn normalize_angle(angle: f64) -> f64 {
    if (0.0..TAU).contains(&angle) {
        return angle;
    }
    let reduced = angle - TAU * (angle / TAU).floor();
    if reduced >= TAU { 0.0 } else { reduced }
}

/// A start and end into the canonical "sweeps counter-clockwise" form: a start
/// in `[0, 2*pi)` and an end strictly greater than it.
pub fn sweep(start: f64, end: f64, offset: f64) -> (f64, f64) {
    let s = normalize_angle(start + offset);
    let mut e = normalize_angle(end + offset);
    if e <= s {
        e += TAU;
    }
    (s, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plus_z_normal_is_world_space() {
        let (ax, ay, n) = axes([0.0, 0.0, 1.0]);
        assert_eq!(
            (ax, ay, n),
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0])
        );
        assert_eq!(
            point_to_ocs([0.0, 0.0, 1.0], [3.0, 4.0, 5.0]),
            [3.0, 4.0, 5.0]
        );
    }

    #[test]
    fn angle_zero_on_a_plus_y_normal_points_along_minus_x() {
        // WIRE.md: "For a normal of (0, 1, 0) it is (-1, 0, 0), which nobody guesses."
        let (ax, _, _) = axes([0.0, 1.0, 0.0]);
        assert_eq!(ax, [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_band_is_one_sixty_fourth_of_a_unit_vector_not_of_the_raw_input() {
        // Outside the band as written (x is 1/64 + 1e-9 of a length-1 z), inside
        // it once normalised. An implementation that tests the raw input picks
        // the z axis and gets an x axis ninety degrees from the right one.
        let e = 1.0 / 64.0 + 1e-9;
        let (raw_x, _, _) = axes([e, 0.0, 1.0]);
        assert!(raw_x[0] > 0.99, "normalise first: {raw_x:?}");
        // And well outside the band it is the other branch.
        let (outside, _, _) = axes([0.1, 0.0, 1.0]);
        assert!(outside[1] > 0.99, "{outside:?}");
    }

    #[test]
    fn a_zero_normal_is_plus_z() {
        assert_eq!(axes([0.0; 3]).2, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn an_angle_in_range_is_returned_bit_identical() {
        let a = 1.234_567_890_123_456_7;
        assert_eq!(normalize_angle(a).to_bits(), a.to_bits());
        assert_eq!(normalize_angle(-0.5), TAU - 0.5);
        assert_eq!(sweep(0.0, 0.0, 0.0), (0.0, TAU));
    }
}
