using ACadSharp.Entities;
using ACadSharp.Reference.Canonical;
using ACadSharp.Tables;

using CSMath;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// Maps ACadSharp entities onto canonical records.
/// </summary>
/// <remarks>
/// <para>
/// The rule the whole class follows: emit what a viewer should draw, in world
/// space, with analytic curves left analytic. Anything it has no representation
/// for becomes a warning record naming the type. Nothing is dropped silently,
/// because a differential over a stream that quietly lost a hundred entities
/// compares two identical absences and passes.
/// </para>
/// <para>
/// Coordinates live in the frame each record's own fields define. Records that
/// carry a normal (polyline, circle, arc, ellipse, text) express their points
/// in the object coordinate system that normal implies, because their angles
/// are measured in that plane and there is nowhere else to measure them from.
/// For the +Z normal that every planar drawing uses, that system is world
/// space, so in practice this is only visible on genuinely tilted geometry.
/// Records with no normal (line, point, polygon, spline) are plain world space.
/// </para>
/// </remarks>
/// <param name="warnings">Where unsupported and malformed findings are reported.</param>
/// <param name="resolver">The block recursion policy.</param>
public sealed class EntityExtractor(WarningCollector warnings, BlockResolver resolver)
{
    private readonly WarningCollector _warnings = warnings;
    private readonly BlockResolver _resolver = resolver;

    /// <summary>
    /// Extracts a whole entity collection into a sink, in document order.
    /// </summary>
    /// <param name="entities">The entities, in the order the document lists them.</param>
    /// <param name="view">Index of the view these belong to.</param>
    /// <param name="transform">Map from these entities' coordinates into world space.</param>
    /// <param name="depth">Current block nesting depth.</param>
    /// <param name="insideBlock">Whether this collection is a block definition being expanded.</param>
    /// <param name="sink">Where records are appended.</param>
    /// <remarks>
    /// Document order, not sorted and not draw order. ACadSharp exposes a
    /// deterministic enumeration order for a block record's entities and
    /// section 15 says to keep it. <c>GetSortedEntities</c> would give the
    /// sort-handle draw order instead, which is a different and equally
    /// defensible choice, but it is a choice the Rust side would then have to
    /// reproduce exactly, and document order is the one it gets for free.
    /// </remarks>
    public void ExtractAll(
        IEnumerable<Entity> entities,
        int view,
        CadTransform transform,
        int depth,
        bool insideBlock,
        List<CanonicalRecord> sink)
    {
        ArgumentNullException.ThrowIfNull(entities);
        ArgumentNullException.ThrowIfNull(sink);

        foreach (Entity entity in entities)
        {
            Extract(entity, view, transform, depth, insideBlock, sink);
        }
    }

