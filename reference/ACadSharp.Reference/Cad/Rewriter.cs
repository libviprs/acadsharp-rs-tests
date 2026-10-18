using System.Globalization;

using ACadSharp.Header;
using ACadSharp.IO;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// Reads a drawing and writes it again at another version, in another container
/// (R5.3, acadsharp-rs-tests#29). This is ACadSharp acting as a writer for the
/// suite's round-trip matrix: the oracle's reader is the independent second
/// opinion on acadrust's writer, and this is the other half.
/// </summary>
/// <remarks>
/// A read-and-rewrite keeps the source's header dates (the writer writes the
/// header's, not the clock's), so the output is a function of the source, the
/// version and the container and nothing else. The caller hashes it; two runs
/// have to agree, and a test holds that.
/// </remarks>
public static class Rewriter
{
    /// <summary>What both header GUIDs are set to before writing.</summary>
    public const string PinnedGuid = "{5ACAD5A0-0000-4000-8000-000000000001}";

    /// <summary>The containers this can write.</summary>
    public static readonly IReadOnlyList<string> Containers = ["dwg", "dxf", "dxf-binary"];

    /// <summary>Rewrites <paramref name="input"/> to <paramref name="output"/>.</summary>
    /// <param name="input">A DWG or a DXF (chosen by extension).</param>
    /// <param name="output">Where to write.</param>
    /// <param name="container"><c>dwg</c>, <c>dxf</c> (ASCII) or <c>dxf-binary</c>.</param>
    /// <param name="version">The target, for example <c>AC1015</c>.</param>
    /// <returns>Nothing on success, or why not.</returns>
    public static ExportError? Rewrite(string input, string output, string container, string version)
    {
        if (!Containers.Contains(container, StringComparer.Ordinal))
        {
            return new ExportError(
                ExitCode.Usage,
                $"--as wants one of {string.Join(", ", Containers)}, got {container}");
        }

        if (!Enum.TryParse(version, ignoreCase: false, out ACadVersion target)
            || !version.StartsWith("AC10", StringComparison.Ordinal))
        {
            return new ExportError(ExitCode.Usage, $"--version wants a version such as AC1015, got {version}");
        }

        string? directory = Path.GetDirectoryName(Path.GetFullPath(output));
        if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
        {
            return new ExportError(
                ExitCode.InvalidOutputPath, $"{output}: the directory {directory} does not exist");
        }

        CadDocument document;
        try
        {
            document = string.Equals(Path.GetExtension(input), ".dxf", StringComparison.OrdinalIgnoreCase)
                ? DxfReader.Read(input)
                : DwgReader.Read(input);
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return new ExportError(
                ExitCode.ReaderFailure, $"{input}: ACadSharp could not read this file: {ex.GetType().Name}: {ex.Message}");
        }

        try
        {
            document.Header.Version = target;

            // ACadSharp stamps a fresh GUID pair into the header of everything it
            // writes, so two writes of one drawing would differ in those bytes
            // and nowhere else. They are pinned to constants: the output has to
            // be a function of the source, the version and the container.
            // The same goes for the universal create and update times: the
            // constructor takes the clock's, the writer writes them, and the
            // source's own (local) times are what the drawing says.
            document.Header.UniversalCreateDateTime = document.Header.CreateDateTime.ToUniversalTime();
            document.Header.UniversalUpdateDateTime = document.Header.UpdateDateTime.ToUniversalTime();
            PinGuid(document.Header, nameof(CadHeader.FingerPrintGuid));
            PinGuid(document.Header, nameof(CadHeader.VersionGuid));
            switch (container)
            {
                case "dwg":
                    DwgWriter.Write(output, document);
                    break;
                case "dxf":
                    DxfWriter.Write(output, document, false);
                    break;
                default:
                    DxfWriter.Write(output, document, true);
                    break;
            }
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return new ExportError(
                ExitCode.WriterFailure,
                string.Create(
                    CultureInfo.InvariantCulture,
                    $"{input}: ACadSharp could not write {container} {version}: {ex.GetType().Name}: {ex.Message}"));
        }

        return null;
    }

