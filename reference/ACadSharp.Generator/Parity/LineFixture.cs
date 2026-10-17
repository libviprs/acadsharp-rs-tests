using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>A 3D line, and a long one along X.</summary>
public static class LineFixture
{
    public const string Name = "parity_line";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/LineFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        doc.Entities.Add(new Line(new XYZ(1, 2, 3), new XYZ(4, 5, 6)) { Layer = layer });
        doc.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(100, 0, 0)) { Layer = layer });
    }
}
