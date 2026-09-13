using System.Globalization;
using System.Text;

using ACadSharp.Reference;
using ACadSharp.Reference.Cad;
using ACadSharp.Reference.Canonical;
using ACadSharp.Reference.Svg;

// The .NET JIT reference oracle.
//
//   dotnet run --project reference/ACadSharp.Reference -- \
//       fixtures/dwg/sample_AC1032.dwg \
//       --jsonl out/acadsharp-ac1032.reference.jsonl \
//       --svg   out/acadsharp-ac1032.reference.svg
//
//   dotnet run --project reference/ACadSharp.Reference -- \
//       --manifest fixtures/manifest.toml --all --output out/
//
// Synchronous from end to end on purpose. It reads a handful of files and
// writes a handful of files; wrapping that in Task.Run would add a thread pool
// hop and a cancellation story to a batch tool that has neither.
return CommandLine.Run(args, Console.Out, Console.Error);

namespace ACadSharp.Reference
{
    /// <summary>The command line, kept out of the entry point so tests can drive it.</summary>
    public static class CommandLine
    {
        /// <summary>Suffix of the canonical semantic artefact, before compression.</summary>
        public const string JsonlSuffix = ".reference.jsonl";

        /// <summary>Suffix of the visual artefact.</summary>
        public const string SvgSuffix = ".reference.svg";

        /// <summary>Runs the tool.</summary>
        /// <param name="args">Command line arguments, without the program name.</param>
        /// <param name="output">Where progress goes.</param>
        /// <param name="error">Where failures go.</param>
        /// <returns>The process exit code.</returns>
        public static int Run(string[] args, TextWriter output, TextWriter error)
        {
            ArgumentNullException.ThrowIfNull(args);
            ArgumentNullException.ThrowIfNull(output);
            ArgumentNullException.ThrowIfNull(error);

            Result<Options> parsed = Options.Parse(args);
            if (!parsed.IsOk)
            {
                error.WriteLine(parsed.Error!.Message);
                error.WriteLine();
                error.WriteLine(Options.Usage);
                return (int)parsed.Error.Code;
            }

            Options options = parsed.Value;
            if (options.ShowHelp)
            {
                output.WriteLine(Options.Usage);
                return (int)ExitCode.Ok;
            }

            Result<IReadOnlyList<Job>> jobs = BuildJobs(options);
            if (!jobs.IsOk)
            {
                error.WriteLine(jobs.Error!.Message);
                return (int)jobs.Error.Code;
            }

            var summaries = new List<string>();
            foreach (Job job in jobs.Value)
            {
                ExportError? failure = RunJob(job, options, output, error, summaries);
                if (failure is not null)
                {
                    error.WriteLine(failure.Message);
                    return (int)failure.Code;
                }
            }

            if (options.SummaryPath is { } summaryPath)
            {
                ExportError? failure = WriteSummary(summaryPath, summaries);
                if (failure is not null)
                {
                    error.WriteLine(failure.Message);
                    return (int)failure.Code;
                }
            }

            return (int)ExitCode.Ok;
        }

        private static ExportError? RunJob(
            Job job, Options options, TextWriter output, TextWriter error, List<string> summaries)
        {
            Result<ExtractionResult> extracted = DocumentExtractor.Extract(job.Input, options.MaxDepth);
            if (!extracted.IsOk)
            {
                return extracted.Error;
            }

            ExtractionResult result = extracted.Value;
            if (options.Verbose)
            {
                foreach (string line in result.Log)
                {
                    error.WriteLine(string.Create(CultureInfo.InvariantCulture, $"  {job.Id}: {line}"));
                }
            }

            if (!string.Equals(result.AcadVersion, job.ExpectedVersion, StringComparison.Ordinal)
                && job.ExpectedVersion.Length > 0)
            {
                return new ExportError(
                    ExitCode.UnsupportedVersion,
                    $"{job.Input}: the manifest says {job.ExpectedVersion} but ACadSharp read " +
                    $"{result.AcadVersion}. One of the two is describing a different file.");
            }

            byte[] jsonl;
            try
            {
                jsonl = CanonicalWriter.Encode(result.Records);
            }
            catch (NonFiniteValueException ex)
            {
                return new ExportError(
                    ExitCode.CanonicalSerializationFailure, $"{job.Input}: {ex.Message}");
            }
            catch (ArgumentOutOfRangeException ex)
            {
                return new ExportError(
                    ExitCode.CanonicalSerializationFailure, $"{job.Input}: {ex.Message}");
            }

            ExportError? written = Write(job.JsonlPath, jsonl);
            if (written is not null)
            {
                return written;
            }

            if (job.SvgPath is { } svgPath)
            {
                byte[] svg;
                try
                {
                    svg = SvgWriter.Encode(result.Records, options.MarginFraction);
                }
                catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
                {
                    return new ExportError(
                        ExitCode.SvgGenerationFailure,
                        $"{job.Input}: SVG generation failed: {ex.GetType().Name}: {ex.Message}");
                }

                written = Write(svgPath, svg);
                if (written is not null)
                {
                    return written;
                }
            }

            output.WriteLine(string.Create(
                CultureInfo.InvariantCulture,
                $"{job.Id}: {result.AcadVersion}, {DocumentExtractor.Census(result)}"));

            IReadOnlyDictionary<string, int> counts =
                CanonicalWriter.CountByRecordName(result.Records);
            summaries.Add(SummaryEntry(job, result, counts));
            return null;
        }

