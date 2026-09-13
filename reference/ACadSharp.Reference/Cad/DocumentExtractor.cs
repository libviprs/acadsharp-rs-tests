using System.Globalization;

using ACadSharp.Entities;
using ACadSharp.IO;
using ACadSharp.Objects;
using ACadSharp.Reference.Canonical;
using ACadSharp.Tables;

namespace ACadSharp.Reference.Cad;

/// <summary>Everything one DWG produced.</summary>
/// <param name="Records">The canonical stream, already in canonical order.</param>
/// <param name="AcadVersion">The drawing's version code, for the manifest and the console.</param>
/// <param name="Log">
/// Richer reader diagnostics for the console, which never reach the canonical file.
/// </param>
public sealed record ExtractionResult(
    IReadOnlyList<CanonicalRecord> Records,
    string AcadVersion,
    IReadOnlyList<string> Log);

/// <summary>
/// Reads a DWG through ACadSharp and produces the canonical record stream.
/// </summary>
/// <remarks>
/// <para>
/// The whole pipeline in one place: read, collect notifications, walk the
/// views, extract, then order. The ordering is the part worth reading: document
/// first, then every view record, then every entity record grouped by view in
/// view order, then every warning sorted by a stable key. Section 14 fixes that
/// order and the regeneration tool hashes the result, so nothing here may
/// depend on the order ACadSharp happened to hand anything back except where
/// that order is the drawing's own.
/// </para>
/// </remarks>
public static class DocumentExtractor
{
    /// <summary>Container format stamped into the document record.</summary>
    public const string Format = "dwg";

    /// <summary>
    /// Reads a DWG and extracts it.
    /// </summary>
    /// <param name="path">Path to the input file.</param>
    /// <param name="maxDepth">Block nesting limit.</param>
    /// <returns>The records, or the reason there are none.</returns>
    public static Result<ExtractionResult> Extract(
        string path, int maxDepth = BlockResolver.DefaultMaxDepth)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);

        if (!File.Exists(path))
        {
            return Result.Fail<ExtractionResult>(
                ExitCode.ReaderFailure, $"{path}: no such file");
        }

        var warnings = new WarningCollector();
        CadDocument document;
        try
        {
            document = DwgReader.Read(path, warnings.OnNotification);
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            // ACadSharp raises a plain Exception for a header it cannot make
            // sense of, so the version case has to be separated by inspection
            // rather than by type.
            ExitCode code = ex.Message.Contains("version", StringComparison.OrdinalIgnoreCase)
                ? ExitCode.UnsupportedVersion
                : ExitCode.ReaderFailure;
            return Result.Fail<ExtractionResult>(
                code, $"{path}: ACadSharp could not read this file: {ex.GetType().Name}: {ex.Message}");
        }

        try
        {
            return Result.Ok<ExtractionResult>(ExtractDocument(document, warnings, maxDepth));
        }
        catch (NonFiniteValueException ex)
        {
            return Result.Fail<ExtractionResult>(
                ExitCode.CanonicalSerializationFailure, $"{path}: {ex.Message}");
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return Result.Fail<ExtractionResult>(
                ExitCode.UnhandledAcadSharpError,
                $"{path}: extraction failed: {ex.GetType().Name}: {ex.Message}");
        }
    }

    /// <summary>
    /// Extracts an already-open document. Exposed so tests can feed one they built in memory.
    /// </summary>
    /// <param name="document">The document.</param>
    /// <param name="warnings">The collector, already carrying any reader notifications.</param>
    /// <param name="maxDepth">Block nesting limit.</param>
    /// <returns>The extraction.</returns>
    public static ExtractionResult ExtractDocument(
        CadDocument document,
        WarningCollector warnings,
        int maxDepth = BlockResolver.DefaultMaxDepth)
    {
        ArgumentNullException.ThrowIfNull(document);
        ArgumentNullException.ThrowIfNull(warnings);

        var resolver = new BlockResolver(warnings, maxDepth);
        var extractor = new EntityExtractor(warnings, resolver);

        string version = document.Header?.Version.ToString() ?? "Unknown";
        var records = new List<CanonicalRecord>
        {
            new DocumentRecord(Format, version, document.Header?.MaintenanceVersion ?? 0),
        };

        IReadOnlyList<Layout> layouts = OrderLayouts(document);
        var entityRecords = new List<CanonicalRecord>();
        for (int index = 0; index < layouts.Count; index++)
        {
            Layout layout = layouts[index];
            records.Add(new ViewRecord(index, layout.Name ?? string.Empty, KindOf(layout)));

            BlockRecord? block = layout.AssociatedBlock;
            if (block is null)
            {
                warnings.Add(
                    WarningCategory.MissingReference,
                    nameof(Layout),
                    "layout_without_block_record");
                continue;
            }

            extractor.ExtractAll(
                block.Entities,
                index,
                CadTransform.Identity,
                depth: 0,
                insideBlock: false,
                entityRecords);
        }

        records.AddRange(entityRecords);
        records.AddRange(CanonicalWriter.SortWarnings(warnings.Records));

        return new ExtractionResult(records, version, warnings.Log);
    }

    /// <summary>
    /// Puts the drawing's layouts into the order the view records use.
    /// </summary>
    /// <param name="document">The document.</param>
    /// <returns>The layouts, in tab order.</returns>
    /// <remarks>
    /// Tab order, then name, and never the order the collection enumerates in.
    /// ACadSharp's layout collection is keyed by name and hands them back
    /// alphabetically, which would put Layout1 and Layout2 in front of Model.
    /// Tab order is the drawing's own answer to "which tab is first", it is
    /// stored in the file, and model space is always zero.
    /// </remarks>
    public static IReadOnlyList<Layout> OrderLayouts(CadDocument document)
    {
        ArgumentNullException.ThrowIfNull(document);
        return [.. document.Layouts
            .OrderBy(l => l.TabOrder)
            .ThenBy(l => l.Name ?? string.Empty, StringComparer.Ordinal)];
    }

    private static ViewKind KindOf(Layout layout)
    {
        if (!layout.IsPaperSpace)
        {
            return ViewKind.Model;
        }

        return layout.AssociatedBlock is null ? ViewKind.Unknown : ViewKind.Layout;
    }

    /// <summary>
    /// A one-line census for the console and the regeneration summary.
    /// </summary>
    /// <param name="result">The extraction.</param>
    /// <returns>Record counts by type, ordered by name.</returns>
    public static string Census(ExtractionResult result)
    {
        ArgumentNullException.ThrowIfNull(result);
        IReadOnlyDictionary<string, int> counts = CanonicalWriter.CountByRecordName(result.Records);
        return string.Create(
            CultureInfo.InvariantCulture,
            $"{result.Records.Count} records ({CanonicalWriter.FormatCounts(counts)})");
    }
}
