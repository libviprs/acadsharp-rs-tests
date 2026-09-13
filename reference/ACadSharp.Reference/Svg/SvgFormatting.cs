using System.Globalization;
using System.Text;

using ACadSharp.Reference.Canonical;

namespace ACadSharp.Reference.Svg;

/// <summary>
/// The formatting rules for the visual oracle.
/// </summary>
/// <remarks>
/// <para>
/// Numbers go through the same shortest-round-trip invariant formatting the
/// JSONL uses. The SVG is a secondary artefact, but it is hashed in the same
/// manifest, so it gets the same guarantees.
/// </para>
/// <para>
/// The coordinate convention is here too, in one place, because it is the thing
/// most likely to be got wrong twice in different directions: CAD Y points up
/// and SVG Y points down, so the drawing is mirrored about the X axis, and that
/// is done by negating each Y as it is written rather than by wrapping
/// everything in a scale(1,-1) group. A group transform would mirror the
/// glyphs as well.
/// </para>
/// </remarks>
public static class SvgFormatting
{
    /// <summary>Formats a coordinate or length.</summary>
    /// <param name="value">The value.</param>
    /// <returns>The token.</returns>
    public static string Number(double value) => NumericFormatting.Real(value);

    /// <summary>Turns a CAD Y into an SVG Y.</summary>
    /// <param name="y">The CAD ordinate.</param>
    /// <returns>Its mirror.</returns>
    /// <remarks>
    /// Negating rather than subtracting from a height keeps the transform
    /// independent of the view box, so a record's SVG position does not change
    /// when an unrelated entity moves the drawing bounds.
    /// </remarks>
    public static double FlipY(double y) => y == 0.0 ? 0.0 : -y;

    /// <summary>Turns a CAD rotation in radians into an SVG rotation in degrees.</summary>
    /// <param name="radians">Counter-clockwise CAD angle.</param>
    /// <returns>The clockwise SVG angle, in degrees.</returns>
    public static double RotationDegrees(double radians) => -radians * 180.0 / Math.PI;

    /// <summary>Escapes text for an XML text node or attribute value.</summary>
    /// <param name="value">The raw text.</param>
    /// <returns>The escaped text.</returns>
    /// <remarks>
    /// The five predefined entities and nothing else. Non-ASCII stays as UTF-8,
    /// for the same reason the JSONL does not escape it: the point is that
    /// Unicode survives the whole pipeline unchanged.
    /// </remarks>
    public static string Escape(string? value)
    {
        if (string.IsNullOrEmpty(value))
        {
            return string.Empty;
        }

        var text = new StringBuilder(value.Length);
        foreach (char c in value)
        {
            switch (c)
            {
                case '&':
                    text.Append("&amp;");
                    break;
                case '<':
                    text.Append("&lt;");
                    break;
                case '>':
                    text.Append("&gt;");
                    break;
                case '"':
                    text.Append("&quot;");
                    break;
                case '\'':
                    text.Append("&apos;");
                    break;
                default:
                    if (c < ' ' && c is not '\n' and not '\t')
                    {
                        text.Append(CultureInfo.InvariantCulture, $"&#{(int)c};");
                    }
                    else
                    {
                        text.Append(c);
                    }

                    break;
            }
        }

        return text.ToString();
    }
}