    /// <summary>Extracts a single entity.</summary>
    /// <param name="entity">The entity.</param>
    /// <param name="view">Index of the view it belongs to.</param>
    /// <param name="transform">Map into world space.</param>
    /// <param name="depth">Current block nesting depth.</param>
    /// <param name="insideBlock">Whether this is inside a block definition being expanded.</param>
    /// <param name="sink">Where records are appended.</param>
    public void Extract(
        Entity entity,
        int view,
        CadTransform transform,
        int depth,
        bool insideBlock,
        List<CanonicalRecord> sink)
    {
        ArgumentNullException.ThrowIfNull(entity);
        ArgumentNullException.ThrowIfNull(sink);

        // An entity the drawing marks invisible is not drawn, and a reference
        // that emitted it would disagree with every viewer. Absence is the
        // correct representation here, which is why this one is not a warning.
        if (entity.IsInvisible)
        {
            return;
        }

        switch (entity)
        {
            // Structural markers. A SEQEND closes a vertex or attribute run and
            // a BLOCK/ENDBLK bracket a definition; none of the three is
            // drawable, so they are the one deliberate silent skip in here.
            case Seqend:
            case ACadSharp.Blocks.Block:
            case ACadSharp.Blocks.BlockEnd:
                return;

            case Line line:
                sink.Add(new LineRecord(
                    view,
                    transform.Apply(V(line.StartPoint)),
                    transform.Apply(V(line.EndPoint))));
                return;

            case Point point:
                sink.Add(new PointRecord(view, transform.Apply(V(point.Location))));
                return;

            // Arc derives from Circle, so it has to be matched first or every
            // arc in the corpus would silently become a full circle.
            case Arc arc:
                EmitArc(arc, view, transform, sink);
                return;

            case Circle circle:
                EmitCircle(circle, view, transform, sink);
                return;

            case Ellipse ellipse:
                EmitEllipse(ellipse, view, transform, sink);
                return;

            case Spline spline:
                EmitSpline(spline, view, transform, sink);
                return;

            case LwPolyline lw:
                EmitPolyline(
                    view,
                    transform,
                    V(lw.Normal),
                    lw.Vertices.Select(v => new Point3(v.Location.X, v.Location.Y, lw.Elevation)),
                    lw.Vertices.Select(v => v.Bulge),
                    lw.IsClosed,
                    sink);
                return;

            case Polyline2D polyline2D:
                EmitPolyline(
                    view,
                    transform,
                    V(polyline2D.Normal),
                    polyline2D.Vertices.Select(v => V(v.Location)),
                    polyline2D.Vertices.Select(v => v.Bulge),
                    polyline2D.IsClosed,
                    sink);
                return;

            case Polyline3D polyline3D:
                // A 3D polyline's vertices are already world space and it has no
                // bulges, so it gets the +Z normal and a row of zeroes rather
                // than a plane it does not have.
                EmitPolyline(
                    view,
                    transform,
                    Point3.UnitZ,
                    polyline3D.Vertices.Select(v => V(v.Location)),
                    polyline3D.Vertices.Select(_ => 0.0),
                    polyline3D.IsClosed,
                    sink);
                return;

            case Solid solid:
                // DXF stores a SOLID's corners 1, 2, 4, 3. Drawing them in
                // stored order gives a bow tie, so they are reordered here and
                // the record documents that its points are in perimeter order.
                EmitPolygon(
                    view,
                    CadTransform.Compose(transform, CadTransform.ArbitraryAxis(V(solid.Normal))),
                    [V(solid.FirstCorner), V(solid.SecondCorner), V(solid.FourthCorner), V(solid.ThirdCorner)],
                    sink);
                return;

            case Face3D face:
                EmitPolygon(
                    view,
                    transform,
                    [V(face.FirstCorner), V(face.SecondCorner), V(face.ThirdCorner), V(face.FourthCorner)],
                    sink);
                return;

            case AttributeDefinition attdef:
                // An ATTDEF inside a block definition is the template for an
                // attribute, not something a viewer draws when the block is
                // inserted: the insert's own ATTRIBs are drawn instead. At the
                // top of a view it is ordinary text.
                if (!insideBlock)
                {
                    EmitText(attdef, view, transform, sink);
                }

                return;

            case AttributeBase attrib:
                EmitText(attrib, view, transform, sink);
                return;

            case TextEntity text:
                EmitText(text, view, transform, sink);
                return;

            case MText mtext:
                EmitMText(mtext, view, transform, sink);
                return;

            case Hatch hatch:
                EmitHatch(hatch, view, transform, sink);
                return;

            case Insert insert:
                EmitInsert(insert, view, transform, depth, sink);
                return;

            case Dimension dimension:
                EmitDimension(dimension, view, transform, depth, sink);
                return;

            case UnknownEntity unknown:
                _warnings.Add(
                    WarningCategory.UnsupportedEntity,
                    unknown.DxfClass?.DxfName ?? nameof(UnknownEntity),
                    "unknown_entity");
                return;

            default:
                _warnings.Add(
                    WarningCategory.UnsupportedEntity,
                    entity.GetType().Name,
                    "no_canonical_mapping");
                return;
        }
    }

    private static Point3 V(XYZ v) => new(v.X, v.Y, v.Z);

