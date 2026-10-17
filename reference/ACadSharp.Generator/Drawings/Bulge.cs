using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// An LWPOLYLINE of three bulge segments whose sagitta is known.
/// </summary>
/// <remarks>
/// A bulge is <c>2 * sagitta / chord</c>, so a chord of 10 with a sagitta of 2 is
/// 0.4, a sagitta of 5 (a semicircle) is 1.0, and a sagitta of -1.25 (the arc
/// bulging the other way) is -0.25. The last vertex's bulge is 0 because there
/// is no segment after it. The values are the claim test's constants too, so a
/// change here is a change there.
/// </remarks>
public static class Bulge
{
    public const string Name = "regression_bulge";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/Bulge.cs",
        doc =>
        {
            var lw = new LwPolyline { Layer = Writer.Layer(doc) };
            lw.Vertices.Add(new LwPolyline.Vertex(new XY(0, 0)) { Bulge = 0.4 });
            lw.Vertices.Add(new LwPolyline.Vertex(new XY(10, 0)) { Bulge = 1.0 });
            lw.Vertices.Add(new LwPolyline.Vertex(new XY(20, 0)) { Bulge = -0.25 });
            lw.Vertices.Add(new LwPolyline.Vertex(new XY(30, 0)));
            doc.Entities.Add(lw);
        });
}
