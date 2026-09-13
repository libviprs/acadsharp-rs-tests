using System.Globalization;

using ACadSharp.IO;
using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// Turns ACadSharp reader notifications and extractor findings into warning records.
/// </summary>
/// <remarks>
/// <para>
/// Two jobs, deliberately in one place. The reader reports through an event and
/// the extractor reports inline, and both end up in the same sorted block at
/// the end of the canonical file, so having one owner of the category mapping
/// keeps the two from drifting apart.
/// </para>
/// <para>
/// The canonical side of a notification is its category and a short
/// discriminator. The rich text, including any exception, goes to the console
/// log instead: an exception message carries paths and sometimes a stack, and
/// none of that can sit in a file whose sha256 is in a manifest.
/// </para>
/// </remarks>
public sealed class WarningCollector
{
    private readonly List<WarningRecord> _records = [];
    private readonly List<string> _log = [];

    /// <summary>Every warning gathered so far, in arrival order.</summary>
    /// <remarks>
    /// Arrival order is for the console only. <see cref="CanonicalWriter.SortWarnings"/>
    /// puts them in canonical order before they are written.
    /// </remarks>
    public IReadOnlyList<WarningRecord> Records => _records;

    /// <summary>The richer console diagnostics, which never reach the canonical file.</summary>
    public IReadOnlyList<string> Log => _log;

    /// <summary>
    /// The handler to hand to <c>DwgReader.Read</c>.
    /// </summary>
    /// <param name="sender">The reader.</param>
    /// <param name="e">The notification.</param>
    public void OnNotification(object? sender, NotificationEventArgs e)
    {
        ArgumentNullException.ThrowIfNull(e);

        (WarningCategory category, string entityType, string detail) = Classify(e);
        _records.Add(new WarningRecord(category, entityType, detail));
        _log.Add(string.Create(
            CultureInfo.InvariantCulture,
            $"[{e.NotificationType}] {e.Message}{(e.Exception is null ? string.Empty : $" ({e.Exception.GetType().Name}: {e.Exception.Message})")}"));
    }

    /// <summary>Records an extractor finding.</summary>
    /// <param name="category">The category.</param>
    /// <param name="entityType">The ACadSharp type name, or a DXF name.</param>
    /// <param name="detail">A short deterministic discriminator, possibly empty.</param>
    public void Add(WarningCategory category, string entityType, string detail = "")
    {
        _records.Add(new WarningRecord(category, entityType, detail));
    }

    /// <summary>
    /// Maps a reader notification onto a category and a stable discriminator.
    /// </summary>
    /// <param name="e">The notification.</param>
    /// <returns>The category, the entity or object type it concerns, and the detail.</returns>
    /// <remarks>
    /// The patterns are matched against ACadSharp 3.7.1's own wording. When a
    /// future ACadSharp rewords a message the match falls through to
    /// <see cref="WarningCategory.ReaderNotification"/> with an empty type,
    /// which shows up as a diff in the expected output rather than as a silent
    /// reclassification: that diff is the point.
    /// </remarks>
    public static (WarningCategory Category, string EntityType, string Detail) Classify(
        NotificationEventArgs e)
    {
        ArgumentNullException.ThrowIfNull(e);
        string message = e.Message ?? string.Empty;

        // "Unlisted object with DXF name FOO has been read as an UnknownNonGraphicalObject"
        const string unlistedPrefix = "Unlisted object with DXF name ";
        if (message.StartsWith(unlistedPrefix, StringComparison.Ordinal))
        {
            return (WarningCategory.UnsupportedEntity, FirstToken(message[unlistedPrefix.Length..]), "unlisted_dxf_class");
        }

        // "Entry not found NAME|1883 for dictionary DICT|1882"
        const string entryPrefix = "Entry not found ";
        if (message.StartsWith(entryPrefix, StringComparison.Ordinal))
        {
            return (WarningCategory.MissingReference, FirstToken(message[entryPrefix.Length..]), "entry_not_found");
        }

        // "XRecord reference not found 360|1863"
        if (message.StartsWith("XRecord reference not found", StringComparison.Ordinal))
        {
            return (WarningCategory.MissingReference, "XRecord", "xrecord_reference_not_found");
        }

        // "ACadSharp.Tables.TextStyle table reference with handle: X | name: Y not found for Z"
        const string tableMarker = " table reference with handle";
        int tableAt = message.IndexOf(tableMarker, StringComparison.Ordinal);
        if (tableAt > 0)
        {
            string type = message[..tableAt];
            int dot = type.LastIndexOf('.');
            return (
                WarningCategory.MissingReference,
                dot >= 0 ? type[(dot + 1)..] : type,
                "table_reference_not_found");
        }

        if (message.Contains("font", StringComparison.OrdinalIgnoreCase)
            || message.Contains(".shx", StringComparison.OrdinalIgnoreCase)
            || message.Contains(".ttf", StringComparison.OrdinalIgnoreCase))
        {
            return (WarningCategory.MissingFont, string.Empty, "reader_font_notification");
        }

        return e.NotificationType switch
        {
            NotificationType.NotImplemented =>
                (WarningCategory.UnsupportedEntity, string.Empty, "reader_not_implemented"),
            NotificationType.NotSupported =>
                (WarningCategory.UnsupportedEntity, string.Empty, "reader_not_supported"),
            NotificationType.Error =>
                (WarningCategory.InvalidEntity, string.Empty, "reader_error"),
            _ => (WarningCategory.ReaderNotification, string.Empty, "reader_warning"),
        };
    }

    /// <summary>
    /// The leading name of a notification, with any handle suffix removed.
    /// </summary>
    /// <param name="rest">The part of the message after a known prefix.</param>
    /// <returns>The name alone.</returns>
    /// <remarks>
    /// ACadSharp spells a reference as <c>NAME|1883</c>, and the number is a
    /// DWG handle. Handles are stable inside one file and meaningless across
    /// two, so the same drawing saved as AC1018 and AC1032 would produce
    /// warning records that differ only in numbers nothing can compare. Section
    /// 28 says to keep object identity out of the schema unless it is stable
    /// and required; it is neither.
    /// </remarks>
    public static string FirstToken(string rest)
    {
        ArgumentNullException.ThrowIfNull(rest);
        int cut = rest.Length;
        for (int i = 0; i < rest.Length; i++)
        {
            if (rest[i] is ' ' or '|')
            {
                cut = i;
                break;
            }
        }

        return rest[..cut];
    }
}