    private void EmitCircle(Circle circle, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        CadTransform toWorld = CadTransform.Compose(
            transform, CadTransform.ArbitraryAxis(V(circle.Normal)));
        double r = circle.Radius;
        CurveMapping.MappedCurve mapped = CurveMapping.Map(
            V(circle.Center), new Point3(r, 0.0, 0.0), new Point3(0.0, r, 0.0), toWorld);

        if (mapped.Degenerate)
        {
            _warnings.Add(WarningCategory.MalformedGeometry, nameof(Circle), "degenerate_after_transform");
            return;
        }

        if (mapped.IsCircular)
        {
            sink.Add(new CircleRecord(view, mapped.Center, mapped.Radius, mapped.Normal));
            return;
        }

        // A non-uniformly scaled circle is an ellipse. Saying so is the honest
        // record; keeping a "circle" with one of the two radii would be a lie
        // the differential would then have to agree with.
        sink.Add(new EllipseRecord(
            view, mapped.Center, mapped.MajorAxis, mapped.Ratio, 0.0, CurveMapping.TwoPi, mapped.Normal));
    }

    private void EmitArc(Arc arc, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        CadTransform toWorld = CadTransform.Compose(
            transform, CadTransform.ArbitraryAxis(V(arc.Normal)));
        double r = arc.Radius;
        CurveMapping.MappedCurve mapped = CurveMapping.Map(
            V(arc.Center), new Point3(r, 0.0, 0.0), new Point3(0.0, r, 0.0), toWorld);

        if (mapped.Degenerate)
        {
            _warnings.Add(WarningCategory.MalformedGeometry, nameof(Arc), "degenerate_after_transform");
            return;
        }

        (double start, double end) = CurveMapping.Sweep(arc.StartAngle, arc.EndAngle, mapped.AngleOffset);
        if (mapped.IsCircular)
        {
            sink.Add(new ArcRecord(view, mapped.Center, mapped.Radius, start, end, mapped.Normal));
            return;
        }

        sink.Add(new EllipseRecord(
            view, mapped.Center, mapped.MajorAxis, mapped.Ratio, start, end, mapped.Normal));
    }

    private void EmitEllipse(Ellipse ellipse, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        // An ELLIPSE stores its centre and major axis in world space already, so
        // the only map that applies is the block chain.
        Point3 major = V(ellipse.MajorAxisEndPoint);
        Point3 normal = V(ellipse.Normal);
        Point3 minorDirection = Geometry.Normalize(Geometry.Cross(normal, major));
        Point3 minor = Geometry.Scale(minorDirection, Geometry.Length(major) * ellipse.RadiusRatio);

        CurveMapping.MappedCurve mapped = CurveMapping.Map(
            V(ellipse.Center), major, minor, transform);

        if (mapped.Degenerate)
        {
            _warnings.Add(WarningCategory.MalformedGeometry, nameof(Ellipse), "degenerate_after_transform");
            return;
        }

        (double start, double end) = CurveMapping.Sweep(
            ellipse.StartParameter, ellipse.EndParameter, mapped.AngleOffset);

        // A full ellipse is stored as 0 to 2*pi, and Sweep would fold that to a
        // zero-length arc. Nothing else can produce an exactly empty sweep.
        if (ellipse.StartParameter == ellipse.EndParameter
            || Math.Abs(ellipse.EndParameter - ellipse.StartParameter) >= CurveMapping.TwoPi)
        {
            start = CurveMapping.Normalize(ellipse.StartParameter + mapped.AngleOffset);
            end = start + CurveMapping.TwoPi;
        }

        double ratio = mapped.Ratio;
        Point3 majorOut = mapped.MajorAxis;
        if (transform.IsSimilarity)
        {
            // On the similarity path the ratio is invariant, so use the number
            // the drawing stores rather than one recomputed from two square
            // roots. Same value, more of the source digits.
            ratio = ellipse.RadiusRatio;
        }

        sink.Add(new EllipseRecord(view, mapped.Center, majorOut, ratio, start, end, mapped.Normal));
    }

