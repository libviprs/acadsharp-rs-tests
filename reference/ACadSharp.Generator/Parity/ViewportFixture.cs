using ACadSharp.Entities;
using ACadSharp.Objects;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>
/// Model space plus two named layouts, one with extents the layout defines and one
/// with none, each holding a line so a paper view is not empty.
/// </summary>
public static class ViewportFixture
{
    public const string Name = "parity_viewport";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/ViewportFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        doc.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(10, 10, 0)) { Layer = Writer.Layer(doc) });

        var a = new Layout("Plan A") { MinExtents = new XYZ(0, 0, 0), MaxExtents = new XYZ(420, 297, 0) };
        doc.Layouts.Add(a);
        a.AssociatedBlock.Entities.Add(new Line(new XYZ(10, 10, 0), new XYZ(200, 10, 0)));

        var b = new Layout("Plan B");
        doc.Layouts.Add(b);
        b.AssociatedBlock.Entities.Add(new Line(new XYZ(20, 20, 0), new XYZ(30, 30, 0)));
    }
}
