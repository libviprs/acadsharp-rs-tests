using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>
/// A block nested two levels deep and inserted with scale and rotation, so the
/// transformed coordinates can be worked out by hand.
/// </summary>
/// <remarks>
/// L1 holds the line (0,0)-(1,0). L2 holds L1 inserted at (2,0). L3 holds L2
/// inserted at (0,3). The drawing inserts L3 at (100,50) with scale 2 and a quarter
/// turn. A point (x,0) in L1 is (x+2,3) in L3, (2x+4,6) scaled, (-6,2x+4) rotated and
/// (94,2x+54) placed: the line is (94,54)-(94,56). A second, untransformed insert of
/// L1 at (200,0) gives the identity case beside it.
/// </remarks>
public static class InsertFixture
{
    public const string Name = "parity_insert";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/InsertFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        var l1 = new BlockRecord("GEN_L1");
        l1.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(1, 0, 0)));
        doc.BlockRecords.Add(l1);
        var l2 = new BlockRecord("GEN_L2");
        l2.Entities.Add(new Insert(l1) { InsertPoint = new XYZ(2, 0, 0) });
        doc.BlockRecords.Add(l2);
        var l3 = new BlockRecord("GEN_L3");
        l3.Entities.Add(new Insert(l2) { InsertPoint = new XYZ(0, 3, 0) });
        doc.BlockRecords.Add(l3);

        doc.Entities.Add(new Insert(l3)
        {
            InsertPoint = new XYZ(100, 50, 0),
            XScale = 2.0,
            YScale = 2.0,
            ZScale = 2.0,
            Rotation = Math.PI / 2,
            Layer = layer,
        });
        doc.Entities.Add(new Insert(l1) { InsertPoint = new XYZ(200, 0, 0), Layer = layer });
    }
}
