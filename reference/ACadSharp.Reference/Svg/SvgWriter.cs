using System.Globalization;
using System.Text;

using ACadSharp.Reference.Cad;
using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Svg;

/// <summary>
/// The secondary, visual oracle: a deterministic SVG of the same canonical records.
/// </summary>
/// <remarks>
/// <para>
/// Generated from the canonical records, never from ACadSharp's DXF or DWG
/// writer. Round-tripping through a writer would make the comparison a test of
/// that writer, and a bug shared by the reader and the writer would cancel out
/// and leave the file looking correct.
/// </para>
/// <para>
/// The JSONL stays authoritative. Splines and out-of-plane conics are sampled
/// into paths here, and that approximation must never reach the semantic
/// record: the whole reason section 5 keeps curves analytic is so a later
/// differential can prove the curve survived, and an oracle that had already
/// flattened it could not.
/// </para>
/// <para>
/// White background, black hairlines, black text, no source colour fidelity.
/// That is the intended libviprs viewer and it is also the only styling that
/// cannot drift: a reference that tried to reproduce AutoCAD colour indices
/// would need a colour table, and the colour table would become a second thing
/// to keep in step.
/// </para>
/// </remarks>
public static class SvgWriter
{
    /// <summary>Samples used for a spline path. Visualisation only.</summary>
    /// <remarks>
    /// Uniform in parameter space rather than adaptive, because an adaptive
    /// subdivision's output depends on a tolerance comparison and would change
    /// the file's hash whenever the tolerance was tuned.
    /// </remarks>
    public const int SplineSamples = 256;

    /// <summary>Samples used for a conic that is not parallel to the XY plane.</summary>
    public const int OutOfPlaneConicSamples = 128;

    /// <summary>Fraction of the larger drawing dimension added as a margin on each side.</summary>
    public const double DefaultMarginFraction = 0.02;

    /// <summary>Nominal pixel size of the root element. The view box does the real work.</summary>
    public const int CanvasSize = 1024;

    /// <summary>
    /// Renders a canonical record stream as SVG.
    /// </summary>
    /// <param name="records">The records, in canonical order.</param>
    /// <param name="marginFraction">Margin as a fraction of the larger drawing dimension.</param>
    /// <returns>UTF-8 bytes, LF line endings, no byte order mark, final newline.</returns>
    public static byte[] Encode(
        IEnumerable<CanonicalRecord> records, double marginFraction = DefaultMarginFraction)
    {
        ArgumentNullException.ThrowIfNull(records);

        List<CanonicalRecord> all = [.. records];
        Bounds bounds = ComputeBounds(all);

        var body = new StringBuilder();
        foreach (CanonicalRecord record in all)
        {
            Render(record, body);
        }

        (double x, double y, double width, double height) = bounds.ViewBox(marginFraction);

        var svg = new StringBuilder();
        svg.Append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        svg.Append(CultureInfo.InvariantCulture, $"<!-- acadsharp-rs-tests visual reference, svg schema {CanonicalSchema.SvgVersion} -->\n");
        svg.Append("<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\"");
        svg.Append(CultureInfo.InvariantCulture, $" width=\"{CanvasSize}\" height=\"{CanvasSize}\"");
        svg.Append(" preserveAspectRatio=\"xMidYMid meet\"");
        svg.Append(CultureInfo.InvariantCulture,
            $" viewBox=\"{SvgFormatting.Number(x)} {SvgFormatting.Number(y)} {SvgFormatting.Number(width)} {SvgFormatting.Number(height)}\">\n");
        svg.Append(CultureInfo.InvariantCulture,
            $"<rect x=\"{SvgFormatting.Number(x)}\" y=\"{SvgFormatting.Number(y)}\" width=\"{SvgFormatting.Number(width)}\" height=\"{SvgFormatting.Number(height)}\" fill=\"#FFFFFF\" stroke=\"none\"/>\n");
        svg.Append("<g fill=\"none\" stroke=\"#000000\" stroke-width=\"1\" vector-effect=\"non-scaling-stroke\" ");
        svg.Append("stroke-linecap=\"round\" stroke-linejoin=\"round\">\n");
        svg.Append(body);
        svg.Append("</g>\n");
        svg.Append("</svg>\n");

        return new UTF8Encoding(encoderShouldEmitUTF8Identifier: false).GetBytes(svg.ToString());
    }

