using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// A block inserted with scale (2, 3), a 30 degree rotation and a non-origin
/// insertion point, with a second block nested one level inside it.
/// </summary>
/// <remarks>
/// INNER holds one line from (0, 0) to (1, 0). OUTER holds INNER inserted at
/// (5, 2) with no transform of its own. The top level inserts OUTER at (100, 50).
/// The claim test computes where the line must land from these numbers by hand.
/// </remarks>
public static class InsertTransform
{
    public const string Name = "regression_insert_transform";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/InsertTransform.cs",
        doc =>
        {
            var inner = new BlockRecord("GEN_INNER");
            inner.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(1, 0, 0)));
            doc.BlockRecords.Add(inner);

            var outer = new BlockRecord("GEN_OUTER");
            outer.Entities.Add(new Insert(inner) { InsertPoint = new XYZ(5, 2, 0) });
            doc.BlockRecords.Add(outer);

            doc.Entities.Add(new Insert(outer)
            {
                InsertPoint = new XYZ(100, 50, 0),
                XScale = 2.0,
                YScale = 3.0,
                ZScale = 1.0,
                Rotation = Math.PI / 6,
                Layer = Writer.Layer(doc),
            });
        });
}
