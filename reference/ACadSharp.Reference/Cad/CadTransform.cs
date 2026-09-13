using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// An affine map from an entity's own coordinates into world coordinates.
/// </summary>
/// <remarks>
/// <para>
/// Written here rather than taken from <c>CSMath</c> so the composition order
/// and the row or column convention are decisions this repository made and can
/// read back, instead of something inherited from a library whose matrix layout
/// could change in a patch release and silently mirror every block in the
/// corpus.
/// </para>
/// <para>
/// The <see cref="IsSimilarity"/> flag is tracked structurally from the scale
/// factors that went into the chain, never sniffed back out of the matrix.
/// Comparing computed matrix entries for equality would classify an ordinary
/// 30 degree rotation as non-uniform on float noise alone, and every rotated
/// block in the corpus would turn into an ellipse.
/// </para>
/// </remarks>
/// <param name="M00">Row 0, column 0.</param>
/// <param name="M01">Row 0, column 1.</param>
/// <param name="M02">Row 0, column 2.</param>
/// <param name="M03">Row 0 translation.</param>
/// <param name="M10">Row 1, column 0.</param>
/// <param name="M11">Row 1, column 1.</param>
/// <param name="M12">Row 1, column 2.</param>
/// <param name="M13">Row 1 translation.</param>
/// <param name="M20">Row 2, column 0.</param>
/// <param name="M21">Row 2, column 1.</param>
/// <param name="M22">Row 2, column 2.</param>
/// <param name="M23">Row 2 translation.</param>
/// <param name="IsSimilarity">
/// Whether every step that built this chain scaled X, Y and Z by the same
/// magnitude. When true, circles stay circles and arc angles only shift.
/// </param>
public readonly record struct CadTransform(
    double M00, double M01, double M02, double M03,
    double M10, double M11, double M12, double M13,
    double M20, double M21, double M22, double M23,
    bool IsSimilarity)
{
    /// <summary>The identity map.</summary>
    public static CadTransform Identity { get; } = new(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        true);

    /// <summary>Maps a point.</summary>
    /// <param name="p">The point in source coordinates.</param>
    /// <returns>The point in target coordinates.</returns>
    public Point3 Apply(Point3 p) => new(
        (M00 * p.X) + (M01 * p.Y) + (M02 * p.Z) + M03,
        (M10 * p.X) + (M11 * p.Y) + (M12 * p.Z) + M13,
        (M20 * p.X) + (M21 * p.Y) + (M22 * p.Z) + M23);

    /// <summary>Maps a direction, ignoring the translation.</summary>
    /// <param name="v">The vector in source coordinates.</param>
    /// <returns>The vector in target coordinates.</returns>
    public Point3 ApplyVector(Point3 v) => new(
        (M00 * v.X) + (M01 * v.Y) + (M02 * v.Z),
        (M10 * v.X) + (M11 * v.Y) + (M12 * v.Z),
        (M20 * v.X) + (M21 * v.Y) + (M22 * v.Z));

    /// <summary>
    /// Composes two maps, outer after inner.
    /// </summary>
    /// <param name="outer">The map applied second.</param>
    /// <param name="inner">The map applied first.</param>
    /// <returns>The single map equivalent to applying inner then outer.</returns>
    public static CadTransform Compose(CadTransform outer, CadTransform inner)
    {
        double Row(double a0, double a1, double a2, double b0, double b1, double b2)
            => (a0 * b0) + (a1 * b1) + (a2 * b2);

        return new CadTransform(
            Row(outer.M00, outer.M01, outer.M02, inner.M00, inner.M10, inner.M20),
            Row(outer.M00, outer.M01, outer.M02, inner.M01, inner.M11, inner.M21),
            Row(outer.M00, outer.M01, outer.M02, inner.M02, inner.M12, inner.M22),
            Row(outer.M00, outer.M01, outer.M02, inner.M03, inner.M13, inner.M23) + outer.M03,

            Row(outer.M10, outer.M11, outer.M12, inner.M00, inner.M10, inner.M20),
            Row(outer.M10, outer.M11, outer.M12, inner.M01, inner.M11, inner.M21),
            Row(outer.M10, outer.M11, outer.M12, inner.M02, inner.M12, inner.M22),
            Row(outer.M10, outer.M11, outer.M12, inner.M03, inner.M13, inner.M23) + outer.M13,

            Row(outer.M20, outer.M21, outer.M22, inner.M00, inner.M10, inner.M20),
            Row(outer.M20, outer.M21, outer.M22, inner.M01, inner.M11, inner.M21),
            Row(outer.M20, outer.M21, outer.M22, inner.M02, inner.M12, inner.M22),
            Row(outer.M20, outer.M21, outer.M22, inner.M03, inner.M13, inner.M23) + outer.M23,

            outer.IsSimilarity && inner.IsSimilarity);
    }

    /// <summary>Translation only.</summary>
    /// <param name="t">The offset.</param>
    /// <returns>The map.</returns>
    public static CadTransform Translation(Point3 t) => new(
        1.0, 0.0, 0.0, t.X,
        0.0, 1.0, 0.0, t.Y,
        0.0, 0.0, 1.0, t.Z,
        true);

    /// <summary>Axis-aligned scaling about the origin.</summary>
    /// <param name="sx">X factor.</param>
    /// <param name="sy">Y factor.</param>
    /// <param name="sz">Z factor.</param>
    /// <returns>The map, flagged a similarity only when the three magnitudes are exactly equal.</returns>
    public static CadTransform Scale(double sx, double sy, double sz) => new(
        sx, 0.0, 0.0, 0.0,
        0.0, sy, 0.0, 0.0,
        0.0, 0.0, sz, 0.0,
        Math.Abs(sx) == Math.Abs(sy) && Math.Abs(sy) == Math.Abs(sz));

    /// <summary>Rotation about the Z axis.</summary>
    /// <param name="radians">Counter-clockwise angle.</param>
    /// <returns>The map.</returns>
    public static CadTransform RotationZ(double radians)
    {
        double c = Math.Cos(radians);
        double s = Math.Sin(radians);
        return new CadTransform(
            c, -s, 0.0, 0.0,
            s, c, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            true);
    }

    /// <summary>
    /// The object coordinate system a DXF normal implies, as a map from OCS into world space.
    /// </summary>
    /// <param name="normal">The entity's normal, which need not be normalised.</param>
    /// <returns>The rotation whose third column is the normal.</returns>
    /// <remarks>
    /// This is the DXF arbitrary axis algorithm, reimplemented rather than
    /// borrowed: it is the one convention both sides of the differential have to
    /// agree on down to the choice of the 1/64 threshold, so it is written out
    /// where a reviewer can compare it with the specification.
    /// </remarks>
    public static CadTransform ArbitraryAxis(Point3 normal)
    {
        Point3 n = Geometry.Normalize(normal);
        if (n == Point3.Zero)
        {
            n = Point3.UnitZ;
        }

        Point3 ax = Math.Abs(n.X) < 1.0 / 64.0 && Math.Abs(n.Y) < 1.0 / 64.0
            ? Geometry.Cross(new Point3(0.0, 1.0, 0.0), n)
            : Geometry.Cross(new Point3(0.0, 0.0, 1.0), n);
        ax = Geometry.Normalize(ax);
        Point3 ay = Geometry.Normalize(Geometry.Cross(n, ax));

        return new CadTransform(
            ax.X, ay.X, n.X, 0.0,
            ax.Y, ay.Y, n.Y, 0.0,
            ax.Z, ay.Z, n.Z, 0.0,
            true);
    }
}

