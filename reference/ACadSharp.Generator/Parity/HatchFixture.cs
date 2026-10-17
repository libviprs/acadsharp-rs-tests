using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>
/// A solid hatch whose boundary is a five-vertex polyline, and a second hatch with
/// an outer loop and an island.
/// </summary>
public static class HatchFixture
{
    public const string Name = "parity_hatch";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/HatchFixture.cs",
        Build);

    private static Hatch.BoundaryPath Loop(BoundaryPathFlags flags, params (double X, double Y)[] points)
    {
        var path = new Hatch.BoundaryPath { Flags = flags | BoundaryPathFlags.Polyline };
        var edge = new Hatch.BoundaryPath.Polyline { IsClosed = true };
        foreach ((double x, double y) in points)
        {
            edge.Vertices.Add(new XYZ(x, y, 0));
        }

        path.Edges.Add(edge);
        return path;
    }

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        var five = new Hatch { IsSolid = true, Layer = layer };
        five.Paths.Add(Loop(BoundaryPathFlags.External, (0, 0), (10, 0), (12, 5), (5, 9), (-2, 5)));
        doc.Entities.Add(five);

        var ring = new Hatch { IsSolid = true, Layer = layer };
        ring.Paths.Add(Loop(BoundaryPathFlags.External, (30, 0), (50, 0), (50, 20), (30, 20)));
        ring.Paths.Add(Loop(BoundaryPathFlags.Default, (35, 5), (45, 5), (40, 15)));
        doc.Entities.Add(ring);
    }
}