    private void EmitSpline(Spline spline, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        // Affine maps commute with the rational B-spline basis, so control and
        // fit points move and degree, knots and weights do not.
        List<Point3> control = [.. spline.ControlPoints.Select(p => transform.Apply(V(p)))];
        List<Point3> fit = [.. spline.FitPoints.Select(p => transform.Apply(V(p)))];

        if (control.Count == 0 && fit.Count == 0)
        {
            _warnings.Add(WarningCategory.InvalidEntity, nameof(Spline), "no_control_or_fit_points");
            return;
        }

        sink.Add(new SplineRecord(
            view,
            spline.Degree,
            spline.IsClosed,
            spline.IsPeriodic,
            control,
            [.. spline.Knots],
            [.. spline.Weights],
            fit));
    }

    private void EmitPolyline(
        int view,
        CadTransform transform,
        Point3 normal,
        IEnumerable<Point3> localPoints,
        IEnumerable<double> bulges,
        bool closed,
        List<CanonicalRecord> sink)
    {
        CadTransform toWorld = CadTransform.Compose(transform, CadTransform.ArbitraryAxis(normal));

        // The plane the bulges live in, after the map. Taking the new normal
        // from the images of the local axes rather than from the old normal
        // keeps the sense of rotation consistent, so a mirrored block does not
        // need its bulge signs flipped by hand.
        Point3 u = toWorld.ApplyVector(new Point3(1.0, 0.0, 0.0));
        Point3 v = toWorld.ApplyVector(new Point3(0.0, 1.0, 0.0));
        Point3 worldNormal = Geometry.Normalize(Geometry.Cross(u, v));
        if (worldNormal == Point3.Zero)
        {
            worldNormal = Point3.UnitZ;
        }

        CadTransform ocs = CadTransform.ArbitraryAxis(worldNormal);
        Point3 ax = new(ocs.M00, ocs.M10, ocs.M20);
        Point3 ay = new(ocs.M01, ocs.M11, ocs.M21);

        var points = new List<Point3>();
        foreach (Point3 local in localPoints)
        {
            Point3 world = toWorld.Apply(local);
            points.Add(new Point3(
                Geometry.Dot(world, ax),
                Geometry.Dot(world, ay),
                Geometry.Dot(world, worldNormal)));
        }

        List<double> bulgeList = [.. bulges];
        if (points.Count == 0)
        {
            _warnings.Add(WarningCategory.InvalidEntity, "Polyline", "no_vertices");
            return;
        }

        while (bulgeList.Count < points.Count)
        {
            bulgeList.Add(0.0);
        }

        if (bulgeList.Count > points.Count)
        {
            bulgeList.RemoveRange(points.Count, bulgeList.Count - points.Count);
        }

        if (!transform.IsSimilarity && bulgeList.Exists(b => b != 0.0))
        {
            // A bulge is a circular arc by definition. Under a non-uniform
            // scale its image is elliptical and schema 1 has no way to say so
            // inside a polyline, so the bulge is carried through unchanged and
            // the approximation is declared rather than hidden.
            _warnings.Add(
                WarningCategory.MalformedGeometry, "Polyline", "non_uniform_scaled_bulge");
        }

        sink.Add(new PolylineRecord(view, closed, worldNormal, points, bulgeList));
    }

    private static void EmitPolygon(
        int view, CadTransform toWorld, IReadOnlyList<Point3> localCorners, List<CanonicalRecord> sink)
    {
        var points = new List<Point3>(localCorners.Count);
        foreach (Point3 corner in localCorners)
        {
            Point3 world = toWorld.Apply(corner);
            // A degenerate quad is how DXF spells a triangle: the last two
            // corners coincide. Emitting the duplicate would give the polygon a
            // zero-length edge.
            if (points.Count > 0 && points[^1] == world)
            {
                continue;
            }

            points.Add(world);
        }

        if (points.Count >= 3)
        {
            sink.Add(new PolygonRecord(view, points));
        }
    }

