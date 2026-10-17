using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>
/// A TEXT with non-ASCII content, an MTEXT, an ATTDEF at the top of the drawing,
/// and an insert whose attribute has to come out as visible text.
/// </summary>
public static class TextFixture
{
    public const string Name = "parity_text";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/TextFixture.cs",
        Build);

    private static void Build(CadDocument doc)
    {
        var layer = Writer.Layer(doc);
        doc.Entities.Add(new TextEntity
        {
            Value = "Grüße 北京 – ünï",
            InsertPoint = new XYZ(10, 20, 0),
            Height = 2.5,
            Rotation = 0.3,
            Layer = layer,
        });
        doc.Entities.Add(new MText
        {
            Value = "MTEXT content",
            InsertPoint = new XYZ(30, 40, 0),
            Height = 1.8,
            Layer = layer,
        });
        doc.Entities.Add(new AttributeDefinition
        {
            Tag = "TAG1",
            Prompt = "prompt",
            Value = "ATTDEF-VALUE",
            InsertPoint = new XYZ(50, 0, 0),
            Height = 1.2,
            Layer = layer,
        });

        var block = new BlockRecord("GEN_ATT_BLK");
        block.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(1, 0, 0)));
        doc.BlockRecords.Add(block);
        var insert = new Insert(block) { InsertPoint = new XYZ(60, 0, 0), Layer = layer };
        insert.Attributes.Add(new AttributeEntity
        {
            Tag = "T",
            InsertPoint = new XYZ(60, 2, 0),
            Height = 1.5,
            Value = "ATT-VISIBLE",
        });
        doc.Entities.Add(insert);
    }
}