    private static void Render(CanonicalRecord record, StringBuilder body)
    {
        switch (record)
        {
            case LineRecord l:
                body.Append(CultureInfo.InvariantCulture,
                    $"<line x1=\"{N(l.Start.X)}\" y1=\"{N(SvgFormatting.FlipY(l.Start.Y))}\" x2=\"{N(l.End.X)}\" y2=\"{N(SvgFormatting.FlipY(l.End.Y))}\"/>\n");
                return;

            case PointRecord p:
                // A CAD point has no size. A zero-length path with a round cap
                // is the one primitive that draws as a single hairline dot at
                // every zoom level, which is what a viewer shows.
                body.Append(CultureInfo.InvariantCulture,
                    $"<path d=\"M {N(p.Location.X)} {N(SvgFormatting.FlipY(p.Location.Y))} l 0 0\"/>\n");
                return;

            case PolylineRecord pl:
                RenderPolyline(pl, body);
                return;

            case CircleRecord c:
                RenderCircle(c, body);
                return;

            case ArcRecord a:
                RenderArc(a, body);
                return;

            case EllipseRecord e:
                RenderEllipse(e, body);
                return;

            case SplineRecord s:
                RenderSpline(s, body);
                return;

            case PolygonRecord g:
                RenderPolygon(g, body);
                return;

            case TextRecord t:
                RenderText(t, body);
                return;

            // A document, view or warning record has no visual form, and the
            // SVG is deliberately not a place to render diagnostics.
            default:
                return;
        }
    }

    private static string N(double value) => SvgFormatting.Number(value);

    private static bool IsPlanar(Point3 normal) =>
        normal.X == 0.0 && normal.Y == 0.0 && normal.Z == 1.0;

    private static Point3 ToWorld(Point3 ocsPoint, Point3 normal) =>
        IsPlanar(normal) ? ocsPoint : CadTransform.ArbitraryAxis(normal).Apply(ocsPoint);

    private static Point3 VectorToWorld(Point3 ocsVector, Point3 normal) =>
        IsPlanar(normal) ? ocsVector : CadTransform.ArbitraryAxis(normal).ApplyVector(ocsVector);

    private static void RenderPolyline(PolylineRecord pl, StringBuilder body)
    {
        if (pl.Points.Count < 2)
        {
            return;
        }

        var path = new StringBuilder();
        Point3 first = ToWorld(pl.Points[0], pl.Normal);
        path.Append(CultureInfo.InvariantCulture, $"M {N(first.X)} {N(SvgFormatting.FlipY(first.Y))}");

        int segments = pl.Closed ? pl.Points.Count : pl.Points.Count - 1;
        for (int i = 0; i < segments; i++)
        {
            Point3 a = ToWorld(pl.Points[i], pl.Normal);
            Point3 b = ToWorld(pl.Points[(i + 1) % pl.Points.Count], pl.Normal);
            double bulge = i < pl.Bulges.Count ? pl.Bulges[i] : 0.0;
            AppendSegment(path, a, b, bulge);
        }

        if (pl.Closed)
        {
            path.Append(" Z");
        }

        body.Append(CultureInfo.InvariantCulture, $"<path d=\"{path}\"/>\n");
    }

    private static void AppendSegment(StringBuilder path, Point3 a, Point3 b, double bulge)
    {
        if (bulge == 0.0)
        {
            path.Append(CultureInfo.InvariantCulture, $" L {N(b.X)} {N(SvgFormatting.FlipY(b.Y))}");
            return;
        }

        // A bulge is tan(included angle / 4). Positive sweeps counter-clockwise
        // in CAD, which is clockwise once Y is mirrored, hence sweep flag 0.
        double included = 4.0 * Math.Atan(bulge);
        double chord = Math.Sqrt(((b.X - a.X) * (b.X - a.X)) + ((b.Y - a.Y) * (b.Y - a.Y)));
        if (chord == 0.0)
        {
            path.Append(CultureInfo.InvariantCulture, $" L {N(b.X)} {N(SvgFormatting.FlipY(b.Y))}");
            return;
        }

        double radius = chord * (1.0 + (bulge * bulge)) / (4.0 * Math.Abs(bulge));
        int largeArc = Math.Abs(included) > Math.PI ? 1 : 0;
        int sweep = bulge > 0.0 ? 0 : 1;
        path.Append(CultureInfo.InvariantCulture,
            $" A {N(radius)} {N(radius)} 0 {largeArc} {sweep} {N(b.X)} {N(SvgFormatting.FlipY(b.Y))}");
    }

