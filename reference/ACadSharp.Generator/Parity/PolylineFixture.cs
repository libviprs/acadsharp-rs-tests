using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>
/// Every polyline form the DWG reader has: a closed lightweight polyline with
/// bulges, an open one without, a 2D polyline with a bulge, and a 3D polyline.
/// </summary>
public static class PolylineFixture
{
    public const string Name = "parity_polyline";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/PolylineFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);

        var closed = new LwPolyline { IsClosed = true, Layer = layer };
        closed.Vertices.Add(new LwPolyline.Vertex(new XY(0, 0)));
        closed.Vertices.Add(new LwPolyline.Vertex(new XY(10, 0)) { Bulge = 0.5 });
        closed.Vertices.Add(new LwPolyline.Vertex(new XY(10, 10)) { Bulge = -0.25 });
        closed.Vertices.Add(new LwPolyline.Vertex(new XY(0, 10)));
        doc.Entities.Add(closed);

        var open = new LwPolyline { Layer = layer };
        open.Vertices.Add(new LwPolyline.Vertex(new XY(20, 0)));
        open.Vertices.Add(new LwPolyline.Vertex(new XY(25, 5)));
        open.Vertices.Add(new LwPolyline.Vertex(new XY(30, 0)));
        doc.Entities.Add(open);

        var p2 = new Polyline2D { Layer = layer };
        p2.Vertices.Add(new Vertex2D(new XYZ(40, 0, 0)));
        p2.Vertices.Add(new Vertex2D(new XYZ(50, 5, 0)) { Bulge = 0.3 });
        p2.Vertices.Add(new Vertex2D(new XYZ(60, 0, 0)));
        doc.Entities.Add(p2);

        doc.Entities.Add(new Polyline3D(
            new List<XYZ> { new XYZ(70, 0, 0), new XYZ(80, 5, 5), new XYZ(90, 0, 10) },
            false) { Layer = layer });
    }
}