    /// <summary>
    /// Writes a drawing built from nothing: a model space holding the given texts, in
    /// the given code page, at the given version (R5.4, acadsharp-rs-tests#30). This is
    /// how the code page fixtures get made, so what is in the file is the authored
    /// string and nothing a source drawing carried along.
    /// </summary>
    /// <param name="baseDrawing">The empty drawing the texts are added to.</param>
    /// <param name="output">Where to write.</param>
    /// <param name="container"><c>dwg</c>, <c>dxf</c> (ASCII) or <c>dxf-binary</c>.</param>
    /// <param name="version">The target, for example <c>AC1015</c>.</param>
    /// <param name="codePage">The header's code page string, for example <c>ANSI_1250</c>.</param>
    /// <param name="texts">One TEXT entity per string, one line apart.</param>
    /// <returns>Nothing on success, or why not.</returns>
    public static ExportError? Author(
        string baseDrawing,
        string output, string container, string version, string codePage, IReadOnlyList<string> texts)
    {
        if (!Containers.Contains(container, StringComparer.Ordinal))
        {
            return new ExportError(
                ExitCode.Usage,
                $"--as wants one of {string.Join(", ", Containers)}, got {container}");
        }

        if (!Enum.TryParse(version, ignoreCase: false, out ACadVersion target)
            || !version.StartsWith("AC10", StringComparison.Ordinal))
        {
            return new ExportError(ExitCode.Usage, $"--version wants a version such as AC1015, got {version}");
        }

        try
        {
            // Built on the empty drawing ACadSharp wrote and the suite vendors, and not
            // on `new CadDocument()`: a document made from nothing carries clock-derived
            // fields beyond the four dates and the GUID pair, and two writes of it
            // differed in the AC1018 header. A read-and-rewrite of a file is deterministic.
            CadDocument document = DwgReader.Read(baseDrawing);
            document.Header.Version = target;
            document.Header.CodePage = codePage;
            document.Header.UniversalCreateDateTime = document.Header.CreateDateTime.ToUniversalTime();
            document.Header.UniversalUpdateDateTime = document.Header.UpdateDateTime.ToUniversalTime();
            PinGuid(document.Header, nameof(CadHeader.FingerPrintGuid));
            PinGuid(document.Header, nameof(CadHeader.VersionGuid));
            for (int i = 0; i < texts.Count; i++)
            {
                document.Entities.Add(new ACadSharp.Entities.TextEntity
                {
                    Value = texts[i],
                    InsertPoint = new CSMath.XYZ(0.0, 10.0 * i, 0.0),
                    Height = 2.5,
                });
            }

            switch (container)
            {
                case "dwg":
                    DwgWriter.Write(output, document);
                    break;
                case "dxf":
                    DxfWriter.Write(output, document, false);
                    break;
                default:
                    DxfWriter.Write(output, document, true);
                    break;
            }
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return new ExportError(
                ExitCode.WriterFailure,
                string.Create(
                    CultureInfo.InvariantCulture,
                    $"{output}: ACadSharp could not author {container} {version}: {ex.GetType().Name}: {ex.Message}"));
        }

        return null;
    }

    /// <summary>
    /// <c>CadHeader</c> exposes both GUIDs read-only (the constructor draws a
    /// fresh one), so the only way to pin them is through the property's own
    /// setter. Failing to find it is a failure of the run, not something to skip:
    /// an output that moves between runs judges nothing.
    /// </summary>
    private static void PinGuid(CadHeader header, string property)
    {
        System.Reflection.PropertyInfo? info = typeof(CadHeader).GetProperty(property);
        System.Reflection.MethodInfo? setter = info?.GetSetMethod(nonPublic: true);
        if (setter is null)
        {
            throw new InvalidOperationException($"CadHeader.{property} has no setter to pin");
        }

        setter.Invoke(header, [PinnedGuid]);
    }
}
