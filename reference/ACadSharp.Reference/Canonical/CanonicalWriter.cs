using System.Globalization;
using System.Text;

namespace ACadSharp.Reference.Canonical;

/// <summary>
/// Serialises canonical records to newline-delimited JSON.
/// </summary>
/// <remarks>
/// <para>
/// Every property order in here is fixed and documented. Nothing is sorted at
/// write time, nothing is reflected over, and no serializer is configured:
/// reflection order and serializer defaults are both things that can change
/// under a framework upgrade without a diff appearing in this repository, and
/// the output of this class is a hash in a manifest.
/// </para>
/// <para>
/// UTF-8, LF line endings, no BOM, no pretty printing, one object per line, a
/// final newline.
/// </para>
/// </remarks>
public static class CanonicalWriter
{
    /// <summary>
    /// Encodes a record stream as the exact bytes of a <c>.reference.jsonl</c> file.
    /// </summary>
    /// <param name="records">The records, already in canonical order.</param>
    /// <returns>UTF-8 bytes, with no byte order mark and a trailing LF.</returns>
    public static byte[] Encode(IEnumerable<CanonicalRecord> records)
    {
        ArgumentNullException.ThrowIfNull(records);

        var text = new StringBuilder();
        foreach (CanonicalRecord record in records)
        {
            AppendLine(text, record);
            text.Append('\n');
        }

        // UTF8Encoding(false) rather than Encoding.UTF8, which emits a BOM
        // preamble from GetPreamble and is easy to reach for by accident.
        return new UTF8Encoding(encoderShouldEmitUTF8Identifier: false).GetBytes(text.ToString());
    }

    /// <summary>
    /// Encodes one record as its single JSON line, with no trailing newline.
    /// </summary>
    /// <param name="record">The record.</param>
    /// <returns>The JSON text of the line.</returns>
    public static string EncodeLine(CanonicalRecord record)
    {
        var text = new StringBuilder();
        AppendLine(text, record);
        return text.ToString();
    }

