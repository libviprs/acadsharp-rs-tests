using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// Carries an analytic curve through a transform without flattening it.
/// </summary>
/// <remarks>
/// <para>
/// A circle, arc or ellipse is described in its own plane by a centre and two
/// conjugate semi-diameters. Every affine map takes conjugate semi-diameters to
/// conjugate semi-diameters, so transforming those two vectors and rebuilding
/// the curve from them is exact, and the alternative, tessellating and
/// re-fitting, is exactly what section 5 forbids.
/// </para>
/// <para>
/// When the chain is a similarity the result is the same kind of curve with the
/// same shape parameters and an angle offset, computed on the fast path so an
/// untransformed circle in the corpus keeps the radius the file actually
/// stores. Otherwise a two by two singular value decomposition recovers the
/// principal axes of the image ellipse, which is the general and still exact
/// answer.
/// </para>
/// </remarks>
public static class CurveMapping
{
    /// <summary>Two pi, the full turn every angle here is reduced against.</summary>
    public const double TwoPi = 2.0 * Math.PI;

    /// <summary>An analytic curve after mapping, expressed in the OCS of its own normal.</summary>
    /// <param name="IsCircular">Whether the image is still a circle rather than a true ellipse.</param>
    /// <param name="Center">Centre in the OCS the normal implies.</param>
    /// <param name="Radius">Radius, meaningful only when <paramref name="IsCircular"/>.</param>
    /// <param name="MajorAxis">Major semi-axis vector in OCS, meaningful only for an ellipse.</param>
    /// <param name="Ratio">Minor over major, meaningful only for an ellipse.</param>
    /// <param name="AngleOffset">
    /// What to add to the source start and end angles. On the similarity path
    /// this is the planar rotation; on the general path it is the negated right
    /// rotation of the decomposition.
    /// </param>
    /// <param name="Normal">World-space unit normal of the curve's plane.</param>
    /// <param name="Degenerate">
    /// True when the two semi-diameters became parallel or zero, which means the
    /// curve collapsed to a segment or a point and cannot be encoded as a curve.
    /// </param>
    public readonly record struct MappedCurve(
        bool IsCircular,
        Point3 Center,
        double Radius,
        Point3 MajorAxis,
        double Ratio,
        double AngleOffset,
        Point3 Normal,
        bool Degenerate);

    /// <summary>
    /// Maps a curve given its centre and two conjugate semi-diameters in the
    /// coordinate system the transform accepts.
    /// </summary>
    /// <param name="center">Centre, in the source coordinate system.</param>
    /// <param name="u">First conjugate semi-diameter. For a circle, the radius along local X.</param>
    /// <param name="v">Second conjugate semi-diameter. For a circle, the radius along local Y.</param>
    /// <param name="toWorld">The map into world coordinates.</param>
    /// <returns>The mapped curve.</returns>
    public static MappedCurve Map(Point3 center, Point3 u, Point3 v, CadTransform toWorld)
    {
        Point3 worldCenter = toWorld.Apply(center);
        Point3 worldU = toWorld.ApplyVector(u);
        Point3 worldV = toWorld.ApplyVector(v);

        Point3 normal = Geometry.Normalize(Geometry.Cross(worldU, worldV));
        if (normal == Point3.Zero)
        {
            return new MappedCurve(false, worldCenter, 0.0, Point3.Zero, 1.0, 0.0, Point3.UnitZ, true);
        }

        CadTransform ocs = CadTransform.ArbitraryAxis(normal);
        Point3 ax = new(ocs.M00, ocs.M10, ocs.M20);
        Point3 ay = new(ocs.M01, ocs.M11, ocs.M21);

        Point3 c = new(
            Geometry.Dot(worldCenter, ax),
            Geometry.Dot(worldCenter, ay),
            Geometry.Dot(worldCenter, normal));

        double ux = Geometry.Dot(worldU, ax);
        double uy = Geometry.Dot(worldU, ay);
        double vx = Geometry.Dot(worldV, ax);
        double vy = Geometry.Dot(worldV, ay);

        if (toWorld.IsSimilarity)
        {
            // u and v stay perpendicular and keep their length ratio, so the
            // only thing that happened to the curve is a rotation in its plane.
            double radius = Math.Sqrt((ux * ux) + (uy * uy));
            double minor = Math.Sqrt((vx * vx) + (vy * vy));
            double phi = Math.Atan2(uy, ux);
            bool circular = radius == minor;
            return new MappedCurve(
                circular,
                c,
                radius,
                new Point3(ux, uy, 0.0),
                radius == 0.0 ? 1.0 : minor / radius,
                phi,
                normal,
                radius == 0.0);
        }

        // [u v] = R(phi) * diag(s1, s2) * R(theta)^T, the closed-form two by
        // two singular value decomposition. R(phi)'s columns are the principal
        // directions, the singular values are the semi-axis lengths, and
        // subtracting theta from the source parameter re-parametrises the curve
        // onto them.
        double e = (ux + vy) / 2.0;
        double f = (ux - vy) / 2.0;
        double g = (uy + vx) / 2.0;
        double h = (uy - vx) / 2.0;
        double q = Math.Sqrt((e * e) + (h * h));
        double r = Math.Sqrt((f * f) + (g * g));
        double sigma1 = q + r;
        double sigma2 = q - r;
        double a1 = Math.Atan2(h, e);
        double a2 = Math.Atan2(g, f);
        double phiOut = (a1 + a2) / 2.0;
        double theta = (a1 - a2) / 2.0;

        if (sigma1 == 0.0)
        {
            return new MappedCurve(false, c, 0.0, Point3.Zero, 1.0, 0.0, normal, true);
        }

        var major = new Point3(sigma1 * Math.Cos(phiOut), sigma1 * Math.Sin(phiOut), 0.0);
        double ratio = Math.Abs(sigma2) / sigma1;
        if (ratio == 0.0)
        {
            return new MappedCurve(false, c, 0.0, major, 0.0, 0.0, normal, true);
        }

        return new MappedCurve(false, c, sigma1, major, ratio, -theta, normal, false);
    }

    /// <summary>Reduces an angle into <c>[0, 2*pi)</c>.</summary>
    /// <param name="angle">Any angle in radians.</param>
    /// <returns>The equivalent angle in the canonical range.</returns>
    /// <remarks>
    /// An angle already in range and offset by exactly zero comes back
    /// bit-identical, which is what keeps an untransformed arc in the corpus
    /// carrying the digits the file stores.
    /// </remarks>
    public static double Normalize(double angle)
    {
        if (angle >= 0.0 && angle < TwoPi)
        {
            return angle;
        }

        double reduced = angle - (TwoPi * Math.Floor(angle / TwoPi));

        // Floor can land exactly on the upper bound for a tiny negative input.
        return reduced >= TwoPi ? 0.0 : reduced;
    }

    /// <summary>
    /// Puts a start and end angle into the canonical "sweeps counter-clockwise" form.
    /// </summary>
    /// <param name="start">Source start angle.</param>
    /// <param name="end">Source end angle.</param>
    /// <param name="offset">Planar rotation to add to both.</param>
    /// <returns>A start in <c>[0, 2*pi)</c> and an end strictly greater than it.</returns>
    public static (double Start, double End) Sweep(double start, double end, double offset)
    {
        double s = Normalize(start + offset);
        double e = Normalize(end + offset);
        if (e <= s)
        {
            e += TwoPi;
        }

        return (s, e);
    }
}