        private static string SummaryEntry(
            Job job, ExtractionResult result, IReadOnlyDictionary<string, int> counts)
        {
            var text = new StringBuilder();
            text.Append("  {");
            text.Append(CultureInfo.InvariantCulture, $"\"id\": {NumericFormatting.Quoted(job.Id)}, ");
            text.Append(CultureInfo.InvariantCulture,
                $"\"acad_version\": {NumericFormatting.Quoted(result.AcadVersion)}, ");
            text.Append(CultureInfo.InvariantCulture,
                $"\"records\": {NumericFormatting.Whole(result.Records.Count)}, ");
            text.Append("\"counts\": {");
            bool first = true;
            foreach ((string name, int count) in counts)
            {
                if (!first)
                {
                    text.Append(", ");
                }

                first = false;
                text.Append(CultureInfo.InvariantCulture,
                    $"{NumericFormatting.Quoted(name)}: {NumericFormatting.Whole(count)}");
            }

            text.Append("}}");
            return text.ToString();
        }

        private static ExportError? WriteSummary(string path, List<string> entries)
        {
            var text = new StringBuilder("[\n");
            for (int i = 0; i < entries.Count; i++)
            {
                text.Append(entries[i]);
                text.Append(i == entries.Count - 1 ? "\n" : ",\n");
            }

            text.Append("]\n");
            return Write(path, new UTF8Encoding(false).GetBytes(text.ToString()));
        }

        private static ExportError? Write(string path, byte[] bytes)
        {
            try
            {
                string? directory = Path.GetDirectoryName(Path.GetFullPath(path));
                if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
                {
                    return new ExportError(
                        ExitCode.InvalidOutputPath,
                        $"{path}: the directory {directory} does not exist. This tool will not " +
                        "create one: a typo in an output path would otherwise leave the real " +
                        "artefacts unwritten and still exit zero.");
                }

                File.WriteAllBytes(path, bytes);
                return null;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException
                                          or NotSupportedException or ArgumentException)
            {
                return new ExportError(
                    ExitCode.InvalidOutputPath, $"{path}: cannot write: {ex.Message}");
            }
        }

        private static Result<IReadOnlyList<Job>> BuildJobs(Options options)
        {
            if (options.ManifestPath is { } manifestPath)
            {
                Result<IReadOnlyList<FixtureEntry>> rows = FixtureManifest.Load(manifestPath);
                if (!rows.IsOk)
                {
                    return Result.Fail<IReadOnlyList<Job>>(rows.Error!);
                }

                string manifestDirectory =
                    Path.GetDirectoryName(Path.GetFullPath(manifestPath)) ?? ".";
                string outputDirectory = options.OutputDirectory!;

                var jobs = new List<Job>(rows.Value.Count);
                foreach (FixtureEntry row in rows.Value)
                {
                    jobs.Add(new Job(
                        row.Id,
                        Path.Combine(manifestDirectory, row.File.Replace('/', Path.DirectorySeparatorChar)),
                        Path.Combine(outputDirectory, row.Id + JsonlSuffix),
                        options.NoSvg ? null : Path.Combine(outputDirectory, row.Id + SvgSuffix),
                        row.AcadVersion));
                }

                return Result.Ok<IReadOnlyList<Job>>(jobs);
            }

            string input = options.Input!;
            return Result.Ok<IReadOnlyList<Job>>([
                new Job(
                    Path.GetFileNameWithoutExtension(input),
                    input,
                    options.JsonlPath!,
                    options.SvgPath,
                    string.Empty),
            ]);
        }

