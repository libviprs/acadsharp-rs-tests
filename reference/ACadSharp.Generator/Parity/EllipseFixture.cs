using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>A full ellipse and an elliptical arc with a rotated major axis.</summary>
public static class EllipseFixture
{
    public const string Name = "parity_ellipse";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/EllipseFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        doc.Entities.Add(new Ellipse { Center = new XYZ(1, 2, 0), MajorAxisEndPoint = new XYZ(4, 0, 0), RadiusRatio = 0.25, StartParameter = 0, EndParameter = 2 * Math.PI, Layer = layer });
        doc.Entities.Add(new Ellipse { Center = new XYZ(-5, -5, 0), MajorAxisEndPoint = new XYZ(0, 3, 0), RadiusRatio = 0.5, StartParameter = 0.5, EndParameter = 2.5, Layer = layer });
    }
}