    private static void RenderCircle(CircleRecord c, StringBuilder body)
    {
        if (!IsPlanar(c.Normal))
        {
            RenderSampledConic(
                c.Center, new Point3(c.Radius, 0.0, 0.0), new Point3(0.0, c.Radius, 0.0),
                0.0, CurveMapping.TwoPi, c.Normal, closed: true, body);
            return;
        }

        body.Append(CultureInfo.InvariantCulture,
            $"<circle cx=\"{N(c.Center.X)}\" cy=\"{N(SvgFormatting.FlipY(c.Center.Y))}\" r=\"{N(c.Radius)}\"/>\n");
    }

    private static void RenderArc(ArcRecord a, StringBuilder body)
    {
        if (!IsPlanar(a.Normal))
        {
            RenderSampledConic(
                a.Center, new Point3(a.Radius, 0.0, 0.0), new Point3(0.0, a.Radius, 0.0),
                a.Start, a.End, a.Normal, closed: false, body);
            return;
        }

        double delta = a.End - a.Start;
        if (delta >= CurveMapping.TwoPi)
        {
            body.Append(CultureInfo.InvariantCulture,
                $"<circle cx=\"{N(a.Center.X)}\" cy=\"{N(SvgFormatting.FlipY(a.Center.Y))}\" r=\"{N(a.Radius)}\"/>\n");
            return;
        }

        double sx = a.Center.X + (a.Radius * Math.Cos(a.Start));
        double sy = a.Center.Y + (a.Radius * Math.Sin(a.Start));
        double ex = a.Center.X + (a.Radius * Math.Cos(a.End));
        double ey = a.Center.Y + (a.Radius * Math.Sin(a.End));
        int largeArc = delta > Math.PI ? 1 : 0;

        body.Append(CultureInfo.InvariantCulture,
            $"<path d=\"M {N(sx)} {N(SvgFormatting.FlipY(sy))} A {N(a.Radius)} {N(a.Radius)} 0 {largeArc} 0 {N(ex)} {N(SvgFormatting.FlipY(ey))}\"/>\n");
    }

    private static void RenderEllipse(EllipseRecord e, StringBuilder body)
    {
        Point3 minor = MinorAxis(e);
        if (!IsPlanar(e.Normal))
        {
            RenderSampledConic(
                e.Center, e.MajorAxis, minor, e.Start, e.End, e.Normal,
                closed: e.End - e.Start >= CurveMapping.TwoPi, body);
            return;
        }

        double rx = Geometry.Length(e.MajorAxis);
        double ry = rx * e.Ratio;
        double rotation = SvgFormatting.RotationDegrees(Math.Atan2(e.MajorAxis.Y, e.MajorAxis.X));
        double delta = e.End - e.Start;

        if (delta >= CurveMapping.TwoPi)
        {
            body.Append(CultureInfo.InvariantCulture,
                $"<ellipse cx=\"{N(e.Center.X)}\" cy=\"{N(SvgFormatting.FlipY(e.Center.Y))}\" rx=\"{N(rx)}\" ry=\"{N(ry)}\" transform=\"rotate({N(rotation)} {N(e.Center.X)} {N(SvgFormatting.FlipY(e.Center.Y))})\"/>\n");
            return;
        }

        Point3 start = EllipsePoint(e, minor, e.Start);
        Point3 end = EllipsePoint(e, minor, e.End);
        int largeArc = delta > Math.PI ? 1 : 0;

        body.Append(CultureInfo.InvariantCulture,
            $"<path d=\"M {N(start.X)} {N(SvgFormatting.FlipY(start.Y))} A {N(rx)} {N(ry)} {N(rotation)} {largeArc} 0 {N(end.X)} {N(SvgFormatting.FlipY(end.Y))}\"/>\n");
    }

    private static Point3 MinorAxis(EllipseRecord e)
    {
        // The minor axis is the major turned a quarter turn inside the plane,
        // scaled by the ratio. In the OCS the plane is Z = 0, so that is a plain
        // two dimensional rotation.
        return new Point3(-e.MajorAxis.Y * e.Ratio, e.MajorAxis.X * e.Ratio, 0.0);
    }

    private static Point3 EllipsePoint(EllipseRecord e, Point3 minor, double t) => new(
        e.Center.X + (Math.Cos(t) * e.MajorAxis.X) + (Math.Sin(t) * minor.X),
        e.Center.Y + (Math.Cos(t) * e.MajorAxis.Y) + (Math.Sin(t) * minor.Y),
        e.Center.Z);