        private sealed record Job(
            string Id, string Input, string JsonlPath, string? SvgPath, string ExpectedVersion);

        /// <summary>The parsed command line.</summary>
        public sealed record Options
        {
            /// <summary>The usage text, which is also the specification of the CLI.</summary>
            public const string Usage = """
                ACadSharp.Reference: the .NET JIT reference oracle for acadsharp-rs.

                One file:
                  ACadSharp.Reference <input.dwg> --jsonl <out.jsonl> [--svg <out.svg>]

                The whole corpus:
                  ACadSharp.Reference --manifest <manifest.toml> --all --output <dir>

                Options:
                  --jsonl <path>      Where to write the canonical JSONL. Required for a single file.
                  --svg <path>        Where to write the visual SVG. Optional for a single file.
                  --manifest <path>   fixtures/manifest.toml. Requires --all and --output.
                  --all               Process every [[fixture]] row in the manifest.
                  --output <dir>      Directory for --all output, named <id>.reference.jsonl/.svg.
                  --no-svg            With --all, skip the SVG artefacts.
                  --summary <path>    Write a JSON census of what was produced.
                  --max-depth <n>     Block nesting limit (default 16).
                  --svg-margin <f>    SVG margin as a fraction of the drawing size (default 0.02).
                  --verbose           Echo the reader's own diagnostics to stderr.
                  -h, --help          This text.

                Exit codes: 0 ok, 2 usage, 3 reader failure, 4 unsupported version,
                5 invalid output path, 6 canonical serialization failure,
                7 SVG generation failure, 8 unhandled ACadSharp error.

                This tool never compresses and never writes into a checked-in
                directory of its own accord. tools/regenerate_reference.py owns
                both, and owns the --accept gate.
                """;

            /// <summary>The single input file, when not running over a manifest.</summary>
            public string? Input { get; init; }

            /// <summary>Where the single run writes its JSONL.</summary>
            public string? JsonlPath { get; init; }

            /// <summary>Where the single run writes its SVG, if anywhere.</summary>
            public string? SvgPath { get; init; }

            /// <summary>The manifest to drive <c>--all</c> from.</summary>
            public string? ManifestPath { get; init; }

            /// <summary>Where <c>--all</c> writes.</summary>
            public string? OutputDirectory { get; init; }

            /// <summary>Whether <c>--all</c> skips SVG generation.</summary>
            public bool NoSvg { get; init; }

            /// <summary>Where to write the JSON census, if anywhere.</summary>
            public string? SummaryPath { get; init; }

            /// <summary>Block nesting limit.</summary>
            public int MaxDepth { get; init; } = BlockResolver.DefaultMaxDepth;

            /// <summary>SVG margin as a fraction of the larger drawing dimension.</summary>
            public double MarginFraction { get; init; } = SvgWriter.DefaultMarginFraction;

            /// <summary>Whether to echo reader diagnostics.</summary>
            public bool Verbose { get; init; }

            /// <summary>Whether the user just asked for help.</summary>
            public bool ShowHelp { get; init; }