    private void EmitText(TextEntity text, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        // DXF puts the drawn position in the second alignment point whenever
        // the text is anything other than left-baseline.
        bool useAlignment = text.HorizontalAlignment != TextHorizontalAlignment.Left
            || text.VerticalAlignment != TextVerticalAlignmentType.Baseline;
        Point3 anchor = useAlignment ? V(text.AlignmentPoint) : V(text.InsertPoint);

        EmitTextCommon(
            view,
            transform,
            V(text.Normal),
            anchor,
            text.Height,
            text.Rotation,
            text.WidthFactor,
            HorizontalName(text.HorizontalAlignment),
            VerticalName(text.VerticalAlignment),
            text.Style?.Name ?? string.Empty,
            text.Value,
            sink);
    }

    private void EmitMText(MText mtext, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        (string horizontal, string vertical) = AttachmentNames(mtext.AttachmentPoint);

        // MTEXT has no width factor of its own. One keeps the field set of a
        // text record the same whichever entity produced it.
        EmitTextCommon(
            view,
            transform,
            V(mtext.Normal),
            V(mtext.InsertPoint),
            mtext.Height,
            mtext.Rotation,
            1.0,
            horizontal,
            vertical,
            mtext.Style?.Name ?? string.Empty,
            mtext.Value,
            sink);
    }

    private void EmitTextCommon(
        int view,
        CadTransform transform,
        Point3 normal,
        Point3 localAnchor,
        double height,
        double rotation,
        double widthFactor,
        string horizontal,
        string vertical,
        string style,
        string? value,
        List<CanonicalRecord> sink)
    {
        CadTransform toWorld = CadTransform.Compose(transform, CadTransform.ArbitraryAxis(normal));

        // The baseline direction and the up direction, carried through the map
        // the same way a curve's conjugate diameters are. Their images give the
        // rotation, the height and the width factor all at once.
        Point3 baseline = toWorld.ApplyVector(new Point3(Math.Cos(rotation), Math.Sin(rotation), 0.0));
        Point3 up = toWorld.ApplyVector(new Point3(-Math.Sin(rotation), Math.Cos(rotation), 0.0));
        Point3 worldNormal = Geometry.Normalize(Geometry.Cross(baseline, up));
        if (worldNormal == Point3.Zero)
        {
            _warnings.Add(WarningCategory.MalformedGeometry, "Text", "degenerate_after_transform");
            return;
        }

        CadTransform ocs = CadTransform.ArbitraryAxis(worldNormal);
        Point3 ax = new(ocs.M00, ocs.M10, ocs.M20);
        Point3 ay = new(ocs.M01, ocs.M11, ocs.M21);

        Point3 world = toWorld.Apply(localAnchor);
        var position = new Point3(
            Geometry.Dot(world, ax), Geometry.Dot(world, ay), Geometry.Dot(world, worldNormal));

        double baselineLength = Geometry.Length(baseline);
        double upLength = Geometry.Length(up);
        double outRotation = CurveMapping.Normalize(
            Math.Atan2(Geometry.Dot(baseline, ay), Geometry.Dot(baseline, ax)));

        double outHeight = height;
        double outWidth = widthFactor;
        if (!transform.IsSimilarity)
        {
            outHeight = height * upLength;
            outWidth = upLength == 0.0 ? widthFactor : widthFactor * baselineLength / upLength;
        }
        else if (baselineLength != 1.0)
        {
            outHeight = height * baselineLength;
        }

        sink.Add(new TextRecord(
            view,
            position,
            outHeight,
            outRotation,
            outWidth,
            horizontal,
            vertical,
            style,
            NumericFormatting.NormalizeLineEndings(value)));
    }

    private void EmitHatch(Hatch hatch, int view, CadTransform transform, List<CanonicalRecord> sink)
    {
        CadTransform toWorld = CadTransform.Compose(
            transform, CadTransform.ArbitraryAxis(V(hatch.Normal)));
        double elevation = hatch.Elevation;

        if (hatch.Paths.Count == 0)
        {
            _warnings.Add(WarningCategory.InvalidEntity, nameof(Hatch), "no_boundary_paths");
            return;
        }

        foreach (Hatch.BoundaryPath path in hatch.Paths)
        {
            foreach (Hatch.BoundaryPath.Edge edge in path.Edges)
            {
                EmitHatchEdge(edge, view, toWorld, elevation, sink);
            }

            if (path.Edges.Count == 0)
            {
                _warnings.Add(WarningCategory.InvalidEntity, nameof(Hatch), "boundary_path_without_edges");
            }
        }

        // Schema 1 draws a hatch's boundary and not its fill or its pattern
        // lines. That is a stated limit of the reference, not a decoding
        // failure, so it is not a warning: it would fire on every hatch in
        // every fixture and drown the warnings that mean something.
    }

