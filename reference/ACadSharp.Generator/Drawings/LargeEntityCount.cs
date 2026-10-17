using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// Fifty thousand lines, for the streaming claim.
/// </summary>
/// <remarks>
/// Line i runs from (i mod 250, i div 250) to one unit to the right. Integer
/// coordinates, not random ones, so the DWG, the reference stream and the SVG all
/// compress or stay small: this drawing exists to make the decoder stream, not to
/// make the repository heavy.
/// </remarks>
public static class LargeEntityCount
{
    public const string Name = "regression_large_entity_count";

    public const int Lines = 50_000;
    public const int Columns = 250;

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/LargeEntityCount.cs",
        doc =>
        {
            var layer = Writer.Layer(doc);
            for (int i = 0; i < Lines; i++)
            {
                double x = i % Columns;
                double y = i / Columns;
                doc.Entities.Add(new Line(new XYZ(x, y, 0), new XYZ(x + 1, y, 0)) { Layer = layer });
            }
        });
}
