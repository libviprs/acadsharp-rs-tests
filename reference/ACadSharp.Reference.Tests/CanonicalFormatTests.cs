using System.Globalization;
using System.Text;

using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Tests;

/// <summary>The formatting rules, one test each.</summary>
/// <remarks>
/// These are the rules a future Rust side has to reproduce exactly, so each one
/// is pinned to a literal here rather than to whatever the current
/// implementation happens to emit.
/// </remarks>
public sealed class CanonicalFormatTests
{
    [Theory]
    [InlineData(0.0, "0.0")]
    [InlineData(1.0, "1.0")]
    [InlineData(100.0, "100.0")]
    [InlineData(-100.0, "-100.0")]
    [InlineData(1.5, "1.5")]
    [InlineData(0.1, "0.1")]
    public void DoublesPrintTheirShortestRoundTrip(double value, string expected)
    {
        Assert.Equal(expected, NumericFormatting.Real(value));
    }

    [Fact]
    public void HalfPiPrintsEverySignificantDigit()
    {
        Assert.Equal("1.5707963267948966", NumericFormatting.Real(Math.PI / 2.0));
    }

    [Fact]
    public void EveryNumberIsVisiblyADouble()
    {
        // A JSON parser does not care, but the file's bytes are hashed, so the
        // integer-valued case has to pick one spelling and keep it.
        Assert.Equal("7.0", NumericFormatting.Real(7));
        Assert.Equal("-0.5", NumericFormatting.Real(-0.5));
    }

    [Fact]
    public void NegativeZeroIsNormalised()
    {
        // Two implementations can both round-trip IEEE-754 correctly and still
        // disagree on the sign of a zero produced by the same geometry.
        Assert.Equal(NumericFormatting.Real(0.0), NumericFormatting.Real(-0.0));
        Assert.DoesNotContain("-", NumericFormatting.Real(-0.0), StringComparison.Ordinal);
    }

