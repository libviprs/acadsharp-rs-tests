using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// One dimension of each of the seven kinds, with geometry chosen so the measured
/// value is a number written down here and not computed by the thing under test.
/// </summary>
/// <remarks>
/// ACadSharp computes <c>Measurement</c> from the points and has no setter, so
/// "explicit measured value" means explicit points: linear 10, aligned 5 (a 3-4-5
/// triangle), radius 4, diameter 8, angular 90 degrees, angular 60 degrees, and an
/// ordinate at x = 120. Each dimension's anonymous block is generated with
/// <c>UpdateBlock</c> before the document is written, because the oracle draws a
/// dimension from that block and nothing else.
/// </remarks>
public static class DimensionFields
{
    public const string Name = "regression_dimension_fields";

    private static void AuthorText(CadDocument doc, Dimension dim, string text)
    {
        if (dim.Block is null)
        {
            dim.Block = new BlockRecord("*D9001");
        }

        dim.Block.Entities.Add(new TextEntity
        {
            Value = text,
            InsertPoint = dim.TextMiddlePoint,
            Height = 0.18,
            HorizontalAlignment = TextHorizontalAlignment.Center,
            VerticalAlignment = TextVerticalAlignmentType.Middle,
            AlignmentPoint = dim.TextMiddlePoint,
        });
    }

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/DimensionFields.cs",
        doc =>
        {
            var layer = Writer.Layer(doc);
            var dims = new List<Dimension>
            {
                new DimensionLinear
                {
                    FirstPoint = new XYZ(0, 0, 0),
                    SecondPoint = new XYZ(10, 0, 0),
                    DefinitionPoint = new XYZ(10, 0, 0),
                    TextMiddlePoint = new XYZ(5, 3, 0),
                    Offset = 3,
                },
                new DimensionAligned
                {
                    FirstPoint = new XYZ(0, 10, 0),
                    SecondPoint = new XYZ(3, 14, 0),
                    DefinitionPoint = new XYZ(3, 14, 0),
                    TextMiddlePoint = new XYZ(0.5, 14, 0),
                    Offset = 2,
                },
                new DimensionRadius
                {
                    DefinitionPoint = new XYZ(30, 0, 0),
                    AngleVertex = new XYZ(34, 0, 0),
                    LeaderLength = 1,
                    TextMiddlePoint = new XYZ(36, 1, 0),
                },
                new DimensionDiameter
                {
                    DefinitionPoint = new XYZ(54, 0, 0),
                    AngleVertex = new XYZ(46, 0, 0),
                    LeaderLength = 1,
                    TextMiddlePoint = new XYZ(50, 2, 0),
                },
                new DimensionAngular2Line
                {
                    FirstPoint = new XYZ(70, 0, 0),
                    SecondPoint = new XYZ(80, 0, 0),
                    AngleVertex = new XYZ(70, 0, 0),
                    DefinitionPoint = new XYZ(70, 10, 0),
                    DimensionArc = new XYZ(75, 5, 0),
                    TextMiddlePoint = new XYZ(75, 5, 0),
                },
                new DimensionAngular3Pt
                {
                    FirstPoint = new XYZ(100, 0, 0),
                    SecondPoint = new XYZ(95, 8.660254037844386, 0),
                    AngleVertex = new XYZ(90, 0, 0),
                    DefinitionPoint = new XYZ(97, 5, 0),
                    TextMiddlePoint = new XYZ(97, 5, 0),
                },
                new DimensionOrdinate
                {
                    FeatureLocation = new XYZ(120, 7.5, 0),
                    LeaderEndpoint = new XYZ(120, 12, 0),
                    DefinitionPoint = new XYZ(0, 0, 0),
                    IsOrdinateTypeX = true,
                    TextMiddlePoint = new XYZ(120, 13, 0),
                    // ACadSharp 3.7.1 formats an ordinate's measurement of 120 as
                    // "35°" (it runs it through the angular formatter), so the text
                    // is written out rather than left to UpdateBlock. Recorded in
                    // fixtures/README.md as a finding, not worked around quietly.
                    Text = "120",
                },
            };

            foreach (Dimension dim in dims)
            {
                dim.Layer = layer;
                doc.Entities.Add(dim);
                dim.UpdateBlock();
            }

            // UpdateBlock draws nothing for the angular kinds in 3.7.1: no block at
            // all for the two-line form, an empty one for the three-point form. A
            // file from AutoCAD carries the text, so the block's text is authored
            // here, which is the same bytes the writer would have produced for it.
            AuthorText(doc, dims[4], "90°");
            AuthorText(doc, dims[5], "60°");
        });
}
