namespace ACadSharp.Reference.Tests;

/// <summary>
/// The oracle's independence, asserted from inside the .NET build rather than
/// only from the Rust suite.
/// </summary>
/// <remarks>
/// The same claims are checked in `tests/expected_outputs.rs`, which runs on
/// every PR whether or not the .NET job does. Both, on purpose: this one fails
/// while somebody is editing the csproj, which is when it is cheapest to fix.
/// </remarks>
public sealed class IndependenceTests
{
    [Fact]
    public void TheOracleReferencesExactlyOnePackageAndNoProject()
    {
        string csproj = File.ReadAllText(RepoLayout.OracleProjectPath);

        int packages = csproj.Split("<PackageReference").Length - 1;
        Assert.True(
            packages == 1,
            $"the oracle's csproj has {packages} PackageReference entries. It may have exactly "
            + "one, ACadSharp: a second is how the independent oracle stops being independent.");
        Assert.Contains("Include=\"ACadSharp\"", csproj, StringComparison.Ordinal);
        Assert.DoesNotContain("<ProjectReference", csproj, StringComparison.Ordinal);
    }

    [Fact]
    public void TheOracleRestoresFromALockFile()
    {
        string csproj = File.ReadAllText(RepoLayout.OracleProjectPath);
        Assert.Contains(
            "<RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>",
            csproj,
            StringComparison.Ordinal);

        string lockPath = Path.Combine(
            Path.GetDirectoryName(RepoLayout.OracleProjectPath)!, "packages.lock.json");
        Assert.True(File.Exists(lockPath), $"{lockPath} is missing, so nothing is pinned");
        Assert.Contains("ACadSharp", File.ReadAllText(lockPath), StringComparison.Ordinal);
    }

    [Fact]
    public void TheOracleIsNotCompiledAheadOfTime()
    {
        // The distinction the whole design rests on: JIT ACadSharp is the
        // reference, NativeAOT plus the C ABI plus Rust is the system under
        // test. If both went through the same normalization path the
        // differential would compare a bug against itself and pass.
        string csproj = File.ReadAllText(RepoLayout.OracleProjectPath);
        Assert.Contains("<PublishAot>false</PublishAot>", csproj, StringComparison.Ordinal);
    }

    [Fact]
    public void TheOracleSetsInvariantGlobalizationForTheWholeProcess()
    {
        // Belt and braces for section 13. Every call site also passes
        // InvariantCulture, but a missed one would format a double with a comma
        // on somebody's machine and nowhere near CI.
        string csproj = File.ReadAllText(RepoLayout.OracleProjectPath);
        Assert.Contains(
            "<InvariantGlobalization>true</InvariantGlobalization>",
            csproj,
            StringComparison.Ordinal);
    }

    [Fact]
    public void NoOracleSourceNamesTheProductionStack()
    {
        string[] forbidden = ["Viprs.", "viprs_acad_", "PMTiles", "MapLibre"];
        string directory = Path.GetDirectoryName(RepoLayout.OracleProjectPath)!;

        var offences = new List<string>();
        int scanned = 0;
        foreach (string path in Directory.EnumerateFiles(directory, "*.cs", SearchOption.AllDirectories))
        {
            if (path.Contains($"{Path.DirectorySeparatorChar}obj{Path.DirectorySeparatorChar}",
                    StringComparison.Ordinal)
                || path.Contains($"{Path.DirectorySeparatorChar}bin{Path.DirectorySeparatorChar}",
                    StringComparison.Ordinal))
            {
                continue;
            }

            scanned++;
            foreach (string line in File.ReadAllLines(path))
            {
                // Comments may name what the oracle is independent of; code may not.
                string code = line.Split("//")[0];
                foreach (string needle in forbidden)
                {
                    if (code.Contains(needle, StringComparison.Ordinal))
                    {
                        offences.Add($"{Path.GetFileName(path)}: {needle}");
                    }
                }
            }
        }

        Assert.True(scanned >= 10, $"only scanned {scanned} source files, which is too few");
        Assert.Empty(offences);
    }

    [Fact]
    public void TheCorpusIsTheSevenVersionsTheManifestClaims()
    {
        IReadOnlyList<(string Id, string Path)> fixtures = RepoLayout.Fixtures();
        Assert.Equal(7, fixtures.Count);
        Assert.All(fixtures, fixture =>
            Assert.True(File.Exists(fixture.Path), $"{fixture.Id}: {fixture.Path} is missing"));
    }
}
