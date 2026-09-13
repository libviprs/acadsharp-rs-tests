using System.Globalization;
using System.Text;

namespace ACadSharp.Reference.Canonical;

/// <summary>
/// Turns doubles and strings into the exact bytes the canonical format allows.
/// </summary>
/// <remarks>
/// <para>
/// This is the only place in the tool that formats a number or escapes a
/// string. Everything else calls in here, so "deterministic output" is one
/// implementation to audit instead of a convention every call site has to
/// remember.
/// </para>
/// <para>
/// The rules, all of which the schema document repeats for readers who never
/// open this file: shortest round-trip formatting (<c>"R"</c>), invariant
/// culture, a <c>.0</c> suffix when the result carries no decimal point or
/// exponent so every number is visibly a double, negative zero normalised to
/// zero, and a hard refusal on anything non-finite.
/// </para>
/// </remarks>
public static class NumericFormatting
{
    /// <summary>
    /// Formats a double exactly as the canonical format requires.
    /// </summary>
    /// <param name="value">The value to format.</param>
    /// <returns>A JSON number token.</returns>
    /// <exception cref="NonFiniteValueException">
    /// The value is NaN or an infinity. Encoding either would produce a file
    /// that is not JSON at all, so the export fails loudly here rather than
    /// writing something a parser will reject three steps later.
    /// </exception>
    public static string Real(double value)
    {
        if (double.IsNaN(value) || double.IsInfinity(value))
        {
            throw new NonFiniteValueException(value);
        }

        // -0.0 and 0.0 are the same number and print differently. Two
        // implementations that both round-trip IEEE-754 correctly can disagree
        // on which one they produce for the same geometry, so the canonical
        // format only ever has one of them.
        if (value == 0.0)
        {
            value = 0.0;
        }

        string text = value.ToString("R", CultureInfo.InvariantCulture);
        foreach (char c in text)
        {
            if (c is '.' or 'e' or 'E')
            {
                return text;
            }
        }

        return text + ".0";
    }

    /// <summary>
    /// Formats an integer as a JSON number token.
    /// </summary>
    /// <param name="value">The value to format.</param>
    /// <returns>A JSON number token, with no decimal point.</returns>
    public static string Whole(long value) => value.ToString(CultureInfo.InvariantCulture);

    /// <summary>
    /// Escapes a string into a quoted JSON string literal.
    /// </summary>
    /// <param name="value">The string to escape.</param>
    /// <returns>The literal, quotes included.</returns>
    /// <remarks>
    /// Hand-written rather than delegated to <c>System.Text.Json</c>, whose
    /// default encoder escapes non-ASCII to <c>\uXXXX</c>. Section 11 of the
    /// brief requires Unicode to survive exactly, and an escaping policy that
    /// can be changed by a framework default is not a byte contract. Only the
    /// two characters JSON forbids bare, plus the C0 controls, are escaped;
    /// everything else is written as UTF-8.
    /// </remarks>
    public static string Quoted(string value)
    {
        ArgumentNullException.ThrowIfNull(value);

        var builder = new StringBuilder(value.Length + 2);
        builder.Append('"');
        foreach (char c in value)
        {
            switch (c)
            {
                case '"':
                    builder.Append("\\\"");
                    break;
                case '\\':
                    builder.Append("\\\\");
                    break;
                case '\b':
                    builder.Append("\\b");
                    break;
                case '\f':
                    builder.Append("\\f");
                    break;
                case '\n':
                    builder.Append("\\n");
                    break;
                case '\r':
                    builder.Append("\\r");
                    break;
                case '\t':
                    builder.Append("\\t");
                    break;
                default:
                    if (c < ' ')
                    {
                        builder.Append(CultureInfo.InvariantCulture, $"\\u{(int)c:x4}");
                    }
                    else
                    {
                        builder.Append(c);
                    }

                    break;
            }
        }

        builder.Append('"');
        return builder.ToString();
    }

    /// <summary>
    /// Normalises the line endings inside a CAD string to LF.
    /// </summary>
    /// <param name="value">The raw string, or <see langword="null"/>.</param>
    /// <returns>The same text with CRLF and lone CR rewritten to LF.</returns>
    /// <remarks>
    /// A DWG written on Windows and the same drawing written elsewhere can
    /// differ only in the line endings inside an MTEXT run, and that is not a
    /// difference the differential should ever report.
    /// </remarks>
    public static string NormalizeLineEndings(string? value)
    {
        if (string.IsNullOrEmpty(value))
        {
            return string.Empty;
        }

        return value.Replace("\r\n", "\n", StringComparison.Ordinal)
            .Replace('\r', '\n');
    }
}

/// <summary>
/// Thrown when geometry carries a value that cannot be encoded as JSON.
/// </summary>
/// <remarks>
/// A NaN coordinate is a real finding about the reader, not a formatting
/// problem, so it stops the export and names the value instead of being
/// rounded away into a plausible-looking zero.
/// </remarks>
public sealed class NonFiniteValueException : Exception
{
    /// <summary>Creates the exception for a specific offending value.</summary>
    /// <param name="value">The non-finite value that was about to be encoded.</param>
    public NonFiniteValueException(double value)
        : base($"refusing to encode the non-finite value {value.ToString("R", CultureInfo.InvariantCulture)}; " +
               "a canonical record cannot carry NaN or an infinity")
    {
        Value = value;
    }

    /// <summary>Creates the exception with a message.</summary>
    /// <param name="message">The message.</param>
    public NonFiniteValueException(string message)
        : base(message)
    {
    }

    /// <summary>Creates the exception with a message and an inner exception.</summary>
    /// <param name="message">The message.</param>
    /// <param name="innerException">The cause.</param>
    public NonFiniteValueException(string message, Exception innerException)
        : base(message, innerException)
    {
    }

    /// <summary>Creates the exception with no detail.</summary>
    public NonFiniteValueException()
        : base("refusing to encode a non-finite value")
    {
    }

    /// <summary>The value that could not be encoded.</summary>
    public double Value { get; }
}
