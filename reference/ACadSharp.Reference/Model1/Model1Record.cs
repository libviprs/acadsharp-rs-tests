using System.Text;

using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Model1;

/// <summary>
/// One line of the <c>model-1</c> stream: the kind it belongs to, the key that
/// names it inside that kind, and its encoded JSON.
/// </summary>
/// <remarks>
/// The stream is sorted by (kind order, key) before it is written, so nothing
/// about the order ACadSharp enumerates a table or a header in reaches the file.
/// See <c>docs/MODEL_1.md</c>.
/// </remarks>
/// <param name="Kind">The record kind, for example <c>header</c>.</param>
/// <param name="Key">The key inside the kind, compared ordinally.</param>
/// <param name="Json">The line, without its trailing newline.</param>
public sealed record Model1Record(string Kind, string Key, string Json)
{
    private static readonly string[] Kinds =
    ["header", "layer", "linetype", "textstyle", "dimstyle", "block_record", "layout", "dictionary", "xdata", "xrecord"];

    /// <summary>The kinds in stream order.</summary>
    public static IReadOnlyList<string> KindOrder => Kinds;

    /// <summary>The first field of every line, so a reader cannot take one for a schema 1 record.</summary>
    public const string Prefix = "{\"model\":1,\"record\":";

    /// <summary>Encodes a stream as the exact bytes of a <c>.model1.jsonl</c> file.</summary>
    /// <param name="records">The records, in any order.</param>
    /// <returns>UTF-8 bytes, no byte order mark, LF line endings, a final newline.</returns>
    public static byte[] Encode(IEnumerable<Model1Record> records)
    {
        ArgumentNullException.ThrowIfNull(records);
        var ordered = records
            .OrderBy(r => Array.IndexOf(Kinds, r.Kind))
            .ThenBy(r => r.Key, StringComparer.Ordinal)
            .ToList();
        foreach (Model1Record r in ordered)
        {
            if (Array.IndexOf(Kinds, r.Kind) < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(records), r.Kind, "not a model-1 kind");
            }
        }

        for (int i = 1; i < ordered.Count; i++)
        {
            if (ordered[i].Kind == ordered[i - 1].Kind
                && string.Equals(ordered[i].Key, ordered[i - 1].Key, StringComparison.Ordinal))
            {
                throw new ArgumentException(
                    $"two {ordered[i].Kind} records are keyed {ordered[i].Key}", nameof(records));
            }
        }

        var text = new StringBuilder();
        foreach (Model1Record r in ordered)
        {
            text.Append(r.Json);
            text.Append('\n');
        }

        return new UTF8Encoding(encoderShouldEmitUTF8Identifier: false).GetBytes(text.ToString());
    }

    /// <summary>Builds a line from a kind and already-encoded fields.</summary>
    /// <param name="kind">The record kind.</param>
    /// <param name="key">The key inside the kind.</param>
    /// <param name="fields">The remaining fields, as (name, encoded value) in the order the document gives.</param>
    /// <returns>The record.</returns>
    public static Model1Record Of(string kind, string key, params (string Name, string Value)[] fields)
    {
        var text = new StringBuilder(Prefix);
        text.Append(NumericFormatting.Quoted(kind));
        foreach ((string name, string value) in fields)
        {
            text.Append(',');
            text.Append(NumericFormatting.Quoted(name));
            text.Append(':');
            text.Append(value);
        }

        text.Append('}');
        return new Model1Record(kind, key, text.ToString());
    }
}
