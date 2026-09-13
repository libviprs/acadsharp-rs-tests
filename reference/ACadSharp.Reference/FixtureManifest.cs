using System.Globalization;

namespace ACadSharp.Reference;

/// <summary>One row of <c>fixtures/manifest.toml</c>, as far as this tool cares.</summary>
/// <param name="Id">The fixture's stable identifier, which names its output files.</param>
/// <param name="File">Path to the DWG, relative to the manifest's own directory.</param>
/// <param name="AcadVersion">The version code the manifest claims, for a cross-check.</param>
public sealed record FixtureEntry(string Id, string File, string AcadVersion);

/// <summary>
/// Reads the fixture rows out of the canonical manifest.
/// </summary>
/// <remarks>
/// <para>
/// A deliberately small TOML subset: <c>[[fixture]]</c> tables with quoted
/// string and bare integer values, which is all the manifest contains and all
/// this tool needs. The alternative was a second NuGet package, and the
/// exporter's dependency list being exactly ACadSharp is a property a test
/// asserts. Duplicating a hundred lines of parsing on purpose is cheaper than
/// weakening that.
/// </para>
/// <para>
/// It fails rather than guesses. An unparsable line or a row missing a key it
/// needs is an error naming the line number, not a row quietly skipped: a
/// skipped row would make <c>--all</c> silently regenerate six of seven
/// fixtures.
/// </para>
/// </remarks>
public static class FixtureManifest
{
    /// <summary>Parses the fixture rows out of a manifest file.</summary>
    /// <param name="path">Path to <c>fixtures/manifest.toml</c>.</param>
    /// <returns>The rows in manifest order, or the reason there are none.</returns>
    public static Result<IReadOnlyList<FixtureEntry>> Load(string path)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);

        if (!File.Exists(path))
        {
            return Result.Fail<IReadOnlyList<FixtureEntry>>(
                ExitCode.Usage, $"{path}: no such manifest");
        }

        string[] lines;
        try
        {
            lines = File.ReadAllLines(path);
        }
        catch (IOException ex)
        {
            return Result.Fail<IReadOnlyList<FixtureEntry>>(
                ExitCode.Usage, $"{path}: cannot read manifest: {ex.Message}");
        }

        var rows = new List<FixtureEntry>();
        Dictionary<string, string>? current = null;
        int currentLine = 0;

        Result<IReadOnlyList<FixtureEntry>>? Flush()
        {
            if (current is null)
            {
                return null;
            }

            foreach (string key in new[] { "id", "file", "acad_version" })
            {
                if (!current.ContainsKey(key))
                {
                    return Result.Fail<IReadOnlyList<FixtureEntry>>(
                        ExitCode.Usage,
                        string.Create(
                            CultureInfo.InvariantCulture,
                            $"{path}: the [[fixture]] block starting at line {currentLine} has no {key}"));
                }
            }

            rows.Add(new FixtureEntry(current["id"], current["file"], current["acad_version"]));
            current = null;
            return null;
        }

        for (int i = 0; i < lines.Length; i++)
        {
            string line = lines[i].Trim();
            if (line.Length == 0 || line[0] == '#')
            {
                continue;
            }

            if (line.StartsWith('['))
            {
                Result<IReadOnlyList<FixtureEntry>>? failure = Flush();
                if (failure is not null)
                {
                    return failure.Value;
                }

                if (line.StartsWith("[[fixture]]", StringComparison.Ordinal))
                {
                    current = [];
                    currentLine = i + 1;
                }

                continue;
            }

            if (current is null)
            {
                continue;
            }

            int equals = line.IndexOf('=', StringComparison.Ordinal);
            if (equals <= 0)
            {
                continue;
            }

            string name = line[..equals].Trim();
            string value = line[(equals + 1)..].Trim();
            int comment = FindCommentStart(value);
            if (comment >= 0)
            {
                value = value[..comment].TrimEnd();
            }

            if (value.Length >= 2 && value[0] == '"' && value[^1] == '"')
            {
                current[name] = value[1..^1];
            }
        }

        Result<IReadOnlyList<FixtureEntry>>? tail = Flush();
        if (tail is not null)
        {
            return tail.Value;
        }

        if (rows.Count == 0)
        {
            return Result.Fail<IReadOnlyList<FixtureEntry>>(
                ExitCode.Usage,
                $"{path}: no [[fixture]] rows. An --all run over an empty corpus would " +
                "report success having produced nothing.");
        }

        return Result.Ok<IReadOnlyList<FixtureEntry>>(rows);
    }

    private static int FindCommentStart(string value)
    {
        bool inString = false;
        for (int i = 0; i < value.Length; i++)
        {
            if (value[i] == '"')
            {
                inString = !inString;
            }
            else if (value[i] == '#' && !inString)
            {
                return i;
            }
        }

        return -1;
    }
}