    private void EmitHatchEdge(
        Hatch.BoundaryPath.Edge edge,
        int view,
        CadTransform toWorld,
        double elevation,
        List<CanonicalRecord> sink)
    {
        switch (edge)
        {
            case Hatch.BoundaryPath.Line line:
                sink.Add(new LineRecord(
                    view,
                    toWorld.Apply(new Point3(line.Start.X, line.Start.Y, elevation)),
                    toWorld.Apply(new Point3(line.End.X, line.End.Y, elevation))));
                return;

            case Hatch.BoundaryPath.Arc arc:
            {
                double r = arc.Radius;
                CurveMapping.MappedCurve mapped = CurveMapping.Map(
                    new Point3(arc.Center.X, arc.Center.Y, elevation),
                    new Point3(r, 0.0, 0.0),
                    new Point3(0.0, r, 0.0),
                    toWorld);
                if (mapped.Degenerate)
                {
                    _warnings.Add(WarningCategory.MalformedGeometry, "HatchArc", "degenerate_after_transform");
                    return;
                }

                // A clockwise boundary arc is stored with its angles the other
                // way round; the canonical form always sweeps counter-clockwise.
                double startAngle = arc.CounterClockWise ? arc.StartAngle : arc.EndAngle;
                double endAngle = arc.CounterClockWise ? arc.EndAngle : arc.StartAngle;
                (double start, double end) = CurveMapping.Sweep(startAngle, endAngle, mapped.AngleOffset);
                if (mapped.IsCircular)
                {
                    sink.Add(new ArcRecord(view, mapped.Center, mapped.Radius, start, end, mapped.Normal));
                }
                else
                {
                    sink.Add(new EllipseRecord(
                        view, mapped.Center, mapped.MajorAxis, mapped.Ratio, start, end, mapped.Normal));
                }

                return;
            }

            case Hatch.BoundaryPath.Ellipse ellipse:
            {
                var major = new Point3(ellipse.MajorAxisEndPoint.X, ellipse.MajorAxisEndPoint.Y, 0.0);
                Point3 minorDirection = Geometry.Normalize(Geometry.Cross(Point3.UnitZ, major));
                Point3 minor = Geometry.Scale(
                    minorDirection, Geometry.Length(major) * ellipse.RadiusRatio);
                CurveMapping.MappedCurve mapped = CurveMapping.Map(
                    new Point3(ellipse.Center.X, ellipse.Center.Y, elevation), major, minor, toWorld);
                if (mapped.Degenerate)
                {
                    _warnings.Add(
                        WarningCategory.MalformedGeometry, "HatchEllipse", "degenerate_after_transform");
                    return;
                }

                double startAngle = ellipse.CounterClockWise ? ellipse.StartAngle : ellipse.EndAngle;
                double endAngle = ellipse.CounterClockWise ? ellipse.EndAngle : ellipse.StartAngle;
                (double start, double end) = CurveMapping.Sweep(startAngle, endAngle, mapped.AngleOffset);
                sink.Add(new EllipseRecord(
                    view,
                    mapped.Center,
                    mapped.MajorAxis,
                    toWorld.IsSimilarity ? ellipse.RadiusRatio : mapped.Ratio,
                    start,
                    end,
                    mapped.Normal));
                return;
            }

            case Hatch.BoundaryPath.Polyline polyline:
                EmitPolyline(
                    view,
                    toWorld,
                    Point3.UnitZ,
                    polyline.Vertices.Select(p => new Point3(p.X, p.Y, p.Z)),
                    polyline.Bulges,
                    polyline.IsClosed,
                    sink);
                return;

            case Hatch.BoundaryPath.Spline spline:
            {
                List<Point3> control =
                    [.. spline.ControlPoints.Select(p => toWorld.Apply(new Point3(p.X, p.Y, p.Z)))];
                if (control.Count == 0)
                {
                    _warnings.Add(WarningCategory.InvalidEntity, "HatchSpline", "no_control_points");
                    return;
                }

                sink.Add(new SplineRecord(
                    view,
                    spline.Degree,
                    false,
                    spline.IsPeriodic,
                    control,
                    [.. spline.Knots],
                    [.. spline.Weights],
                    [.. spline.FitPoints.Select(p => toWorld.Apply(new Point3(p.X, p.Y, elevation)))]));
                return;
            }

            default:
                _warnings.Add(
                    WarningCategory.UnsupportedEntity, "HatchEdge", edge.GetType().Name);
                return;
        }
    }