    private static void AppendLine(StringBuilder text, CanonicalRecord record)
    {
        ArgumentNullException.ThrowIfNull(record);

        text.Append("{\"schema\":").Append(NumericFormatting.Whole(CanonicalSchema.Version));
        text.Append(",\"record\":").Append(NumericFormatting.Quoted(record.RecordName));

        switch (record)
        {
            case DocumentRecord d:
                Field(text, "format", NumericFormatting.Quoted(d.Format));
                Field(text, "acad_version", NumericFormatting.Quoted(d.AcadVersion));
                Field(text, "maintenance_version", NumericFormatting.Whole(d.MaintenanceVersion));
                break;

            case ViewRecord v:
                Field(text, "index", NumericFormatting.Whole(v.Index));
                Field(text, "name", NumericFormatting.Quoted(v.Name));
                Field(text, "kind", NumericFormatting.Quoted(KindName(v.Kind)));
                break;

            case LineRecord l:
                Field(text, "view", NumericFormatting.Whole(l.View));
                Field(text, "x1", NumericFormatting.Real(l.Start.X));
                Field(text, "y1", NumericFormatting.Real(l.Start.Y));
                Field(text, "z1", NumericFormatting.Real(l.Start.Z));
                Field(text, "x2", NumericFormatting.Real(l.End.X));
                Field(text, "y2", NumericFormatting.Real(l.End.Y));
                Field(text, "z2", NumericFormatting.Real(l.End.Z));
                break;

            case PolylineRecord p:
                Field(text, "view", NumericFormatting.Whole(p.View));
                Field(text, "closed", p.Closed ? "true" : "false");
                AppendNormal(text, p.Normal);
                Field(text, "points", PointArray(p.Points));
                Field(text, "bulges", DoubleArray(p.Bulges));
                break;

            case CircleRecord c:
                Field(text, "view", NumericFormatting.Whole(c.View));
                Field(text, "cx", NumericFormatting.Real(c.Center.X));
                Field(text, "cy", NumericFormatting.Real(c.Center.Y));
                Field(text, "cz", NumericFormatting.Real(c.Center.Z));
                Field(text, "radius", NumericFormatting.Real(c.Radius));
                AppendNormal(text, c.Normal);
                break;

            case ArcRecord a:
                Field(text, "view", NumericFormatting.Whole(a.View));
                Field(text, "cx", NumericFormatting.Real(a.Center.X));
                Field(text, "cy", NumericFormatting.Real(a.Center.Y));
                Field(text, "cz", NumericFormatting.Real(a.Center.Z));
                Field(text, "radius", NumericFormatting.Real(a.Radius));
                Field(text, "start", NumericFormatting.Real(a.Start));
                Field(text, "end", NumericFormatting.Real(a.End));
                AppendNormal(text, a.Normal);
                break;

            case EllipseRecord e:
                Field(text, "view", NumericFormatting.Whole(e.View));
                Field(text, "cx", NumericFormatting.Real(e.Center.X));
                Field(text, "cy", NumericFormatting.Real(e.Center.Y));
                Field(text, "cz", NumericFormatting.Real(e.Center.Z));
                Field(text, "mx", NumericFormatting.Real(e.MajorAxis.X));
                Field(text, "my", NumericFormatting.Real(e.MajorAxis.Y));
                Field(text, "mz", NumericFormatting.Real(e.MajorAxis.Z));
                Field(text, "ratio", NumericFormatting.Real(e.Ratio));
                Field(text, "start", NumericFormatting.Real(e.Start));
                Field(text, "end", NumericFormatting.Real(e.End));
                AppendNormal(text, e.Normal);
                break;

            case SplineRecord s:
                Field(text, "view", NumericFormatting.Whole(s.View));
                Field(text, "degree", NumericFormatting.Whole(s.Degree));
                Field(text, "closed", s.Closed ? "true" : "false");
                Field(text, "periodic", s.Periodic ? "true" : "false");
                Field(text, "control_points", PointArray(s.ControlPoints));
                Field(text, "knots", DoubleArray(s.Knots));
                Field(text, "weights", DoubleArray(s.Weights));
                Field(text, "fit_points", PointArray(s.FitPoints));
                break;

            case PolygonRecord g:
                Field(text, "view", NumericFormatting.Whole(g.View));
                Field(text, "points", PointArray(g.Points));
                break;

            case PointRecord pt:
                Field(text, "view", NumericFormatting.Whole(pt.View));
                Field(text, "x", NumericFormatting.Real(pt.Location.X));
                Field(text, "y", NumericFormatting.Real(pt.Location.Y));
                Field(text, "z", NumericFormatting.Real(pt.Location.Z));
                break;

            case TextRecord t:
                Field(text, "view", NumericFormatting.Whole(t.View));
                Field(text, "x", NumericFormatting.Real(t.Position.X));
                Field(text, "y", NumericFormatting.Real(t.Position.Y));
                Field(text, "z", NumericFormatting.Real(t.Position.Z));
                Field(text, "height", NumericFormatting.Real(t.Height));
                Field(text, "rotation", NumericFormatting.Real(t.Rotation));
                Field(text, "width_factor", NumericFormatting.Real(t.WidthFactor));
                Field(text, "halign", NumericFormatting.Quoted(t.HorizontalAlignment));
                Field(text, "valign", NumericFormatting.Quoted(t.VerticalAlignment));
                Field(text, "style", NumericFormatting.Quoted(t.Style));
                Field(text, "text", NumericFormatting.Quoted(t.Text));
                break;

            case WarningRecord w:
                Field(text, "category", NumericFormatting.Quoted(CategoryName(w.Category)));
                Field(text, "entity_type", NumericFormatting.Quoted(w.EntityType));
                Field(text, "detail", NumericFormatting.Quoted(w.Detail));
                break;

            default:
                throw new ArgumentOutOfRangeException(
                    nameof(record),
                    record.GetType().FullName,
                    "no canonical encoding for this record type; add one here rather than " +
                    "letting it fall through, or the record would vanish from the reference");
        }

        text.Append('}');
    }

