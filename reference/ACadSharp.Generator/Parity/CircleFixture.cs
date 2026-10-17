using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>Two circles, one small and off-origin and one large.</summary>
public static class CircleFixture
{
    public const string Name = "parity_circle";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/CircleFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        doc.Entities.Add(new Circle { Center = new XYZ(3, 4, 0), Radius = 2.5, Layer = layer });
        doc.Entities.Add(new Circle { Center = new XYZ(-10.5, 0.25, 0), Radius = 100, Layer = layer });
    }
}
