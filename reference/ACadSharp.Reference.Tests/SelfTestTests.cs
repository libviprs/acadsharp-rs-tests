using System.Text;

using ACadSharp.Entities;
using ACadSharp.IO;
using ACadSharp.Reference.Cad;
using ACadSharp.Reference.Canonical;

using CSMath;

namespace ACadSharp.Reference.Tests;

/// <summary>
/// The oracle's self-test: a DWG this project writes, read back and compared
/// against a committed expectation.
/// </summary>
/// <remarks>
/// <para>
/// The corpus tests prove the oracle is consistent with itself. This one proves
/// it is consistent with a file on disk that somebody read once and agreed with,
/// which is a different claim and the one that catches a change in ACadSharp's
/// reader rather than in this code.
/// </para>
/// <para>
/// It needs no corpus, which is why the CI job that runs it does not need the
/// DWG fixtures laid down first.
/// </para>
/// </remarks>
public sealed class SelfTestTests
{
    /// <summary>The radius the committed expectation was produced with.</summary>
    /// <remarks>
    /// Named so the mutation that proves this test reads the file it thinks it
    /// does has somewhere obvious to happen.
    /// </remarks>
    public const double ArcRadius = 25.0;

    [Fact]
    public void TheGeneratedDrawingMatchesTheCommittedExpectation()
    {
        byte[] produced = CanonicalWriter.Encode(RoundTrip(ArcRadius).Records);
        byte[] expected = File.ReadAllBytes(
            Path.Combine(RepoLayout.ExpectedDirectory, "selftest.reference.jsonl"));

        // Byte arrays rather than strings, so a failure says the length and the
        // first differing index instead of printing two screens of JSON.
        Assert.Equal(expected, produced);
    }

    [Fact]
    public void TheGeneratedDrawingsSvgMatchesTheCommittedExpectation()
    {
        byte[] produced = Reference.Svg.SvgWriter.Encode(RoundTrip(ArcRadius).Records);
        byte[] expected = File.ReadAllBytes(
            Path.Combine(RepoLayout.ExpectedDirectory, "selftest.reference.svg"));

        Assert.Equal(expected, produced);
    }

    [Fact]
    public void ChangingTheArcRadiusChangesTheCommittedExpectation()
    {
        // The check that the test above is reading the file it thinks it is. A
        // self-test whose expectation cannot be made to differ is a self-test
        // that would pass against a file full of anything at all.
        byte[] asCommitted = CanonicalWriter.Encode(RoundTrip(ArcRadius).Records);
        byte[] mutated = CanonicalWriter.Encode(RoundTrip(ArcRadius + 1.0).Records);

        Assert.NotEqual(asCommitted, mutated);
        Assert.Contains(
            "\"radius\":26.0", Encoding.UTF8.GetString(mutated), StringComparison.Ordinal);
    }

    [Fact]
    public void TheRoundTripSurvivesTheWriterAndTheReader()
    {
        ExtractionResult result = RoundTrip(ArcRadius);

        Assert.Single(result.Records.OfType<LineRecord>());
        Assert.Single(result.Records.OfType<ArcRecord>());
        Assert.Single(result.Records.OfType<TextRecord>());
        Assert.Equal(
            nameof(ACadVersion.AC1032),
            Assert.Single(result.Records.OfType<DocumentRecord>()).AcadVersion);
    }

    [Fact]
    public void WritingTheSameDrawingTwiceGivesTheSameCanonicalOutput()
    {
        // The DWG bytes themselves are not reproducible: the writer stamps the
        // header with a creation time. The canonical output has to be, and the
        // reason it is, is that nothing volatile is in the schema.
        Assert.Equal(
            CanonicalWriter.Encode(RoundTrip(ArcRadius).Records),
            CanonicalWriter.Encode(RoundTrip(ArcRadius).Records));
    }

    /// <summary>
    /// Builds the three-entity drawing, writes it as DWG, reads it back and extracts it.
    /// </summary>
    /// <param name="arcRadius">Radius of the arc, so a mutation has a handle.</param>
    /// <returns>The extraction.</returns>
    public static ExtractionResult RoundTrip(double arcRadius)
    {
        var document = new CadDocument(ACadVersion.AC1032);
        document.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(100, 0, 0)));
        document.Entities.Add(new Arc(new XYZ(50, 50, 0), arcRadius, 0.0, Math.PI / 2.0));
        document.Entities.Add(new TextEntity
        {
            Value = "ROOM 101",
            Height = 2.5,
            InsertPoint = new XYZ(20, 30, 0),
        });

        string path = Path.Combine(
            Path.GetTempPath(),
            $"acadsharp-reference-selftest-{Guid.NewGuid():N}.dwg");
        try
        {
            // Written outside the repository on purpose. A generated DWG inside
            // fixtures/ would be a corpus file with no manifest entry, and the
            // integrity test refuses one.
            DwgWriter.Write(path, document, null, (_, _) => { });

            Result<ExtractionResult> extracted = DocumentExtractor.Extract(path);
            Assert.True(extracted.IsOk, extracted.Error?.Message);
            return extracted.Value;
        }
        finally
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
    }
}