/// <summary>Small vector helpers, kept out of the transform type itself.</summary>
public static class Geometry
{
    /// <summary>Vector difference.</summary>
    /// <param name="a">Left operand.</param>
    /// <param name="b">Right operand.</param>
    /// <returns><paramref name="a"/> minus <paramref name="b"/>.</returns>
    public static Point3 Subtract(Point3 a, Point3 b) => new(a.X - b.X, a.Y - b.Y, a.Z - b.Z);

    /// <summary>Vector sum.</summary>
    /// <param name="a">Left operand.</param>
    /// <param name="b">Right operand.</param>
    /// <returns><paramref name="a"/> plus <paramref name="b"/>.</returns>
    public static Point3 Add(Point3 a, Point3 b) => new(a.X + b.X, a.Y + b.Y, a.Z + b.Z);

    /// <summary>Scalar multiple.</summary>
    /// <param name="a">The vector.</param>
    /// <param name="k">The factor.</param>
    /// <returns>The scaled vector.</returns>
    public static Point3 Scale(Point3 a, double k) => new(a.X * k, a.Y * k, a.Z * k);

    /// <summary>Dot product.</summary>
    /// <param name="a">Left operand.</param>
    /// <param name="b">Right operand.</param>
    /// <returns>The scalar product.</returns>
    public static double Dot(Point3 a, Point3 b) => (a.X * b.X) + (a.Y * b.Y) + (a.Z * b.Z);

    /// <summary>Cross product.</summary>
    /// <param name="a">Left operand.</param>
    /// <param name="b">Right operand.</param>
    /// <returns>The vector product.</returns>
    public static Point3 Cross(Point3 a, Point3 b) => new(
        (a.Y * b.Z) - (a.Z * b.Y),
        (a.Z * b.X) - (a.X * b.Z),
        (a.X * b.Y) - (a.Y * b.X));

    /// <summary>Euclidean length.</summary>
    /// <param name="a">The vector.</param>
    /// <returns>Its length.</returns>
    public static double Length(Point3 a) => Math.Sqrt(Dot(a, a));

    /// <summary>Unit vector, or the zero vector when the input has no length.</summary>
    /// <param name="a">The vector.</param>
    /// <returns>The normalised vector.</returns>
    public static Point3 Normalize(Point3 a)
    {
        double len = Length(a);
        return len == 0.0 ? Point3.Zero : Scale(a, 1.0 / len);
    }
}
