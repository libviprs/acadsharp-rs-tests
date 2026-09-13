using System.Globalization;
using System.Text;

using ACadSharp.Reference.Cad;
using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Tests;

/// <summary>
/// The property everything else rests on: the same input produces the same bytes.
/// </summary>
/// <remarks>
/// A hash in a manifest is only worth something if the thing being hashed is a
/// function of the input alone. These run the whole pipeline twice per fixture
/// and compare the byte arrays, so a failure reports the length and the first
/// differing index rather than "not equal".
/// </remarks>
public sealed class DeterminismTests
{
    [Theory]
    [ClassData(typeof(CorpusFixtures))]
    public void SameDwgGivesByteIdenticalJsonl(string id, string path)
    {
        byte[] first = Jsonl(path);
        byte[] second = Jsonl(path);
        Assert.Equal(first, second);
        Assert.True(first.Length > 0, $"{id}: the canonical output is empty");
    }

    [Theory]
    [ClassData(typeof(CorpusFixtures))]
    public void SameDwgGivesByteIdenticalSvg(string id, string path)
    {
        ExtractionResult first = Extract(path);
        ExtractionResult second = Extract(path);
        byte[] a = Reference.Svg.SvgWriter.Encode(first.Records);
        byte[] b = Reference.Svg.SvgWriter.Encode(second.Records);
        Assert.Equal(a, b);
        Assert.True(a.Length > 0, $"{id}: the SVG is empty");
    }

    [Theory]
    [ClassData(typeof(CorpusFixtures))]
    public void SameDwgGivesTheSameRecordCount(string id, string path)
    {
        ExtractionResult first = Extract(path);
        ExtractionResult second = Extract(path);
        Assert.Equal(first.Records.Count, second.Records.Count);

        IReadOnlyDictionary<string, int> a = CanonicalWriter.CountByRecordName(first.Records);
        IReadOnlyDictionary<string, int> b = CanonicalWriter.CountByRecordName(second.Records);
        Assert.Equal(CanonicalWriter.FormatCounts(a), CanonicalWriter.FormatCounts(b));

        // A stream of nothing but warnings would satisfy the equality above and
        // be useless as a reference.
        Assert.True(
            a.ContainsKey("line") || a.ContainsKey("polyline") || a.ContainsKey("circle"),
            $"{id}: no geometry records at all, so this comparison proves nothing");
    }

    [Theory]
    [ClassData(typeof(CorpusFixtures))]
    public void AGermanLocaleChangesNothing(string id, string path)
    {
        CultureInfo original = CultureInfo.CurrentCulture;
        try
        {
            var german = new CultureInfo("de-DE");

            // The positive control for this test. If the runtime has no ICU, or
            // the test process is globalization-invariant, this culture formats
            // exactly like the invariant one and the comparison below could not
            // fail however broken the formatting was.
            Assert.Equal("1,5", 1.5.ToString(german));

            byte[] invariant = Jsonl(path);
            CultureInfo.CurrentCulture = german;
            CultureInfo.CurrentUICulture = german;
            byte[] localized = Jsonl(path);

            Assert.Equal(invariant, localized);
            Assert.DoesNotContain(
                "1,5", Encoding.UTF8.GetString(localized), StringComparison.Ordinal);
            Assert.True(invariant.Length > 0, $"{id}: nothing was produced");
        }
        finally
        {
            CultureInfo.CurrentCulture = original;
            CultureInfo.CurrentUICulture = original;
        }
    }

    [Theory]
    [ClassData(typeof(CorpusFixtures))]
    public void TheCanonicalFileHasTheShapeTheSchemaPromises(string id, string path)
    {
        string text = Encoding.UTF8.GetString(Jsonl(path));

        Assert.DoesNotContain('\r', text);
        Assert.EndsWith("\n", text, StringComparison.Ordinal);
        Assert.False(text.Contains("\n\n", StringComparison.Ordinal), $"{id}: blank line in the stream");

        string[] lines = text.Split('\n', StringSplitOptions.RemoveEmptyEntries);
        Assert.All(lines, line =>
        {
            Assert.StartsWith("{\"schema\":1,\"record\":\"", line, StringComparison.Ordinal);
            Assert.EndsWith("}", line, StringComparison.Ordinal);
        });
        Assert.True(lines.Length > 10, $"{id}: only {lines.Length} records");
    }

    private static ExtractionResult Extract(string path)
    {
        Result<ExtractionResult> result = DocumentExtractor.Extract(path);
        Assert.True(result.IsOk, result.Error?.Message);
        return result.Value;
    }

    private static byte[] Jsonl(string path) => CanonicalWriter.Encode(Extract(path).Records);
}
