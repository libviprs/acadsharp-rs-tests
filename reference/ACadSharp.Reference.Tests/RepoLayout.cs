using System.Runtime.CompilerServices;

namespace ACadSharp.Reference.Tests;

/// <summary>Finds the repository from wherever the test runner happens to be.</summary>
/// <remarks>
/// Walking up from this file's own compile-time path rather than from the
/// working directory. A test that resolved the corpus relative to the working
/// directory would pass or fail depending on how it was launched, and the
/// failure mode of guessing wrong here is a green suite that read nothing.
/// </remarks>
public static class RepoLayout
{
    /// <summary>The repository root.</summary>
    public static string Root { get; } = FindRoot();

    /// <summary>The DWG corpus directory.</summary>
    public static string FixtureDwgDirectory => Path.Combine(Root, "fixtures", "dwg");

    /// <summary>The canonical fixture manifest.</summary>
    public static string ManifestPath => Path.Combine(Root, "fixtures", "manifest.toml");

    /// <summary>The oracle's project file.</summary>
    public static string OracleProjectPath =>
        Path.Combine(Root, "reference", "ACadSharp.Reference", "ACadSharp.Reference.csproj");

    /// <summary>The committed expectations this test project carries, beside the assembly.</summary>
    public static string ExpectedDirectory => Path.Combine(AppContext.BaseDirectory, "Expected");

    private static string FindRoot([CallerFilePath] string thisFile = "")
    {
        DirectoryInfo? directory = new FileInfo(thisFile).Directory;
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "fixtures", "manifest.toml")))
            {
                return directory.FullName;
            }

            directory = directory.Parent;
        }

        throw new InvalidOperationException(
            $"walked up from {thisFile} without finding fixtures/manifest.toml");
    }

    /// <summary>Every fixture DWG, by id, in manifest order.</summary>
    /// <returns>Ids paired with absolute paths.</returns>
    public static IReadOnlyList<(string Id, string Path)> Fixtures()
    {
        Result<IReadOnlyList<FixtureEntry>> rows = FixtureManifest.Load(ManifestPath);
        if (!rows.IsOk)
        {
            throw new InvalidOperationException(rows.Error!.Message);
        }

        return [.. rows.Value.Select(row => (
            row.Id,
            Path.Combine(Root, "fixtures", row.File.Replace('/', Path.DirectorySeparatorChar))))];
    }
}

/// <summary>The seven corpus fixtures, as an xUnit theory source.</summary>
/// <remarks>
/// One theory over the corpus rather than seven copies of the same test: a copy
/// is a place for a version to go missing, and the count assertion below is what
/// notices if one does.
/// </remarks>
public sealed class CorpusFixtures : TheoryData<string, string>
{
    /// <summary>Loads the corpus from the manifest.</summary>
    public CorpusFixtures()
    {
        IReadOnlyList<(string Id, string Path)> fixtures = RepoLayout.Fixtures();
        if (fixtures.Count == 0)
        {
            throw new InvalidOperationException(
                "the manifest lists no fixtures, so every theory over it would pass vacuously");
        }

        foreach ((string id, string path) in fixtures)
        {
            Add(id, path);
        }
    }
}
