using ACadSharp.Reference.Cad;

namespace ACadSharp.Reference.Tests;

/// <summary>
/// A rewrite is a function of the source, the version and the container. The
/// round-trip matrix stores nothing for its second stage and judges whatever the
/// writer made, so two writes of one output that differ are a failed run.
/// </summary>
public sealed class RewriteDeterminismTests
{
    // The older saves do not store the times the writer stamps, so the header
    // constructor's clock reaches the output unless the rewrite pins it (#41).
    [Theory]
    [InlineData("acadsharp-ac1014", "AC1018")]
    [InlineData("acadsharp-ac1015", "AC1024")]
    [InlineData("acadsharp-ac1032", "AC1032")]
    public void TwoDwgRewritesOfOneSourceAreByteIdentical(string id, string version)
    {
        string path = RepoLayout.Fixtures().Single(f => f.Id == id).Path;
        string dir = Directory.CreateTempSubdirectory().FullName;
        try
        {
            byte[][] written = new byte[2][];
            for (int i = 0; i < 2; i++)
            {
                string output = Path.Combine(dir, $"out{i}.dwg");
                Assert.Null(Rewriter.Rewrite(path, output, "dwg", version));
                written[i] = File.ReadAllBytes(output);
                Thread.Sleep(1100);
            }

            Assert.Equal(written[0], written[1]);
        }
        finally
        {
            Directory.Delete(dir, recursive: true);
        }
    }
}
