using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// A hatch with an outer loop and two islands, each loop a closed polyline path.
/// </summary>
/// <remarks>
/// Polyline paths on purpose: schema 1 emits one analytic record per boundary
/// edge, and a polyline path is one edge, so the three loops come out as exactly
/// three closed <c>polyline</c> records. Loops built from separate line edges
/// would come out as twelve <c>line</c> records and the ring structure would be
/// invisible in the output.
/// </remarks>
public static class HatchLoops
{
    public const string Name = "regression_hatch_loops";

    private static Hatch.BoundaryPath Ring(double x0, double y0, double x1, double y1, BoundaryPathFlags flags)
    {
        var path = new Hatch.BoundaryPath { Flags = flags };
        var edge = new Hatch.BoundaryPath.Polyline { IsClosed = true };
        edge.Vertices.Add(new XYZ(x0, y0, 0));
        edge.Vertices.Add(new XYZ(x1, y0, 0));
        edge.Vertices.Add(new XYZ(x1, y1, 0));
        edge.Vertices.Add(new XYZ(x0, y1, 0));
        path.Edges.Add(edge);
        return path;
    }

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/HatchLoops.cs",
        doc =>
        {
            var hatch = new Hatch { IsSolid = true, Layer = Writer.Layer(doc) };
            hatch.Paths.Add(Ring(0, 0, 30, 20, BoundaryPathFlags.External | BoundaryPathFlags.Polyline));
            hatch.Paths.Add(Ring(5, 5, 12, 12, BoundaryPathFlags.Polyline));
            hatch.Paths.Add(Ring(18, 5, 25, 15, BoundaryPathFlags.Polyline));
            doc.Entities.Add(hatch);
        });
}
