using System.Security.Cryptography;
using ACadSharp;
using ACadSharp.Entities;
using ACadSharp.IO;
using ACadSharp.Tables;

namespace ACadSharp.Generator;

/// <summary>
/// One drawing this generator can write. The name is the fixture id and the file
/// stem, and nothing else decides either.
/// </summary>
/// <param name="Name">File stem, and the <c>id</c> of its manifest row.</param>
/// <param name="Version">The DWG version it is written as.</param>
/// <param name="Source">
/// The repository path of the one file that defines how this drawing is built,
/// recorded in its manifest row so a reader can find what produced the bytes.
/// </param>
/// <param name="Build">Fills an empty, pinned document.</param>
public sealed record Drawing(string Name, ACadVersion Version, string Source, Action<CadDocument> Build);

/// <summary>
/// Everything that would make two runs of the same code write different bytes,
/// pinned in one place.
/// </summary>
/// <remarks>
/// <c>DwgWriter</c> stamps a drawing with the time it was written and with two
/// GUIDs (measured on 3.7.1: 160 to 4000 bytes differ between two writes of one
/// document, all of them in the compressed header sections). The creation and
/// update stamps, the elapsed-time counters, both GUIDs and the summary info's
/// dates are therefore set to constants. With those pinned, two writes of one
/// document an interval apart are identical, which <c>selftest</c> checks on
/// every run rather than trusting.
/// </remarks>
public static class Writer
{
    /// <summary>Not "now" and not meaningful. A constant is all that matters.</summary>
    public static readonly DateTime Stamp = new(2026, 1, 1, 0, 0, 0, DateTimeKind.Utc);

    /// <summary>The layer every generated entity goes on.</summary>
    public const string LayerName = "GEN";

    /// <summary>The versions <c>DwgWriter</c> 3.7.1 writes. AC1021 throws.</summary>
    public static readonly ACadVersion[] Supported =
    [
        ACadVersion.AC1015,
        ACadVersion.AC1018,
        ACadVersion.AC1024,
        ACadVersion.AC1027,
        ACadVersion.AC1032,
    ];

    /// <summary>A fresh document at <paramref name="version"/> with every stamp pinned.</summary>
    public static CadDocument NewDocument(ACadVersion version)
    {
        var doc = new CadDocument();
        doc.Header.Version = version;
        doc.Header.CreateDateTime = Stamp;
        doc.Header.UpdateDateTime = Stamp;
        doc.Header.UniversalCreateDateTime = Stamp;
        doc.Header.UniversalUpdateDateTime = Stamp;
        doc.Header.TotalEditingTime = TimeSpan.Zero;
        doc.Header.UserElapsedTimeSpan = TimeSpan.Zero;
        // Read-only in the public surface, settable through the private setter.
        SetPrivate(doc.Header, "FingerPrintGuid", "{00000000-0000-0000-0000-000000000001}");
        SetPrivate(doc.Header, "VersionGuid", "{00000000-0000-0000-0000-000000000002}");
        doc.SummaryInfo.CreatedDate = Stamp;
        doc.SummaryInfo.ModifiedDate = Stamp;
        doc.Layers.Add(new Layer(LayerName));
        return doc;
    }

    /// <summary>The layer every generated entity is put on.</summary>
    public static Layer Layer(CadDocument doc) => doc.Layers[LayerName];

    /// <summary>Writes one drawing and returns the bytes.</summary>
    public static byte[] Render(Drawing drawing)
    {
        if (!Supported.Contains(drawing.Version))
        {
            throw new InvalidOperationException(
                $"{drawing.Name}: DwgWriter does not write {drawing.Version}");
        }

        CadDocument doc = NewDocument(drawing.Version);
        drawing.Build(doc);
        using var stream = new MemoryStream();
        using (var writer = new DwgWriter(stream, doc))
        {
            writer.Write();
        }

        return stream.ToArray();
    }

    /// <summary>Lowercase hex sha256.</summary>
    public static string Sha256(byte[] data) =>
        Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();

    private static void SetPrivate(object target, string property, string value)
    {
        var setter = target.GetType().GetProperty(property)?.GetSetMethod(true)
            ?? throw new InvalidOperationException(
                $"CadHeader.{property} has no setter any more; the generator's determinism "
                + "depends on it, so this is an ACadSharp upgrade to read, not to route around");
        setter.Invoke(target, [value]);
    }
}
