using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>Two arcs whose centre, radius and angles are all different numbers, so a swapped field cannot pass.</summary>
public static class ArcFixture
{
    public const string Name = "parity_arc";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/ArcFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        doc.Entities.Add(new Arc { Center = new XYZ(12.5, -7.25, 0), Radius = 5.0, StartAngle = 0.5, EndAngle = 2.0, Layer = layer });
        doc.Entities.Add(new Arc { Center = new XYZ(-3, 2, 0), Radius = 0.75, StartAngle = 3.5, EndAngle = 5.75, Layer = layer });
    }
}
