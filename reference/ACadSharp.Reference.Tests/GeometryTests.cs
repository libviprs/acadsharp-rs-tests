using ACadSharp.Entities;
using ACadSharp.Reference.Cad;
using ACadSharp.Reference.Canonical;
using ACadSharp.Tables;

using CSMath;

namespace ACadSharp.Reference.Tests;

/// <summary>
/// What the extractor does to geometry, driven by documents built in memory.
/// </summary>
/// <remarks>
/// In memory rather than through a DWG round trip, because these are assertions
/// about the extractor and a round trip would put ACadSharp's writer between the
/// input and the thing under test. The round trip has its own test.
/// </remarks>
public sealed class GeometryTests
{
    private const double Tolerance = 1e-9;

    [Fact]
    public void NestedBlockTransformsCompose()
    {
        // inner:  a unit line along X
        // outer:  inner inserted at (10, 0) scaled by two
        // model:  outer inserted at (0, 5) turned a quarter turn
        //
        // So the line's ends are (10, 0) and (12, 0) in outer space, and
        // (0, 15) and (0, 17) in the world. Blocks are the case where a decoder
        // can be wrong and still draw something plausible, which is why this is
        // checked against arithmetic done by hand rather than against a
        // recorded output.
        CadDocument document = NewDocument();

        var inner = new BlockRecord("inner");
        inner.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(1, 0, 0)));
        document.BlockRecords.Add(inner);

        var outer = new BlockRecord("outer");
        outer.Entities.Add(new Insert(inner)
        {
            InsertPoint = new XYZ(10, 0, 0),
            XScale = 2,
            YScale = 2,
            ZScale = 2,
        });
        document.BlockRecords.Add(outer);

        document.Entities.Add(new Insert(outer)
        {
            InsertPoint = new XYZ(0, 5, 0),
            Rotation = Math.PI / 2.0,
        });

        LineRecord line = Assert.Single(Records(document).OfType<LineRecord>());
        AssertPoint(new Point3(0, 15, 0), line.Start);
        AssertPoint(new Point3(0, 17, 0), line.End);
    }

    [Fact]
    public void RotatingABlockShiftsItsArcAnglesAndLeavesTheRadius()
    {
        CadDocument document = NewDocument();
        var block = new BlockRecord("arcs");
        block.Entities.Add(new Arc(new XYZ(0, 0, 0), 5.0, 0.0, Math.PI / 2.0));
        document.BlockRecords.Add(block);
        document.Entities.Add(new Insert(block) { Rotation = Math.PI / 2.0 });

        ArcRecord arc = Assert.Single(Records(document).OfType<ArcRecord>());
        Assert.Equal(5.0, arc.Radius, Tolerance);
        Assert.Equal(Math.PI / 2.0, arc.Start, Tolerance);
        Assert.Equal(Math.PI, arc.End, Tolerance);
        AssertPoint(Point3.UnitZ, arc.Normal);
    }

    [Fact]
    public void AUniformScaleKeepsACircleACircle()
    {
        CadDocument document = NewDocument();
        var block = new BlockRecord("circles");
        block.Entities.Add(new Circle { Center = new XYZ(1, 2, 0), Radius = 2.0 });
        document.BlockRecords.Add(block);
        document.Entities.Add(new Insert(block) { XScale = 3, YScale = 3, ZScale = 3 });

        CircleRecord circle = Assert.Single(Records(document).OfType<CircleRecord>());
        Assert.Equal(6.0, circle.Radius, Tolerance);
        AssertPoint(new Point3(3, 6, 0), circle.Center);
        Assert.Empty(Records(document).OfType<EllipseRecord>());
    }

    [Fact]
    public void ANonUniformScaleTurnsACircleIntoAnEllipseRatherThanALie()
    {
        // Keeping a "circle" with one of the two radii would be a record the
        // differential would then have to agree with.
        CadDocument document = NewDocument();
        var block = new BlockRecord("squashed");
        block.Entities.Add(new Circle { Center = XYZ.Zero, Radius = 1.0 });
        document.BlockRecords.Add(block);
        document.Entities.Add(new Insert(block) { XScale = 2, YScale = 1, ZScale = 1 });

        IReadOnlyList<CanonicalRecord> records = Records(document);
        Assert.Empty(records.OfType<CircleRecord>());
        EllipseRecord ellipse = Assert.Single(records.OfType<EllipseRecord>());
        Assert.Equal(2.0, Geometry.Length(ellipse.MajorAxis), Tolerance);
        Assert.Equal(0.5, ellipse.Ratio, Tolerance);
        Assert.Equal(0.0, ellipse.Start, Tolerance);
        Assert.Equal(2.0 * Math.PI, ellipse.End, Tolerance);
    }

    [Fact]
    public void ACurveIsNeverTessellatedIntoAPolyline()
    {
        CadDocument document = NewDocument();
        document.Entities.Add(new Circle { Center = XYZ.Zero, Radius = 10.0 });
        document.Entities.Add(new Arc(XYZ.Zero, 4.0, 0.1, 2.0));
        document.Entities.Add(new Ellipse
        {
            Center = XYZ.Zero,
            MajorAxisEndPoint = new XYZ(4, 0, 0),
            RadiusRatio = 0.25,
            StartParameter = 0.0,
            EndParameter = Math.PI,
        });
        var spline = new Spline { Degree = 3 };
        spline.ControlPoints.AddRange([
            new XYZ(0, 0, 0), new XYZ(1, 2, 0), new XYZ(3, -1, 0), new XYZ(4, 0, 0)]);
        spline.Knots.AddRange([0, 0, 0, 0, 1, 1, 1, 1]);
        document.Entities.Add(spline);

        IReadOnlyList<CanonicalRecord> records = Records(document);
        Assert.Single(records.OfType<CircleRecord>());
        Assert.Single(records.OfType<ArcRecord>());
        Assert.Single(records.OfType<EllipseRecord>());
        Assert.Empty(records.OfType<PolylineRecord>());

        SplineRecord result = Assert.Single(records.OfType<SplineRecord>());
        Assert.Equal(3, result.Degree);
        Assert.Equal(4, result.ControlPoints.Count);
        Assert.Equal(8, result.Knots.Count);
    }

    [Fact]
    public void ASelfReferencingBlockWarnsInsteadOfHanging()
    {
        // The cycle is closed last, for the reason spelled out in the mutual
        // reference test below: ACadSharp 3.7.1 recurses forever cloning an
        // already-cyclic block, and that happens during setup rather than in
        // anything under test here.
        CadDocument document = NewDocument();
        var block = new BlockRecord("ouroboros");
        block.Entities.Add(new Line(XYZ.Zero, new XYZ(1, 0, 0)));
        document.BlockRecords.Add(block);
        document.Entities.Add(new Insert(block));
        block.Entities.Add(new Insert(block));

        IReadOnlyList<CanonicalRecord> records = Records(document);

        // The line the block does carry is still drawn once; only the cycle is
        // refused. Dropping the whole block would be a different kind of wrong.
        Assert.Single(records.OfType<LineRecord>());
        WarningRecord warning = Assert.Single(
            records.OfType<WarningRecord>(), w => w.Detail == "cyclic_block_reference");
        Assert.Equal(WarningCategory.MalformedGeometry, warning.Category);
    }

    [Fact]
    public void TwoBlocksThatReferenceEachOtherWarnInsteadOfHanging()
    {
        // Built in this order deliberately. ACadSharp 3.7.1 stack-overflows in
        // BlockRecord.Clone when an Insert of an already-cyclic block chain is
        // added to a document, which happens while the test is still setting up
        // and long before the extractor sees anything. Adding the model-space
        // insert first and closing the cycle afterwards produces the same
        // document without going through that path.
        CadDocument document = NewDocument();
        var first = new BlockRecord("ping");
        var second = new BlockRecord("pong");
        document.BlockRecords.Add(first);
        document.BlockRecords.Add(second);
        document.Entities.Add(new Insert(first));
        first.Entities.Add(new Insert(second));
        second.Entities.Add(new Insert(first));
        second.Entities.Add(new Line(XYZ.Zero, new XYZ(1, 1, 0)));

        IReadOnlyList<CanonicalRecord> records = Records(document);
        Assert.Single(records.OfType<LineRecord>());
        Assert.Contains(
            records.OfType<WarningRecord>(), w => w.Detail == "cyclic_block_reference");
    }

    [Fact]
    public void NestingDeeperThanTheLimitWarnsAndStops()
    {
        // Distinct blocks, so the cycle check cannot be what stops it. Without
        // that the depth limit would look like it worked while never running.
        CadDocument document = NewDocument();
        const int depth = 6;
        BlockRecord? previous = null;
        for (int level = 0; level < depth; level++)
        {
            var block = new BlockRecord($"level{level}");
            document.BlockRecords.Add(block);
            if (previous is not null)
            {
                block.Entities.Add(new Insert(previous));
            }
            else
            {
                block.Entities.Add(new Line(XYZ.Zero, new XYZ(1, 0, 0)));
            }

            previous = block;
        }

        document.Entities.Add(new Insert(previous!));

        // Generous limit: everything is drawn.
        Assert.Single(Records(document, maxDepth: 16).OfType<LineRecord>());

        // Tight limit: the innermost line is never reached, and that is said.
        IReadOnlyList<CanonicalRecord> shallow = Records(document, maxDepth: 3);
        Assert.Empty(shallow.OfType<LineRecord>());
        Assert.Contains(
            shallow.OfType<WarningRecord>(),
            w => w.Detail.StartsWith("recursion_limit_", StringComparison.Ordinal));
    }

    [Fact]
    public void UnicodeTextSurvivesTheWholeExtraction()
    {
        const string value = "Размер ±2,5 µm — 日本語 \U0001F4D0";
        CadDocument document = NewDocument();
        document.Entities.Add(new TextEntity
        {
            Value = value,
            Height = 2.5,
            InsertPoint = new XYZ(1, 2, 0),
        });

        TextRecord text = Assert.Single(Records(document).OfType<TextRecord>());
        Assert.Equal(value, text.Text);
    }

    [Fact]
    public void MultiLineTextComesOutWithLfOnly()
    {
        CadDocument document = NewDocument();
        document.Entities.Add(new MText { Value = "first\r\nsecond\rthird", Height = 1.0 });

        TextRecord text = Assert.Single(Records(document).OfType<TextRecord>());
        Assert.Equal("first\nsecond\nthird", text.Text);
    }

    [Theory]
    [InlineData(-1234.5678, -8765.4321)]
    [InlineData(1.0e12, -9.9e11)]
    [InlineData(1.0e-9, -2.5e-9)]
    [InlineData(0.0, -0.0)]
    public void ExtremeCoordinatesSurviveUnrounded(double x, double y)
    {
        // Section 13: no arbitrary rounding. The comparator handles tolerance;
        // the reference carries every digit the source had.
        CadDocument document = NewDocument();
        document.Entities.Add(new Line(new XYZ(x, y, 0), new XYZ(-x, -y, 0)));

        LineRecord line = Assert.Single(Records(document).OfType<LineRecord>());
        Assert.Equal(x, line.Start.X);
        Assert.Equal(y, line.Start.Y);

        string encoded = CanonicalWriter.EncodeLine(line);
        Assert.DoesNotContain("NaN", encoded, StringComparison.Ordinal);
        Assert.DoesNotContain("Infinity", encoded, StringComparison.Ordinal);
    }

    [Fact]
    public void ASolidComesOutInPerimeterOrderRatherThanStorageOrder()
    {
        // DXF stores a SOLID's corners 1, 2, 4, 3. Drawing them in stored order
        // gives a bow tie, which still renders and still hashes.
        CadDocument document = NewDocument();
        document.Entities.Add(new Solid
        {
            FirstCorner = new XYZ(0, 0, 0),
            SecondCorner = new XYZ(1, 0, 0),
            ThirdCorner = new XYZ(0, 1, 0),
            FourthCorner = new XYZ(1, 1, 0),
        });

        PolygonRecord polygon = Assert.Single(Records(document).OfType<PolygonRecord>());
        Assert.Equal(4, polygon.Points.Count);
        AssertPoint(new Point3(0, 0, 0), polygon.Points[0]);
        AssertPoint(new Point3(1, 0, 0), polygon.Points[1]);
        AssertPoint(new Point3(1, 1, 0), polygon.Points[2]);
        AssertPoint(new Point3(0, 1, 0), polygon.Points[3]);
    }

    [Fact]
    public void AnEntityWithNoCanonicalFormBecomesAWarningRatherThanNothing()
    {
        CadDocument document = NewDocument();
        document.Entities.Add(new XLine());
        document.Entities.Add(new Line(XYZ.Zero, new XYZ(1, 0, 0)));

        IReadOnlyList<CanonicalRecord> records = Records(document);
        Assert.Single(records.OfType<LineRecord>());
        WarningRecord warning = Assert.Single(
            records.OfType<WarningRecord>(), w => w.EntityType == nameof(XLine));
        Assert.Equal(WarningCategory.UnsupportedEntity, warning.Category);
        Assert.Equal("no_canonical_mapping", warning.Detail);
    }

    [Fact]
    public void AnInvisibleEntityIsNotDrawn()
    {
        // Absence is the correct representation of something a viewer does not
        // draw, so this one is deliberately not a warning.
        CadDocument document = NewDocument();
        document.Entities.Add(new Line(XYZ.Zero, new XYZ(1, 0, 0)) { IsInvisible = true });
        document.Entities.Add(new Line(XYZ.Zero, new XYZ(0, 1, 0)));

        Assert.Single(Records(document).OfType<LineRecord>());
    }

    [Fact]
    public void TextUsesTheAlignmentPointWhenItIsNotLeftBaseline()
    {
        CadDocument document = NewDocument();
        document.Entities.Add(new TextEntity
        {
            Value = "left baseline",
            Height = 1.0,
            InsertPoint = new XYZ(1, 1, 0),
            AlignmentPoint = new XYZ(9, 9, 0),
        });
        document.Entities.Add(new TextEntity
        {
            Value = "centred",
            Height = 1.0,
            InsertPoint = new XYZ(1, 1, 0),
            AlignmentPoint = new XYZ(9, 9, 0),
            HorizontalAlignment = TextHorizontalAlignment.Center,
        });

        List<TextRecord> text = [.. Records(document).OfType<TextRecord>()];
        Assert.Equal(2, text.Count);
        AssertPoint(new Point3(1, 1, 0), text[0].Position);
        Assert.Equal("left", text[0].HorizontalAlignment);
        AssertPoint(new Point3(9, 9, 0), text[1].Position);
        Assert.Equal("center", text[1].HorizontalAlignment);
    }

    [Fact]
    public void ViewsComeOutInTabOrderRatherThanAlphabetically()
    {
        // ACadSharp's layout collection is keyed by name and enumerates
        // alphabetically, which would put Layout1 and Layout2 in front of Model.
        CadDocument document = NewDocument();
        List<ViewRecord> views = [.. Records(document).OfType<ViewRecord>()];

        Assert.NotEmpty(views);
        Assert.Equal(ViewKind.Model, views[0].Kind);
        Assert.Equal(0, views[0].Index);
        for (int i = 0; i < views.Count; i++)
        {
            Assert.Equal(i, views[i].Index);
        }
    }

    [Fact]
    public void TheDocumentRecordCarriesNothingVolatile()
    {
        CadDocument document = NewDocument();
        DocumentRecord record = Assert.Single(Records(document).OfType<DocumentRecord>());

        Assert.Equal("dwg", record.Format);
        string encoded = CanonicalWriter.EncodeLine(record);
        foreach (string forbidden in new[] { "/", "\\", Environment.MachineName })
        {
            Assert.DoesNotContain(forbidden, encoded, StringComparison.Ordinal);
        }
    }

    private static CadDocument NewDocument() => new(ACadVersion.AC1032);

    private static IReadOnlyList<CanonicalRecord> Records(
        CadDocument document, int maxDepth = BlockResolver.DefaultMaxDepth)
    {
        return DocumentExtractor
            .ExtractDocument(document, new WarningCollector(), maxDepth)
            .Records;
    }

    private static void AssertPoint(Point3 expected, Point3 actual)
    {
        Assert.Equal(expected.X, actual.X, Tolerance);
        Assert.Equal(expected.Y, actual.Y, Tolerance);
        Assert.Equal(expected.Z, actual.Z, Tolerance);
    }
}