            /// <summary>Parses arguments.</summary>
            /// <param name="args">The arguments.</param>
            /// <returns>The options, or why they did not parse.</returns>
            public static Result<Options> Parse(string[] args)
            {
                string? input = null;
                string? jsonl = null;
                string? svg = null;
                string? manifest = null;
                string? outputDirectory = null;
                string? summary = null;
                bool noSvg = false;
                bool all = false;
                bool verbose = false;
                int maxDepth = BlockResolver.DefaultMaxDepth;
                double margin = SvgWriter.DefaultMarginFraction;

                for (int i = 0; i < args.Length; i++)
                {
                    string arg = args[i];
                    string? Next(string name)
                    {
                        if (i + 1 >= args.Length)
                        {
                            return null;
                        }

                        i++;
                        _ = name;
                        return args[i];
                    }

                    switch (arg)
                    {
                        case "-h":
                        case "--help":
                            return Result.Ok<Options>(new Options { ShowHelp = true });

                        case "--jsonl":
                            jsonl = Next(arg);
                            if (jsonl is null)
                            {
                                return Missing(arg);
                            }

                            break;

                        case "--svg":
                            svg = Next(arg);
                            if (svg is null)
                            {
                                return Missing(arg);
                            }

                            break;

                        case "--manifest":
                            manifest = Next(arg);
                            if (manifest is null)
                            {
                                return Missing(arg);
                            }

                            break;

                        case "--output":
                            outputDirectory = Next(arg);
                            if (outputDirectory is null)
                            {
                                return Missing(arg);
                            }

                            break;

                        case "--summary":
                            summary = Next(arg);
                            if (summary is null)
                            {
                                return Missing(arg);
                            }

                            break;

                        case "--all":
                            all = true;
                            break;

                        case "--no-svg":
                            noSvg = true;
                            break;

                        case "--verbose":
                            verbose = true;
                            break;

                        case "--max-depth":
                        {
                            string? value = Next(arg);
                            if (value is null)
                            {
                                return Missing(arg);
                            }

                            if (!int.TryParse(value, CultureInfo.InvariantCulture, out maxDepth)
                                || maxDepth < 1)
                            {
                                return Result.Fail<Options>(
                                    ExitCode.Usage, $"--max-depth wants a positive integer, got {value}");
                            }

                            break;
                        }

                        case "--svg-margin":
                        {
                            string? value = Next(arg);
                            if (value is null)
                            {
                                return Missing(arg);
                            }

                            if (!double.TryParse(value, CultureInfo.InvariantCulture, out margin)
                                || margin < 0.0 || !double.IsFinite(margin))
                            {
                                return Result.Fail<Options>(
                                    ExitCode.Usage,
                                    $"--svg-margin wants a non-negative finite number, got {value}");
                            }

                            break;
                        }

                        default:
                            if (arg.StartsWith('-'))
                            {
                                return Result.Fail<Options>(ExitCode.Usage, $"unknown option {arg}");
                            }

                            if (input is not null)
                            {
                                return Result.Fail<Options>(
                                    ExitCode.Usage,
                                    $"more than one input file given ({input} and {arg}); " +
                                    "use --manifest --all to process a corpus");
                            }

                            input = arg;
                            break;
                    }
                }

                if (args.Length == 0)
                {
                    return Result.Ok<Options>(new Options { ShowHelp = true });
                }

                if (manifest is not null)
                {
                    if (!all)
                    {
                        return Result.Fail<Options>(
                            ExitCode.Usage, "--manifest needs --all; there is nothing else to do with it");
                    }

                    if (outputDirectory is null)
                    {
                        return Result.Fail<Options>(ExitCode.Usage, "--all needs --output <dir>");
                    }

                    if (input is not null)
                    {
                        return Result.Fail<Options>(
                            ExitCode.Usage,
                            $"--all processes the manifest, so the extra input {input} would be ignored");
                    }
                }
                else
                {
                    if (all)
                    {
                        return Result.Fail<Options>(ExitCode.Usage, "--all needs --manifest <path>");
                    }

                    if (input is null)
                    {
                        return Result.Fail<Options>(ExitCode.Usage, "no input file given");
                    }

                    if (jsonl is null)
                    {
                        return Result.Fail<Options>(
                            ExitCode.Usage,
                            "--jsonl is required: the canonical JSONL is the authoritative artefact, " +
                            "so a run that produced only an SVG would be a run that produced nothing " +
                            "the differential can use");
                    }
                }

                return Result.Ok<Options>(new Options
                {
                    Input = input,
                    JsonlPath = jsonl,
                    SvgPath = svg,
                    ManifestPath = manifest,
                    OutputDirectory = outputDirectory,
                    SummaryPath = summary,
                    NoSvg = noSvg,
                    MaxDepth = maxDepth,
                    MarginFraction = margin,
                    Verbose = verbose,
                });
            }

            private static Result<Options> Missing(string option) =>
                Result.Fail<Options>(ExitCode.Usage, $"{option} needs a value");
        }
    }
}