    private static void RenderSampledConic(
        Point3 center,
        Point3 major,
        Point3 minor,
        double start,
        double end,
        Point3 normal,
        bool closed,
        StringBuilder body)
    {
        // The SVG-only fallback for a curve whose plane is not parallel to XY.
        // Its projection is still a conic, but working out which one is a
        // rendering detail with no bearing on the semantic record, so it is
        // sampled instead. Uniform in parameter, so the output is a function of
        // the record and the sample count and nothing else.
        var path = new StringBuilder();
        for (int i = 0; i <= OutOfPlaneConicSamples; i++)
        {
            double t = start + ((end - start) * i / OutOfPlaneConicSamples);
            Point3 local = new(
                center.X + (Math.Cos(t) * major.X) + (Math.Sin(t) * minor.X),
                center.Y + (Math.Cos(t) * major.Y) + (Math.Sin(t) * minor.Y),
                center.Z);
            Point3 world = ToWorld(local, normal);
            path.Append(CultureInfo.InvariantCulture,
                $"{(i == 0 ? "M" : " L")} {N(world.X)} {N(SvgFormatting.FlipY(world.Y))}");
        }

        if (closed)
        {
            path.Append(" Z");
        }

        body.Append(CultureInfo.InvariantCulture, $"<path d=\"{path}\"/>\n");
    }

    private static void RenderSpline(SplineRecord s, StringBuilder body)
    {
        IReadOnlyList<Point3> samples = SampleSpline(s);
        if (samples.Count < 2)
        {
            return;
        }

        var path = new StringBuilder();
        for (int i = 0; i < samples.Count; i++)
        {
            path.Append(CultureInfo.InvariantCulture,
                $"{(i == 0 ? "M" : " L")} {N(samples[i].X)} {N(SvgFormatting.FlipY(samples[i].Y))}");
        }

        if (s.Closed)
        {
            path.Append(" Z");
        }

        body.Append(CultureInfo.InvariantCulture, $"<path d=\"{path}\"/>\n");
    }

    /// <summary>
    /// Samples a spline record into points, for drawing only.
    /// </summary>
    /// <param name="s">The record.</param>
    /// <returns>The sampled polyline.</returns>
    /// <remarks>
    /// Cox-de Boor over the record's own knots and weights. When the knot
    /// vector does not have the length the degree and control point count
    /// require, the control polygon is drawn instead: a wrong curve would be
    /// worse than an obviously angular one, and the semantic record still
    /// carries exactly what the drawing said.
    /// </remarks>
    public static IReadOnlyList<Point3> SampleSpline(SplineRecord s)
    {
        ArgumentNullException.ThrowIfNull(s);

        IReadOnlyList<Point3> control = s.ControlPoints;
        int degree = s.Degree;
        IReadOnlyList<double> knots = s.Knots;

        if (control.Count == 0)
        {
            return s.FitPoints;
        }

        if (degree < 1 || control.Count <= degree || knots.Count != control.Count + degree + 1)
        {
            return control;
        }

        double lo = knots[degree];
        double hi = knots[control.Count];
        if (!(hi > lo))
        {
            return control;
        }

        var points = new List<Point3>(SplineSamples + 1);
        for (int i = 0; i <= SplineSamples; i++)
        {
            double t = lo + ((hi - lo) * i / SplineSamples);
            if (i == SplineSamples)
            {
                t = hi;
            }

            points.Add(DeBoor(control, s.Weights, knots, degree, t));
        }

        return points;
    }