    private void EmitInsert(
        Insert insert, int view, CadTransform transform, int depth, List<CanonicalRecord> sink)
    {
        BlockRecord? block = insert.Block;
        if (!_resolver.TryEnter(block, depth))
        {
            return;
        }

        try
        {
            foreach ((int column, int row) in _resolver.ArrayCells(insert))
            {
                CadTransform child = CadTransform.Compose(
                    transform, BlockResolver.InsertTransform(insert, column, row));
                ExtractAll(block!.Entities, view, child, depth + 1, insideBlock: true, sink);
            }
        }
        finally
        {
            _resolver.Leave(block!);
        }

        // The insert's own ATTRIBs are positioned in the space that contains the
        // insert, not in the block's, so they take the parent transform.
        foreach (AttributeEntity attribute in insert.Attributes)
        {
            Extract(attribute, view, transform, depth, insideBlock: false, sink);
        }
    }

    private void EmitDimension(
        Dimension dimension, int view, CadTransform transform, int depth, List<CanonicalRecord> sink)
    {
        // A dimension's drawn form is the anonymous block the writer generated
        // for it: the extension lines, the arrowheads and the measurement text.
        // Reconstructing that from the definition points would be a second
        // dimension engine, and a reference is not the place for one.
        BlockRecord? block = dimension.Block;
        if (block is null)
        {
            _warnings.Add(
                WarningCategory.MissingReference, dimension.GetType().Name, "dimension_block_missing");
            return;
        }

        if (!_resolver.TryEnter(block, depth))
        {
            return;
        }

        try
        {
            ExtractAll(block.Entities, view, transform, depth + 1, insideBlock: true, sink);
        }
        finally
        {
            _resolver.Leave(block);
        }
    }

    private static string HorizontalName(TextHorizontalAlignment alignment) => alignment switch
    {
        TextHorizontalAlignment.Left => "left",
        TextHorizontalAlignment.Center => "center",
        TextHorizontalAlignment.Right => "right",
        TextHorizontalAlignment.Aligned => "aligned",
        TextHorizontalAlignment.Middle => "middle",
        TextHorizontalAlignment.Fit => "fit",
        _ => "left",
    };

    private static string VerticalName(TextVerticalAlignmentType alignment) => alignment switch
    {
        TextVerticalAlignmentType.Baseline => "baseline",
        TextVerticalAlignmentType.Bottom => "bottom",
        TextVerticalAlignmentType.Middle => "middle",
        TextVerticalAlignmentType.Top => "top",
        _ => "baseline",
    };

    private static (string Horizontal, string Vertical) AttachmentNames(AttachmentPointType attachment)
        => attachment switch
        {
            AttachmentPointType.TopLeft => ("left", "top"),
            AttachmentPointType.TopCenter => ("center", "top"),
            AttachmentPointType.TopRight => ("right", "top"),
            AttachmentPointType.MiddleLeft => ("left", "middle"),
            AttachmentPointType.MiddleCenter => ("center", "middle"),
            AttachmentPointType.MiddleRight => ("right", "middle"),
            AttachmentPointType.BottomLeft => ("left", "bottom"),
            AttachmentPointType.BottomCenter => ("center", "bottom"),
            AttachmentPointType.BottomRight => ("right", "bottom"),
            _ => ("left", "top"),
        };
}