    [Theory]
    [InlineData(double.NaN)]
    [InlineData(double.PositiveInfinity)]
    [InlineData(double.NegativeInfinity)]
    public void NonFiniteValuesAreRefusedRatherThanEncoded(double value)
    {
        // Encoding any of these would produce a file that is not JSON. A NaN
        // coordinate is a real finding about the reader, so the export stops.
        NonFiniteValueException thrown =
            Assert.Throws<NonFiniteValueException>(() => NumericFormatting.Real(value));
        Assert.Contains("non-finite", thrown.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Theory]
    [InlineData(1e300)]
    [InlineData(-1e300)]
    [InlineData(1e-300)]
    [InlineData(-1e-300)]
    [InlineData(1.7976931348623157e308)]
    [InlineData(-1234567.891011)]
    public void ExtremeMagnitudesStillRoundTrip(double value)
    {
        string text = NumericFormatting.Real(value);
        Assert.Equal(value, double.Parse(text, CultureInfo.InvariantCulture));
    }

    [Fact]
    public void TheSmallestSubnormalStillRoundTrips()
    {
        double value = double.Epsilon;
        Assert.Equal(
            value,
            double.Parse(NumericFormatting.Real(value), CultureInfo.InvariantCulture));
    }

    [Fact]
    public void AGermanLocaleCannotReachTheNumbers()
    {
        CultureInfo original = CultureInfo.CurrentCulture;
        try
        {
            var german = new CultureInfo("de-DE");

            // The positive control. Without ICU, or in a globalization-invariant
            // process, this culture formats exactly like the invariant one and
            // the two assertions below could not fail however broken the
            // formatting was.
            Assert.Equal("1,5", 1.5.ToString(german));

            CultureInfo.CurrentCulture = german;
            Assert.Equal("1.5", NumericFormatting.Real(1.5));
            Assert.Equal("1234.5", NumericFormatting.Real(1234.5));
        }
        finally
        {
            CultureInfo.CurrentCulture = original;
        }
    }

    [Fact]
    public void UnicodeSurvivesExactly()
    {
        // System.Text.Json's default encoder would turn most of these into
        // \uXXXX escapes. Section 11 says the text has to survive, so the
        // escaping is hand-written and only touches what JSON forbids bare.
        const string text = "Räume 日本語 Ω ✓ 📐 наклон";
        string quoted = NumericFormatting.Quoted(text);
        Assert.Equal('"' + text + '"', quoted);
        Assert.DoesNotContain("\\u", quoted, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("plain", "\"plain\"")]
    [InlineData("with \"quotes\"", "\"with \\\"quotes\\\"\"")]
    [InlineData("back\\slash", "\"back\\\\slash\"")]
    [InlineData("line\nbreak", "\"line\\nbreak\"")]
    [InlineData("tab\there", "\"tab\\there\"")]
    [InlineData("bell", "\"bell\\u0007\"")]
    public void EscapingIsExactlyTheJsonMinimum(string input, string expected)
    {
        Assert.Equal(expected, NumericFormatting.Quoted(input));
    }

    [Theory]
    [InlineData("a\r\nb", "a\nb")]
    [InlineData("a\rb", "a\nb")]
    [InlineData("a\nb", "a\nb")]
    [InlineData("a\r\n\r\nb", "a\n\nb")]
    [InlineData(null, "")]
    public void LineEndingsAreNormalisedToLf(string? input, string expected)
    {
        // The same drawing written on Windows and elsewhere must not differ only
        // in the line endings inside an MTEXT run.
        Assert.Equal(expected, NumericFormatting.NormalizeLineEndings(input));
    }

    [Fact]
    public void RecordsCarryTheirFieldsInTheDocumentedOrder()
    {
        Assert.Equal(
            "{\"schema\":1,\"record\":\"document\",\"format\":\"dwg\",\"acad_version\":\"AC1032\"," +
            "\"maintenance_version\":228}",
            CanonicalWriter.EncodeLine(new DocumentRecord("dwg", "AC1032", 228)));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"view\",\"index\":0,\"name\":\"Model\",\"kind\":\"model\"}",
            CanonicalWriter.EncodeLine(new ViewRecord(0, "Model", ViewKind.Model)));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"line\",\"view\":0,\"x1\":0.0,\"y1\":0.0,\"z1\":0.0," +
            "\"x2\":100.0,\"y2\":0.0,\"z2\":0.0}",
            CanonicalWriter.EncodeLine(
                new LineRecord(0, Point3.Zero, new Point3(100.0, 0.0, 0.0))));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"circle\",\"view\":1,\"cx\":10.0,\"cy\":20.0,\"cz\":0.0," +
            "\"radius\":5.0,\"nx\":0.0,\"ny\":0.0,\"nz\":1.0}",
            CanonicalWriter.EncodeLine(
                new CircleRecord(1, new Point3(10.0, 20.0, 0.0), 5.0, Point3.UnitZ)));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"arc\",\"view\":0,\"cx\":50.0,\"cy\":50.0,\"cz\":0.0," +
            "\"radius\":25.0,\"start\":0.0,\"end\":1.5707963267948966," +
            "\"nx\":0.0,\"ny\":0.0,\"nz\":1.0}",
            CanonicalWriter.EncodeLine(new ArcRecord(
                0, new Point3(50.0, 50.0, 0.0), 25.0, 0.0, Math.PI / 2.0, Point3.UnitZ)));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"text\",\"view\":0,\"x\":20.0,\"y\":30.0,\"z\":0.0," +
            "\"height\":2.5,\"rotation\":0.0,\"width_factor\":1.0,\"halign\":\"left\"," +
            "\"valign\":\"baseline\",\"style\":\"Standard\",\"text\":\"ROOM 101\"}",
            CanonicalWriter.EncodeLine(new TextRecord(
                0, new Point3(20.0, 30.0, 0.0), 2.5, 0.0, 1.0,
                "left", "baseline", "Standard", "ROOM 101")));

        Assert.Equal(
            "{\"schema\":1,\"record\":\"warning\",\"category\":\"unsupported_entity\"," +
            "\"entity_type\":\"MultiLeader\",\"detail\":\"no_canonical_mapping\"}",
            CanonicalWriter.EncodeLine(new WarningRecord(
                WarningCategory.UnsupportedEntity, "MultiLeader", "no_canonical_mapping")));
    }

    [Fact]
    public void ThePolylineFieldSetNeverDependsOnItsContents()
    {
        // Always a bulge per vertex, even when every one of them is zero. A
        // field that appears only sometimes is a field a reader has to guess at.
        string line = CanonicalWriter.EncodeLine(new PolylineRecord(
            0,
            Closed: true,
            Point3.UnitZ,
            [new Point3(0.0, 0.0, 0.0), new Point3(1.0, 0.0, 0.0)],
            [0.0, 0.0]));
        Assert.Contains("\"bulges\":[0.0,0.0]", line, StringComparison.Ordinal);
        Assert.Contains("\"closed\":true", line, StringComparison.Ordinal);
    }

    [Fact]
    public void TheStreamIsUtf8WithNoBomAndAFinalNewline()
    {
        byte[] bytes = CanonicalWriter.Encode([
            new DocumentRecord("dwg", "AC1032", 0),
            new ViewRecord(0, "Modèle", ViewKind.Model),
        ]);

        Assert.NotEqual(0xEF, bytes[0]);
        Assert.Equal((byte)'\n', bytes[^1]);
        string text = Encoding.UTF8.GetString(bytes);
        Assert.Equal(2, text.Split('\n', StringSplitOptions.RemoveEmptyEntries).Length);
        Assert.Contains("Modèle", text, StringComparison.Ordinal);
    }

    [Fact]
    public void WarningsSortByADocumentedKeyRatherThanArrival()
    {
        WarningRecord[] arrived =
        [
            new(WarningCategory.UnsupportedEntity, "Zebra", "b"),
            new(WarningCategory.MissingReference, "Alpha", "a"),
            new(WarningCategory.UnsupportedEntity, "Alpha", "b"),
            new(WarningCategory.UnsupportedEntity, "Alpha", "a"),
        ];

        IReadOnlyList<WarningRecord> sorted = CanonicalWriter.SortWarnings(arrived);

        Assert.Equal(
            ["missing_reference/Alpha/a", "unsupported_entity/Alpha/a",
             "unsupported_entity/Alpha/b", "unsupported_entity/Zebra/b"],
            sorted.Select(w =>
                $"{CanonicalWriter.CategoryName(w.Category)}/{w.EntityType}/{w.Detail}"));

        // Duplicates are kept, so the record count still equals the number of
        // things that went wrong.
        Assert.Equal(arrived.Length, sorted.Count);
    }
}