    private static Point3 DeBoor(
        IReadOnlyList<Point3> control,
        IReadOnlyList<double> weights,
        IReadOnlyList<double> knots,
        int degree,
        double t)
    {
        int n = control.Count;
        int span = degree;
        for (int i = degree; i < n; i++)
        {
            if (t >= knots[i] && t < knots[i + 1])
            {
                span = i;
                break;
            }

            span = i;
        }

        bool rational = weights.Count == n;
        var dx = new double[degree + 1];
        var dy = new double[degree + 1];
        var dz = new double[degree + 1];
        var dw = new double[degree + 1];
        for (int j = 0; j <= degree; j++)
        {
            int index = span - degree + j;
            double w = rational ? weights[index] : 1.0;
            dx[j] = control[index].X * w;
            dy[j] = control[index].Y * w;
            dz[j] = control[index].Z * w;
            dw[j] = w;
        }

        for (int r = 1; r <= degree; r++)
        {
            for (int j = degree; j >= r; j--)
            {
                int index = span - degree + j;
                double denominator = knots[index + degree - r + 1] - knots[index];
                double alpha = denominator == 0.0 ? 0.0 : (t - knots[index]) / denominator;
                dx[j] = ((1.0 - alpha) * dx[j - 1]) + (alpha * dx[j]);
                dy[j] = ((1.0 - alpha) * dy[j - 1]) + (alpha * dy[j]);
                dz[j] = ((1.0 - alpha) * dz[j - 1]) + (alpha * dz[j]);
                dw[j] = ((1.0 - alpha) * dw[j - 1]) + (alpha * dw[j]);
            }
        }

        double weight = dw[degree];
        return weight == 0.0
            ? new Point3(dx[degree], dy[degree], dz[degree])
            : new Point3(dx[degree] / weight, dy[degree] / weight, dz[degree] / weight);
    }

    private static void RenderPolygon(PolygonRecord g, StringBuilder body)
    {
        if (g.Points.Count < 3)
        {
            return;
        }

        var points = new StringBuilder();
        for (int i = 0; i < g.Points.Count; i++)
        {
            if (i > 0)
            {
                points.Append(' ');
            }

            points.Append(CultureInfo.InvariantCulture,
                $"{N(g.Points[i].X)},{N(SvgFormatting.FlipY(g.Points[i].Y))}");
        }

        // The one place fill is not none: a polygon record only ever comes from
        // fill-like source geometry.
        body.Append(CultureInfo.InvariantCulture,
            $"<polygon points=\"{points}\" fill=\"#000000\"/>\n");
    }

    private static void RenderText(TextRecord t, StringBuilder body)
    {
        Point3 world = ToWorld(t.Position, Point3.UnitZ);
        double x = world.X;
        double y = SvgFormatting.FlipY(world.Y);
        double rotation = SvgFormatting.RotationDegrees(t.Rotation);

        string anchor = t.HorizontalAlignment switch
        {
            "center" or "middle" => "middle",
            "right" => "end",
            _ => "start",
        };
        string baseline = t.VerticalAlignment switch
        {
            "top" => "text-before-edge",
            "middle" => "central",
            "bottom" => "text-after-edge",
            _ => "alphabetic",
        };

        string[] lines = t.Text.Split('\n');

        body.Append(CultureInfo.InvariantCulture,
            $"<text x=\"{N(x)}\" y=\"{N(y)}\" font-family=\"sans-serif\" font-size=\"{N(t.Height)}\" fill=\"#000000\" stroke=\"none\" text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\"");

        // Rotation and width factor both act about the anchor, in that order:
        // the width factor stretches along the rotated baseline, not along the
        // page. textLength was the obvious alternative and is a trap, because
        // it needs a length this tool has no font metrics to compute.
        var transform = new StringBuilder();
        if (rotation != 0.0)
        {
            transform.Append(CultureInfo.InvariantCulture, $"rotate({N(rotation)} {N(x)} {N(y)})");
        }

        if (t.WidthFactor != 1.0)
        {
            if (transform.Length > 0)
            {
                transform.Append(' ');
            }

            transform.Append(CultureInfo.InvariantCulture,
                $"translate({N(x)} {N(y)}) scale({N(t.WidthFactor)} 1) translate({N(-x)} {N(-y)})");
        }

        if (transform.Length > 0)
        {
            body.Append(CultureInfo.InvariantCulture, $" transform=\"{transform}\"");
        }

        body.Append(" xml:space=\"preserve\">");

        if (lines.Length == 1)
        {
            body.Append(SvgFormatting.Escape(lines[0]));
        }
        else
        {
            for (int i = 0; i < lines.Length; i++)
            {
                body.Append(CultureInfo.InvariantCulture,
                    $"<tspan x=\"{N(x)}\" dy=\"{N(i == 0 ? 0.0 : t.Height)}\">{SvgFormatting.Escape(lines[i])}</tspan>");
            }
        }

        body.Append("</text>\n");
    }

    private sealed class Bounds
    {
        private double _minX = double.PositiveInfinity;
        private double _minY = double.PositiveInfinity;
        private double _maxX = double.NegativeInfinity;
        private double _maxY = double.NegativeInfinity;

        public bool IsEmpty => double.IsInfinity(_minX);

