namespace ACadSharp.Reference.Canonical;

/// <summary>
/// The canonical record model, schema version 1.
/// </summary>
/// <remarks>
/// <para>
/// This is a test contract between the .NET reference oracle and a future
/// <c>acadsharp-rs</c>, not a public serialization format for either. It
/// describes what a viewer should draw, in world coordinates, with analytic
/// curves left analytic. It deliberately does not describe the ACadSharp
/// object model.
/// </para>
/// <para>
/// <c>docs/CANONICAL_SCHEMA.md</c> is the written version of everything in
/// this file and is the document a reader should start from.
/// </para>
/// </remarks>
public static class CanonicalSchema
{
    /// <summary>Schema version stamped into the <c>schema</c> field of every record.</summary>
    public const int Version = 1;

    /// <summary>Version of the SVG dialect the visual oracle emits.</summary>
    public const int SvgVersion = 1;
}

/// <summary>A world-space point. Z is carried even where the brief's examples are 2D.</summary>
/// <param name="X">World X.</param>
/// <param name="Y">World Y.</param>
/// <param name="Z">World Z.</param>
/// <remarks>
/// Dropping Z would make the record smaller and the differential weaker: an
/// entity that moved out of the drawing plane would compare equal to one that
/// did not.
/// </remarks>
public readonly record struct Point3(double X, double Y, double Z)
{
    /// <summary>The origin.</summary>
    public static Point3 Zero => new(0.0, 0.0, 0.0);

    /// <summary>The Z axis, the default normal for planar entities.</summary>
    public static Point3 UnitZ => new(0.0, 0.0, 1.0);
}

/// <summary>What a view record describes.</summary>
public enum ViewKind
{
    /// <summary>Model space.</summary>
    Model,

    /// <summary>A paper-space layout.</summary>
    Layout,

    /// <summary>A block record that carries geometry but is neither of the above.</summary>
    Unknown,
}

/// <summary>The deterministic warning categories.</summary>
/// <remarks>
/// Fixed and small on purpose. A reader notification's text changes between
/// ACadSharp releases; the category it maps to should not, so a comparison can
/// be made on the category and the message can stay in the console log.
/// </remarks>
public enum WarningCategory
{
    /// <summary>An entity type the extractor has no canonical representation for.</summary>
    UnsupportedEntity,

    /// <summary>An entity whose own data is inconsistent or unusable.</summary>
    InvalidEntity,

    /// <summary>A handle or name the document points at but does not contain.</summary>
    MissingReference,

    /// <summary>A text style whose font the drawing names but does not carry.</summary>
    MissingFont,

    /// <summary>Geometry that survives the pipeline only approximately.</summary>
    MalformedGeometry,

    /// <summary>Anything the ACadSharp reader reported that is none of the above.</summary>
    ReaderNotification,

    /// <summary>Reserved for a notification that cannot be classified at all.</summary>
    Unknown,
}

/// <summary>Base of every canonical record.</summary>
public abstract record CanonicalRecord
{
    /// <summary>The value of the record's <c>record</c> field.</summary>
    public abstract string RecordName { get; }
}

/// <summary>The single document record, always first in the stream.</summary>
/// <param name="Format">Container format the bytes came from. Always <c>dwg</c> today.</param>
/// <param name="AcadVersion">The drawing's version code, for example <c>AC1032</c>.</param>
/// <param name="MaintenanceVersion">The header's maintenance version.</param>
/// <remarks>
/// Nothing here depends on where the file was on disk, when it was read, which
/// machine read it or which runtime did the reading. That is the whole
/// selection rule for this record.
/// </remarks>
public sealed record DocumentRecord(string Format, string AcadVersion, int MaintenanceVersion)
    : CanonicalRecord
{
    /// <inheritdoc/>
    public override string RecordName => "document";
}

/// <summary>One drawable space: model space or a paper-space layout.</summary>
/// <param name="Index">Position in the emitted view order, starting at zero.</param>
/// <param name="Name">The layout's name as the drawing spells it.</param>
/// <param name="Kind">Whether this is model space, a layout, or neither.</param>
public sealed record ViewRecord(int Index, string Name, ViewKind Kind) : CanonicalRecord
{
    /// <inheritdoc/>
    public override string RecordName => "view";
}

/// <summary>Base of every record that belongs to a view.</summary>
/// <param name="View">Index of the view record this belongs to.</param>
public abstract record EntityRecord(int View) : CanonicalRecord;

/// <summary>A straight segment in world space.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Start">First endpoint.</param>
/// <param name="End">Second endpoint.</param>
public sealed record LineRecord(int View, Point3 Start, Point3 End) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "line";
}

/// <summary>A chain of vertices, each of which may bulge into a circular arc.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Closed">Whether the last vertex joins back to the first.</param>
/// <param name="Normal">World-space normal of the plane the bulges live in.</param>
/// <param name="Points">The vertices, in document order.</param>
/// <param name="Bulges">
/// One bulge per vertex, always present and the same length as
/// <paramref name="Points"/>. A bulge is <c>tan(theta / 4)</c> for the arc from
/// that vertex to the next; zero means a straight segment. Always emitted, even
/// when every value is zero, so the field set of a polyline record never
/// depends on its contents.
/// </param>
public sealed record PolylineRecord(
    int View,
    bool Closed,
    Point3 Normal,
    IReadOnlyList<Point3> Points,
    IReadOnlyList<double> Bulges) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "polyline";
}

