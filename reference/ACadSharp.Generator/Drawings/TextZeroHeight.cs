using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// A TEXT and an MTEXT with height 0, and an MTEXT with a very long string.
/// </summary>
/// <remarks>
/// Measured against ACadSharp 3.7.1 itself: the public height setters refuse 0, so
/// the zero heights go in through the backing field, DwgWriter writes them, and
/// DwgReader then fails on reading them back (its own setter throws), which the
/// oracle reports as one <c>reader_error</c> warning per entity and no record. So
/// the pinned reference cannot hold a zero-height text, and what this fixture pins
/// is that it is lost loudly (two warnings) while the long string beside it
/// survives intact.
/// </remarks>
public static class TextZeroHeight
{
    public const string Name = "regression_text_zero_height";

    /// <summary>Length of the MTEXT string, in UTF-16 code units. The claim test uses it.</summary>
    public const int LongLength = 6000;

    /// <summary>
    /// 6000 characters that are not periodic with a short period, so truncation or
    /// a wrapped buffer cannot reproduce the string by accident.
    /// </summary>
    public static string LongString()
    {
        var sb = new System.Text.StringBuilder(LongLength);
        for (int i = 0; sb.Length < LongLength; i++)
        {
            sb.Append((char)('A' + (i * 7 + i / 26) % 26));
        }

        return sb.ToString(0, LongLength);
    }

    private static void SetHeightToZero(MText mtext)
    {
        try
        {
            mtext.Height = 0.0;
        }
        catch (ArgumentOutOfRangeException)
        {
            // Same reason as the TEXT below.
            var field = typeof(MText).GetField(
                    "_height", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)
                ?? throw new InvalidOperationException(
                    "MText no longer has a _height field; read the ACadSharp upgrade before routing around it");
            field.SetValue(mtext, 0.0);
        }
    }

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/TextZeroHeight.cs",
        doc =>
        {
            var text = new TextEntity
            {
                Value = "ZERO HEIGHT",
                InsertPoint = new XYZ(10, 10, 0),
                Layer = Writer.Layer(doc),
            };
            // The public setter refuses 0 ("must be greater than zero"), but a
            // height of 0 is what an AutoCAD drawing can hold and what the bug
            // class is about, so it goes in through the backing field. If the
            // field is renamed by an ACadSharp upgrade this throws, on purpose.
            var field = typeof(TextEntity).GetField(
                    "_height", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)
                ?? throw new InvalidOperationException(
                    "TextEntity no longer has a _height field; read the ACadSharp upgrade before routing around it");
            field.SetValue(text, 0.0);
            doc.Entities.Add(text);
            var flat = new MText
            {
                Value = "ZERO HEIGHT MTEXT",
                InsertPoint = new XYZ(10, 20, 0),
                Layer = Writer.Layer(doc),
            };
            SetHeightToZero(flat);
            doc.Entities.Add(flat);
            doc.Entities.Add(new MText
            {
                Value = LongString(),
                InsertPoint = new XYZ(10, 30, 0),
                Height = 2.5,
                Layer = Writer.Layer(doc),
            });
        });
}