    /// <summary>The wire spelling of a view kind.</summary>
    /// <param name="kind">The kind.</param>
    /// <returns>Its lowercase name.</returns>
    public static string KindName(ViewKind kind) => kind switch
    {
        ViewKind.Model => "model",
        ViewKind.Layout => "layout",
        ViewKind.Unknown => "unknown",
        _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, "unmapped view kind"),
    };

    /// <summary>The wire spelling of a warning category.</summary>
    /// <param name="category">The category.</param>
    /// <returns>Its snake_case name.</returns>
    public static string CategoryName(WarningCategory category) => category switch
    {
        WarningCategory.UnsupportedEntity => "unsupported_entity",
        WarningCategory.InvalidEntity => "invalid_entity",
        WarningCategory.MissingReference => "missing_reference",
        WarningCategory.MissingFont => "missing_font",
        WarningCategory.MalformedGeometry => "malformed_geometry",
        WarningCategory.ReaderNotification => "reader_notification",
        WarningCategory.Unknown => "unknown",
        _ => throw new ArgumentOutOfRangeException(
            nameof(category), category, "unmapped warning category"),
    };

    private static void AppendNormal(StringBuilder text, Point3 normal)
    {
        Field(text, "nx", NumericFormatting.Real(normal.X));
        Field(text, "ny", NumericFormatting.Real(normal.Y));
        Field(text, "nz", NumericFormatting.Real(normal.Z));
    }

    private static void Field(StringBuilder text, string name, string encodedValue)
    {
        text.Append(',').Append('"').Append(name).Append("\":").Append(encodedValue);
    }

    private static string PointArray(IReadOnlyList<Point3> points)
    {
        var text = new StringBuilder("[");
        for (int i = 0; i < points.Count; i++)
        {
            if (i > 0)
            {
                text.Append(',');
            }

            Point3 p = points[i];
            text.Append('[')
                .Append(NumericFormatting.Real(p.X)).Append(',')
                .Append(NumericFormatting.Real(p.Y)).Append(',')
                .Append(NumericFormatting.Real(p.Z)).Append(']');
        }

        return text.Append(']').ToString();
    }

    private static string DoubleArray(IReadOnlyList<double> values)
    {
        var text = new StringBuilder("[");
        for (int i = 0; i < values.Count; i++)
        {
            if (i > 0)
            {
                text.Append(',');
            }

            text.Append(NumericFormatting.Real(values[i]));
        }

        return text.Append(']').ToString();
    }

    /// <summary>
    /// Orders warning records the one way that cannot depend on how the reader
    /// happened to walk the file.
    /// </summary>
    /// <param name="warnings">The warnings, in whatever order they arrived.</param>
    /// <returns>The same warnings, sorted by category, then type, then detail.</returns>
    /// <remarks>
    /// Reader notifications arrive in an order that comes partly out of
    /// dictionary iteration inside ACadSharp, which is stable for one process
    /// but is not something this repository should be pinning a hash to.
    /// Section 15 allows exactly this: a collection whose order is not
    /// meaningful, sorted by a documented stable key. Duplicates are kept, so
    /// the number of warning records still equals the number of things that
    /// went wrong.
    /// </remarks>
    public static IReadOnlyList<WarningRecord> SortWarnings(IEnumerable<WarningRecord> warnings)
    {
        ArgumentNullException.ThrowIfNull(warnings);
        return warnings
            .OrderBy(w => CategoryName(w.Category), StringComparer.Ordinal)
            .ThenBy(w => w.EntityType, StringComparer.Ordinal)
            .ThenBy(w => w.Detail, StringComparer.Ordinal)
            .ToList();
    }

    /// <summary>
    /// A per-record-type census of a canonical stream.
    /// </summary>
    /// <param name="records">The records.</param>
    /// <returns>Counts keyed by record name, ordered by name.</returns>
    /// <remarks>
    /// Feeds the regeneration tool's summary, so a reviewer of an ACadSharp
    /// bump can see that four hundred arcs became four hundred polylines
    /// without decompressing anything.
    /// </remarks>
    public static IReadOnlyDictionary<string, int> CountByRecordName(
        IEnumerable<CanonicalRecord> records)
    {
        ArgumentNullException.ThrowIfNull(records);
        var counts = new SortedDictionary<string, int>(StringComparer.Ordinal);
        foreach (CanonicalRecord record in records)
        {
            counts.TryGetValue(record.RecordName, out int n);
            counts[record.RecordName] = n + 1;
        }

        return counts;
    }

    /// <summary>
    /// Formats a count map for a console summary.
    /// </summary>
    /// <param name="counts">The counts.</param>
    /// <returns>A single line, for example <c>arc:21, line:700</c>.</returns>
    public static string FormatCounts(IReadOnlyDictionary<string, int> counts)
    {
        ArgumentNullException.ThrowIfNull(counts);
        return string.Join(
            ", ",
            counts.Select(kv => string.Create(
                CultureInfo.InvariantCulture, $"{kv.Key}:{kv.Value}")));
    }
}