/// <summary>A full circle. Never flattened into vertices.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Center">World-space centre.</param>
/// <param name="Radius">Radius in drawing units.</param>
/// <param name="Normal">World-space normal of the circle's plane.</param>
public sealed record CircleRecord(int View, Point3 Center, double Radius, Point3 Normal)
    : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "circle";
}

/// <summary>A circular arc. Never flattened into vertices.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Center">World-space centre.</param>
/// <param name="Radius">Radius in drawing units.</param>
/// <param name="Start">Start angle in radians, counter-clockwise from the plane's X axis.</param>
/// <param name="End">End angle in radians. Sweeps counter-clockwise from <paramref name="Start"/>.</param>
/// <param name="Normal">World-space normal of the arc's plane.</param>
public sealed record ArcRecord(
    int View,
    Point3 Center,
    double Radius,
    double Start,
    double End,
    Point3 Normal) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "arc";
}

/// <summary>An ellipse or elliptical arc. Never flattened into vertices.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Center">World-space centre.</param>
/// <param name="MajorAxis">Vector from the centre to the end of the major axis.</param>
/// <param name="Ratio">Minor axis length divided by major axis length, in (0, 1].</param>
/// <param name="Start">Start parameter in radians. Zero is the major-axis end.</param>
/// <param name="End">End parameter in radians. A full ellipse is 0 to 2*pi.</param>
/// <param name="Normal">World-space normal of the ellipse's plane.</param>
public sealed record EllipseRecord(
    int View,
    Point3 Center,
    Point3 MajorAxis,
    double Ratio,
    double Start,
    double End,
    Point3 Normal) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "ellipse";
}

/// <summary>A NURBS curve, carried as its defining data rather than as samples.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Degree">Curve degree.</param>
/// <param name="Closed">Whether the curve is closed.</param>
/// <param name="Periodic">Whether the knot vector is periodic.</param>
/// <param name="ControlPoints">Control points in world space.</param>
/// <param name="Knots">The knot vector, as the drawing stores it.</param>
/// <param name="Weights">Control point weights. Empty when the curve is non-rational.</param>
/// <param name="FitPoints">Fit points in world space. Empty when the curve has none.</param>
public sealed record SplineRecord(
    int View,
    int Degree,
    bool Closed,
    bool Periodic,
    IReadOnlyList<Point3> ControlPoints,
    IReadOnlyList<double> Knots,
    IReadOnlyList<double> Weights,
    IReadOnlyList<Point3> FitPoints) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "spline";
}

/// <summary>A closed, filled region.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Points">Vertices in perimeter order.</param>
/// <remarks>
/// Perimeter order, not DXF storage order. A DXF <c>SOLID</c> stores its
/// corners 1, 2, 4, 3 and drawing them in stored order produces a bow tie.
/// </remarks>
public sealed record PolygonRecord(int View, IReadOnlyList<Point3> Points) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "polygon";
}

/// <summary>A single marked position.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Location">World-space position.</param>
/// <remarks>
/// Beyond the brief's minimum record set. The alternative was 40 warning
/// records per fixture for an entity that is trivially representable, which
/// would have buried the warnings that matter.
/// </remarks>
public sealed record PointRecord(int View, Point3 Location) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "point";
}

/// <summary>Text a viewer should draw.</summary>
/// <param name="View">Index of the owning view.</param>
/// <param name="Position">World-space anchor, already resolved from the entity's alignment.</param>
/// <param name="Height">Cap height in drawing units.</param>
/// <param name="Rotation">Rotation in radians, counter-clockwise.</param>
/// <param name="WidthFactor">Horizontal scaling. One for MTEXT, which has no such field.</param>
/// <param name="HorizontalAlignment">One of <c>left</c>, <c>center</c>, <c>right</c>, <c>aligned</c>, <c>middle</c>, <c>fit</c>.</param>
/// <param name="VerticalAlignment">One of <c>baseline</c>, <c>bottom</c>, <c>middle</c>, <c>top</c>.</param>
/// <param name="Style">Name of the text style, never a resolved font file path.</param>
/// <param name="Text">The string, with line endings normalised to LF and Unicode untouched.</param>
public sealed record TextRecord(
    int View,
    Point3 Position,
    double Height,
    double Rotation,
    double WidthFactor,
    string HorizontalAlignment,
    string VerticalAlignment,
    string Style,
    string Text) : EntityRecord(View)
{
    /// <inheritdoc/>
    public override string RecordName => "text";
}

/// <summary>Something the oracle could not represent, or the reader complained about.</summary>
/// <param name="Category">The deterministic category.</param>
/// <param name="EntityType">The ACadSharp type name, or the DXF name for an unknown class.</param>
/// <param name="Detail">
/// A short, deterministic discriminator. Never an exception message, never a
/// stack trace, never a handle.
/// </param>
public sealed record WarningRecord(WarningCategory Category, string EntityType, string Detail)
    : CanonicalRecord
{
    /// <inheritdoc/>
    public override string RecordName => "warning";
}