        public void Add(double x, double y)
        {
            if (double.IsNaN(x) || double.IsNaN(y) || double.IsInfinity(x) || double.IsInfinity(y))
            {
                return;
            }

            _minX = Math.Min(_minX, x);
            _minY = Math.Min(_minY, y);
            _maxX = Math.Max(_maxX, x);
            _maxY = Math.Max(_maxY, y);
        }

        public void Add(Point3 p) => Add(p.X, p.Y);

        public (double X, double Y, double Width, double Height) ViewBox(double marginFraction)
        {
            if (IsEmpty)
            {
                // An empty drawing still needs a view box a renderer accepts.
                return (-1.0, -1.0, 2.0, 2.0);
            }

            double width = _maxX - _minX;
            double height = _maxY - _minY;
            double margin = Math.Max(width, height) * marginFraction;
            if (margin == 0.0)
            {
                margin = 1.0;
            }

            // The Y flip turns [minY, maxY] into [-maxY, -minY]: the whole
            // drawing, once, not one box per entity.
            return (
                _minX - margin,
                -_maxY - margin,
                width + (2.0 * margin),
                height + (2.0 * margin));
        }
    }

    private static Bounds ComputeBounds(IReadOnlyList<CanonicalRecord> records)
    {
        var bounds = new Bounds();
        foreach (CanonicalRecord record in records)
        {
            switch (record)
            {
                case LineRecord l:
                    bounds.Add(l.Start);
                    bounds.Add(l.End);
                    break;

                case PointRecord p:
                    bounds.Add(p.Location);
                    break;

                case PolylineRecord pl:
                    foreach (Point3 point in pl.Points)
                    {
                        bounds.Add(ToWorld(point, pl.Normal));
                    }

                    break;

                case CircleRecord c:
                {
                    Point3 centre = ToWorld(c.Center, c.Normal);
                    Point3 u = VectorToWorld(new Point3(c.Radius, 0.0, 0.0), c.Normal);
                    Point3 v = VectorToWorld(new Point3(0.0, c.Radius, 0.0), c.Normal);
                    AddConicBox(bounds, centre, u, v);
                    break;
                }

                case ArcRecord a:
                {
                    // The containing circle, not the arc's own extent. It never
                    // crops and it does not need four quadrant tests to be
                    // right, at the cost of some empty margin on a drawing made
                    // only of short arcs.
                    Point3 centre = ToWorld(a.Center, a.Normal);
                    Point3 u = VectorToWorld(new Point3(a.Radius, 0.0, 0.0), a.Normal);
                    Point3 v = VectorToWorld(new Point3(0.0, a.Radius, 0.0), a.Normal);
                    AddConicBox(bounds, centre, u, v);
                    break;
                }

                case EllipseRecord e:
                {
                    Point3 centre = ToWorld(e.Center, e.Normal);
                    Point3 u = VectorToWorld(e.MajorAxis, e.Normal);
                    Point3 v = VectorToWorld(MinorAxis(e), e.Normal);
                    AddConicBox(bounds, centre, u, v);
                    break;
                }

                case SplineRecord s:
                    // Control points, whose hull contains the curve.
                    foreach (Point3 point in s.ControlPoints)
                    {
                        bounds.Add(point);
                    }

                    foreach (Point3 point in s.FitPoints)
                    {
                        bounds.Add(point);
                    }

                    break;

                case PolygonRecord g:
                    foreach (Point3 point in g.Points)
                    {
                        bounds.Add(point);
                    }

                    break;

                case TextRecord t:
                {
                    Point3 world = ToWorld(t.Position, Point3.UnitZ);
                    bounds.Add(world);
                    // The text's own height, so a single line of text near the
                    // edge is not clipped by its own ascent.
                    bounds.Add(world.X, world.Y + t.Height);
                    break;
                }

                default:
                    break;
            }
        }

        return bounds;
    }

    private static void AddConicBox(Bounds bounds, Point3 centre, Point3 u, Point3 v)
    {
        // The axis-aligned box of an ellipse with conjugate semi-diameters u and
        // v is the centre plus or minus sqrt(u^2 + v^2) componentwise, whatever
        // rotation it carries.
        double dx = Math.Sqrt((u.X * u.X) + (v.X * v.X));
        double dy = Math.Sqrt((u.Y * u.Y) + (v.Y * v.Y));
        bounds.Add(centre.X - dx, centre.Y - dy);
        bounds.Add(centre.X + dx, centre.Y + dy);
    }
}
