using System.Globalization;

using ACadSharp.Header;
using ACadSharp.IO;
using ACadSharp.Reference.Canonical;
using ACadSharp.Tables;

namespace ACadSharp.Reference.Model1;

/// <summary>
/// Reads the <c>model-1</c> records out of a document: everything past what a
/// viewer draws. See <c>docs/MODEL_1.md</c> for the contract.
/// </summary>
public static class Model1Extractor
{
    /// <summary>Reads a DWG or a DXF (by extension) and extracts its model-1 records.</summary>
    /// <param name="path">Path to the input file.</param>
    /// <returns>The encoded stream, or the reason there is none.</returns>
    public static Result<byte[]> ExtractFile(string path)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);
        if (!File.Exists(path))
        {
            return Result.Fail<byte[]>(ExitCode.ReaderFailure, $"{path}: no such file");
        }

        CadDocument document;
        try
        {
            // The container is chosen by extension, as the schema 1 extraction does.
            // An object whose class ACadSharp does not know is dropped unless
            // KeepUnknownNonGraphicalObjects is on, and a dictionary entry that is
            // dropped has no class name to compare. The switch keeps the object
            // after the reader's notification has fired, and schema 1 emits no
            // object, so nothing else moves because of it.
            //
            // The DXF reader needs a second switch. DxfObjectsSectionReader reads
            // the codes of an unknown object (its handle among them) only when
            // KeepUnknownEntities is on, where the name says it means
            // KeepUnknownNonGraphicalObjects, so with the right switch alone the
            // object has no handle, no dictionary resolves it and the entry is
            // dropped. The unknown entities this keeps are drawing content, which
            // no model-1 record reads, and the DWG reader is not given it.
            document = string.Equals(Path.GetExtension(path), ".dxf", StringComparison.OrdinalIgnoreCase)
                ? DxfReader.Read(
                    path,
                    new DxfReaderConfiguration { KeepUnknownNonGraphicalObjects = true, KeepUnknownEntities = true })
                : DwgReader.Read(path, new DwgReaderConfiguration { KeepUnknownNonGraphicalObjects = true });
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return Result.Fail<byte[]>(
                ExitCode.ReaderFailure, $"{path}: ACadSharp could not read this file: {ex.GetType().Name}: {ex.Message}");
        }

        try
        {
            bool dxf = string.Equals(Path.GetExtension(path), ".dxf", StringComparison.OrdinalIgnoreCase);
            return Result.Ok<byte[]>(Model1Record.Encode(Extract(document, dxf)));
        }
        catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
        {
            return Result.Fail<byte[]>(
                ExitCode.UnhandledAcadSharpError,
                $"{path}: model-1 extraction failed: {ex.GetType().Name}: {ex.Message}");
        }
    }

    /// <summary>Extracts every model-1 record the document holds.</summary>
    /// <param name="document">The document.</param>
    /// <returns>The records, unsorted. <see cref="Model1Record.Encode"/> sorts them.</returns>
    /// <param name="dxf">Whether the document was read from a DXF.</param>
    public static IReadOnlyList<Model1Record> Extract(CadDocument document, bool dxf = false)
    {
        s_dxf = dxf;
        ArgumentNullException.ThrowIfNull(document);
        var records = new List<Model1Record>();
        ExtractHeader(document, records);
        ExtractLayers(document, records);
        ExtractLinetypes(document, records);
        ExtractTextStyles(document, records);
        Dictionary<object, string> owners = ExtractDictionaries(document, records);
        ExtractXData(document, records, owners);
        return records;
    }

    private static void ExtractHeader(CadDocument document, List<Model1Record> records)
    {
        CadHeader header = document.Header;

        // GetHeaderMap is reflection over CadHeader's attributed properties,
        // which is fine in a JIT process. It is the one enumeration of the
        // header's variables that carries the DXF name and the DXF codes.
        foreach (string name in CadHeader.GetHeaderMap().Keys)
        {
            // ACadSharp initialises these two to the time of the read and its DWG
            // reader never sets them, so their value is the moment the oracle ran
            // and a frozen file would move on every freeze. Not written, and listed
            // as excluded for the oracle in parity/MODEL1.toml.
            if (name is "$TDUCREATE" or "$TDUUPDATE")
            {
                continue;
            }

            Dictionary<DxfCode, object>? values = header.GetValues(name);
            if (values is null || values.Count == 0)
            {
                throw new InvalidOperationException($"{name}: the header map lists it and GetValues gives nothing");
            }

            List<int> codes = [.. values.Keys.Select(k => (int)k)];
            string type = HeaderType(name, codes);
            string value = type switch
            {
                "point2" => Point(values, 2),
                "point3" => Point(values, 3),
                _ => HeaderValue(name, type, values.Values.First()),
            };

            // A GUID a version does not store is a fresh random one per read, which
            // cannot be frozen, so it is written empty. R14 files hold neither.
            if (name is "$VERSIONGUID" or "$FINGERPRINTGUID" && header.Version < ACadVersion.AC1015)
            {
                value = NumericFormatting.Quoted(string.Empty);
            }

            records.Add(Model1Record.Of(
                "header",
                name,
                ("name", NumericFormatting.Quoted(name)),
                ("type", NumericFormatting.Quoted(type)),
                ("value", value)));
        }
    }

    private static void ExtractLayers(CadDocument document, List<Model1Record> records)
    {
        foreach (Layer layer in document.Layers)
        {
            string name = layer.Name ?? string.Empty;
            Color color = layer.Color;

            // Only the bits a layer owns: frozen (1), frozen in new viewports (2)
            // and locked (4). The xref bits describe the reference, not the layer.
            long flags = (long)(short)layer.Flags & 7;

            // The colour index (DXF 62; its sign is `on`), or -1 for a true colour
            // (DXF 420), which has no index.
            long index = color.IsTrueColor ? -1 : Math.Abs((long)color.Index);
            // Packed 0xRRGGBB from the components, because the packed value ACadSharp
            // exposes (Color.TrueColor) is blue first.
            long trueColor = color.IsTrueColor
                ? ((long)color.R << 16) | ((long)color.G << 8) | color.B
                : -1;

            records.Add(Model1Record.Of(
                "layer",
                name,
                ("name", NumericFormatting.Quoted(name)),
                ("flags", NumericFormatting.Whole(flags)),
                ("color", NumericFormatting.Whole(index)),
                ("true_color", NumericFormatting.Whole(trueColor)),
                ("on", layer.IsOn ? "true" : "false"),
                ("linetype", NumericFormatting.Quoted(layer.LineType?.Name ?? string.Empty)),
                ("lineweight", NumericFormatting.Whole((short)layer.LineWeight)),
                ("plot", layer.PlotFlag ? "true" : "false")));
        }
    }

    private static void ExtractTextStyles(CadDocument document, List<Model1Record> records)
    {
        foreach (TextStyle style in document.TextStyles)
        {
            string name = style.Name ?? string.Empty;

            // An entry with no name cannot be keyed, so it has no record. A STYLE
            // entry without a name is a shape file that only a linetype segment
            // points at, and ACadSharp invents unnamed0, unnamed1... for them.
            if (name.Length == 0 || (style.IsShapeFile && IsInventedName(name)))
            {
                continue;
            }

            long mirror = (long)style.MirrorFlag;
            long flags = (long)(short)style.Flags;

            records.Add(Model1Record.Of(
                "textstyle",
                name,
                ("name", NumericFormatting.Quoted(name)),
                ("font_file", NumericFormatting.Quoted(style.Filename ?? string.Empty)),
                ("big_font_file", NumericFormatting.Quoted(style.BigFontFilename ?? string.Empty)),
                ("height", NumericFormatting.Real(style.Height)),
                ("width_factor", NumericFormatting.Real(style.Width)),
                ("last_height", NumericFormatting.Real(style.LastHeight)),
                ("oblique_angle", NumericFormatting.Real(style.ObliqueAngle)),
                ("backward", (mirror & 2) != 0 ? "true" : "false"),
                ("upside_down", (mirror & 4) != 0 ? "true" : "false"),
                ("is_shape", (flags & 1) != 0 ? "true" : "false"),
                ("is_vertical", (flags & 4) != 0 ? "true" : "false"),
                ("xref_dependent", (flags & 16) != 0 ? "true" : "false"),

                // ACadSharp 3.7.1 has no annotative property on TextStyle. The
                // field is written as false and left out of the comparison
                // (excluded for the oracle in parity/MODEL1.toml).
                ("annotative", "false")));
        }
    }

    /// <summary>
    /// One <c>dictionary</c> record per DICTIONARY (and DICTIONARYWDFLT) reachable
    /// from the root, keyed by the path of entry keys; plus the extension
    /// dictionaries of table entries (<c>table/&lt;kind&gt;/&lt;name&gt;</c>) and of
    /// layouts (<c>layout/&lt;name&gt;</c>). Extension dictionaries of entities are
    /// keyed by handle only and are left out (docs/MODEL_1.md).
    /// </summary>
    private static Dictionary<object, string> ExtractDictionaries(CadDocument document, List<Model1Record> records)
    {
        // The xdata owner of every dictionary and XRECORD the walk reaches:
        // `dictionary/<path>` and `xrecord/<path>`.
        var owners = new Dictionary<object, string>(ReferenceEqualityComparer.Instance);
        if (document.RootDictionary is { } root)
        {
            WalkDictionary(root, string.Empty, records, owners);
        }

        foreach ((string kind, IEnumerable<TableEntry> entries) in TableEntries(document))
        {
            foreach (TableEntry entry in entries)
            {
                if (entry.XDictionary is { } xdic)
                {
                    WalkDictionary(xdic, $"table/{kind}/{entry.Name}", records, owners);
                }
            }
        }

        foreach (Objects.Layout layout in document.Layouts)
        {
            if (layout.XDictionary is { } xdic)
            {
                WalkDictionary(xdic, $"layout/{layout.Name}", records, owners);
            }
        }

        return owners;
    }

    /// <summary>
    /// One <c>xdata</c> record per (owner, application): table entries
    /// (<c>table/&lt;kind&gt;/&lt;name&gt;</c>), layouts, dictionaries and XRECORDs
    /// by their path, and entities by handle.
    /// </summary>
    private static void ExtractXData(
        CadDocument document, List<Model1Record> records, Dictionary<object, string> owners)
    {
        foreach ((string kind, IEnumerable<TableEntry> entries) in TableEntries(document))
        {
            foreach (TableEntry entry in entries)
            {
                AddXData(document, entry, $"table/{kind}/{entry.Name}", records);
            }
        }

        foreach (Objects.Layout layout in document.Layouts)
        {
            AddXData(document, layout, $"layout/{layout.Name}", records);
        }

        foreach ((object obj, string owner) in owners)
        {
            AddXData(document, (CadObject)obj, owner, records);
        }

        foreach (BlockRecord record in document.BlockRecords)
        {
            foreach (Entities.Entity entity in record.Entities)
            {
                // The DXF reader is told to keep unknown entities (see ExtractFile),
                // and one of those is no entity the other readers have.
                if (entity is Entities.UnknownEntity)
                {
                    continue;
                }

                AddXData(document, entity, "entity/" + entity.Handle.ToString("X", CultureInfo.InvariantCulture), records);
            }
        }
    }

    private static void AddXData(CadDocument document, CadObject owner, string ownerPath, List<Model1Record> records)
    {
        foreach (KeyValuePair<AppId, XData.ExtendedData> app in owner.ExtendedData)
        {
            string appid = app.Key.Name ?? string.Empty;
            var values = new List<string>();
            foreach (XData.ExtendedDataRecord record in app.Value.Records)
            {
                int code = (int)record.Code;
                object? raw = record switch
                {
                    XData.ExtendedDataLayer layer => document.GetCadObject<Layer>(layer.Value)?.Name ?? string.Empty,
                    _ => record.RawValue,
                };
                values.Add(CodedValue(code, raw));
            }

            records.Add(Model1Record.Of(
                "xdata",
                ownerPath + "|" + appid,
                ("name", NumericFormatting.Quoted(ownerPath + "|" + appid)),
                ("owner", NumericFormatting.Quoted(ownerPath)),
                ("appid", NumericFormatting.Quoted(appid)),
                ("values", "[" + string.Join(',', values) + "]")));
        }
    }

    /// <summary>
    /// The written form of a value, decided by its group code. The same table as
    /// <c>model1::value_form</c> in the Rust adapters and <c>value_form</c> in
    /// <c>tools/model1.py</c>, written a third time on purpose.
    /// </summary>
    /// <param name="code">The group code.</param>
    /// <returns><c>text</c>, <c>handle</c>, <c>binary</c>, <c>point</c>, <c>real</c> or <c>whole</c>.</returns>
    public static string ValueForm(int code) => code switch
    {
        >= 0 and <= 4 or >= 6 and <= 9 or >= 100 and <= 102 or >= 300 and <= 309
            or >= 410 and <= 419 or >= 430 and <= 439 or >= 470 and <= 479 or 999 => "text",
        1000 or 1002 or 1003 => "text",
        5 or 105 or >= 320 and <= 369 or >= 390 and <= 399 or >= 480 and <= 481 or 1005 => "handle",
        >= 310 and <= 319 or 1004 => "binary",
        >= 10 and <= 37 or >= 1010 and <= 1013 => "point",
        >= 38 and <= 59 or >= 110 and <= 149 or >= 210 and <= 239 or >= 460 and <= 469
            or >= 1040 and <= 1042 => "real",
        >= 60 and <= 99 or >= 150 and <= 179 or >= 270 and <= 299 or >= 370 and <= 389
            or >= 400 and <= 409 or >= 420 and <= 429 or >= 440 and <= 459 or 1070 or 1071 => "whole",
        _ => throw new InvalidOperationException(
            string.Create(CultureInfo.InvariantCulture, $"no written form is defined for group code {code}")),
    };

    /// <summary>One <c>{"code":..,"value":..}</c> pair.</summary>
    private static string CodedValue(int code, object? raw)
    {
        string form = ValueForm(code);
        string value = form switch
        {
            "text" => NumericFormatting.Quoted(NumericFormatting.NormalizeLineEndings(
                raw switch
                {
                    null => string.Empty,
                    char c => c.ToString(),
                    _ => Convert.ToString(raw, CultureInfo.InvariantCulture) ?? string.Empty,
                })),
            "handle" => NumericFormatting.Quoted(raw switch
            {
                CadObject o => o.Handle.ToString("X", CultureInfo.InvariantCulture),
                _ => Convert.ToUInt64(raw, CultureInfo.InvariantCulture).ToString("X", CultureInfo.InvariantCulture),
            }),
            "binary" => NumericFormatting.Quoted(Convert.ToHexString((byte[])raw!)),
            "point" => raw switch
            {
                CSMath.XYZ p => "[" + NumericFormatting.Real(p.X) + "," + NumericFormatting.Real(p.Y) + "," + NumericFormatting.Real(p.Z) + "]",
                CSMath.XY p => "[" + NumericFormatting.Real(p.X) + "," + NumericFormatting.Real(p.Y) + ",0.0]",
                _ => throw new InvalidOperationException($"group code {code}: a {raw?.GetType().Name} is not a point"),
            },
            "real" => NumericFormatting.Real(Convert.ToDouble(raw, CultureInfo.InvariantCulture)),
            _ => NumericFormatting.Whole(raw is bool b ? (b ? 1 : 0) : Convert.ToInt64(raw, CultureInfo.InvariantCulture)),
        };
        return "{\"code\":" + code.ToString(CultureInfo.InvariantCulture) + ",\"value\":" + value + "}";
    }

    /// <summary>The table entries by the kind the owner paths of docs/MODEL_1.md use.</summary>
    internal static IEnumerable<(string Kind, IEnumerable<TableEntry> Entries)> TableEntries(CadDocument document)
    {
        yield return ("layer", document.Layers);
        yield return ("linetype", document.LineTypes);
        yield return ("textstyle", document.TextStyles);
        yield return ("dimstyle", document.DimensionStyles);
        yield return ("block_record", document.BlockRecords);
        yield return ("appid", document.AppIds);
        yield return ("view", document.Views);
        yield return ("vport", document.VPorts);
        yield return ("ucs", document.UCSs);
    }

    private static void WalkDictionary(
        Objects.CadDictionary dictionary, string path, List<Model1Record> records, Dictionary<object, string> owners)
    {
        if (!owners.TryAdd(dictionary, "dictionary/" + path))
        {
            return;
        }

        string[] keys = [.. dictionary.EntryNames.OrderBy(k => k, StringComparer.Ordinal)];
        var entries = new List<string>();
        foreach (string key in keys)
        {
            Objects.NonGraphicalObject entry = dictionary[key] as Objects.NonGraphicalObject
                ?? throw new InvalidOperationException($"{path}/{key}: a dictionary entry that is not an object");
            entries.Add("{\"key\":" + NumericFormatting.Quoted(key)
                + ",\"class\":" + NumericFormatting.Quoted(entry.ObjectName) + "}");
        }

        records.Add(Model1Record.Of(
            "dictionary",
            path,
            ("name", NumericFormatting.Quoted(path)),
            ("hard_owner", dictionary.HardOwnerFlag ? "true" : "false"),
            ("cloning", NumericFormatting.Whole((long)dictionary.ClonningFlags)),
            ("entries", "[" + string.Join(',', entries) + "]"),
            ("entry_count", NumericFormatting.Whole(keys.Length))));

        foreach (string key in keys)
        {
            string childPath = path.Length == 0 ? key : path + "/" + key;
            if (dictionary[key] is Objects.CadDictionary child)
            {
                WalkDictionary(child, childPath, records, owners);
            }
            else if (dictionary[key] is Objects.XRecord xrecord && owners.TryAdd(xrecord, "xrecord/" + childPath))
            {
                records.Add(XRecordRecord(childPath, xrecord));
            }
        }
    }

    /// <summary>Set by <see cref="Extract"/>: whether the document came from a DXF.</summary>
    private static bool s_dxf;

    private static Model1Record XRecordRecord(string path, Objects.XRecord xrecord)
    {
        long cloning = (long)xrecord.CloningFlags;
        var list = xrecord.Entries.ToList();

        // ACadSharp's DXF reader leaves the 280 cloning flag of an XRECORD in its
        // entry list and the flag at 0, where the DWG reader sets the flag and
        // ezdxf and acadrust keep the 280 out of the list. A leading 280 with no
        // flag is the flag; it is moved back so the entries line up
        // (libviprs/acadsharp-rs-tests#133).
        if (s_dxf && cloning == 0 && list.Count > 0 && list[0].Code == 280)
        {
            cloning = Convert.ToInt64(list[0].Value, CultureInfo.InvariantCulture);
            list.RemoveAt(0);
        }

        // The handle-valued entries go last, each group in stored order: ACadSharp
        // lists them last and a DWG keeps them in a stream of their own, so their
        // place among the others is not compared (libviprs/acadsharp-rs-tests#136).
        list = [.. list.Where(e => ValueForm(e.Code) != "handle"), .. list.Where(e => ValueForm(e.Code) == "handle")];

        return Model1Record.Of(
            "xrecord",
            path,
            ("name", NumericFormatting.Quoted(path)),
            ("cloning", NumericFormatting.Whole(cloning)),
            ("entries", "[" + string.Join(',', list.Select(e => CodedValue(e.Code, e.Value))) + "]"));
    }

    private static bool IsInventedName(string name) =>
        name.StartsWith("unnamed", StringComparison.Ordinal)
        && name.Length > "unnamed".Length
        && name["unnamed".Length..].All(char.IsAsciiDigit);

    private static void ExtractLinetypes(CadDocument document, List<Model1Record> records)
    {
        foreach (LineType linetype in document.LineTypes)
        {
            string name = linetype.Name ?? string.Empty;
            var segments = new List<string>();
            foreach (LineType.Segment segment in linetype.Segments)
            {
                segments.Add("{"
                    + "\"length\":" + NumericFormatting.Real(segment.Length)
                    + ",\"flags\":" + NumericFormatting.Whole((long)segment.Flags)
                    + ",\"shape_number\":" + NumericFormatting.Whole(segment.ShapeNumber)
                    + ",\"scale\":" + NumericFormatting.Real(segment.Scale)
                    + ",\"rotation\":" + NumericFormatting.Real(segment.Rotation)
                    + ",\"offset\":[" + NumericFormatting.Real(segment.Offset.X) + "," + NumericFormatting.Real(segment.Offset.Y) + "]"
                    + ",\"text\":" + NumericFormatting.Quoted(NumericFormatting.NormalizeLineEndings(segment.Text))
                    + ",\"style\":" + NumericFormatting.Quoted(segment.Style?.Name ?? string.Empty)
                    + "}");
            }

            records.Add(Model1Record.Of(
                "linetype",
                name,
                ("name", NumericFormatting.Quoted(name)),
                ("description", NumericFormatting.Quoted(NumericFormatting.NormalizeLineEndings(linetype.Description))),
                ("alignment", NumericFormatting.Whole(linetype.Alignment)),
                ("pattern_length", NumericFormatting.Real(linetype.PatternLength)),
                ("segments", "[" + string.Join(',', segments) + "]")));
        }
    }

    /// <summary>
    /// The written form of a header variable, decided by the DXF group code the
    /// variable is stored under. One rule, shared by every emitter.
    /// </summary>
    /// <param name="name">The variable, for the error message.</param>
    /// <param name="codes">Its DXF codes: one for a scalar, two or three for a point.</param>
    /// <returns><c>string</c>, <c>int</c>, <c>double</c>, <c>bool</c>, <c>point2</c> or <c>point3</c>.</returns>
    public static string HeaderType(string name, IReadOnlyList<int> codes)
    {
        ArgumentNullException.ThrowIfNull(codes);
        if (codes.Count == 2)
        {
            return "point2";
        }

        if (codes.Count == 3)
        {
            return "point3";
        }

        int code = codes[0];
        return code switch
        {
            >= 0 and <= 9 => "string",
            >= 40 and <= 59 => "double",
            >= 60 and <= 79 => "int",
            >= 90 and <= 99 => "int",
            >= 100 and <= 102 => "string",
            >= 140 and <= 149 => "double",
            >= 160 and <= 179 => "int",
            >= 270 and <= 289 => "int",
            >= 290 and <= 299 => "bool",
            >= 370 and <= 389 => "int",
            _ => throw new InvalidOperationException(
                string.Create(CultureInfo.InvariantCulture, $"{name}: no written form is defined for DXF code {code}")),
        };
    }

    private static string Point(Dictionary<DxfCode, object> values, int dimension)
    {
        double[] parts = [.. values.Values.Select(v => Convert.ToDouble(v, CultureInfo.InvariantCulture))];
        if (parts.Length != dimension)
        {
            throw new InvalidOperationException("a point variable did not give " + dimension + " components");
        }

        return "[" + string.Join(',', parts.Select(NumericFormatting.Real)) + "]";
    }

    private static string HeaderValue(string name, string type, object? raw)
    {
        switch (type)
        {
            case "string":
                return NumericFormatting.Quoted(AsString(name, raw));
            case "int":
                return NumericFormatting.Whole(AsInt(name, raw));
            case "double":
                return NumericFormatting.Real(AsDouble(name, raw));
            case "bool":
                return AsInt(name, raw) != 0 ? "true" : "false";
            default:
                throw new InvalidOperationException($"{name}: {type} is not a scalar type");
        }
    }

    private static string AsString(string name, object? raw) => raw switch
    {
        null => string.Empty,
        string s => NumericFormatting.NormalizeLineEndings(s),
        ulong handle => handle.ToString("X", CultureInfo.InvariantCulture),
        TableEntry entry => entry.Name ?? string.Empty,
        _ => throw new InvalidOperationException($"{name}: a {raw.GetType().Name} has no string form"),
    };

    private static long AsInt(string name, object? raw) => raw switch
    {
        bool b => b ? 1 : 0,
        char c => c,
        Enum e => Convert.ToInt64(e, CultureInfo.InvariantCulture),
        Color color => color.Index,
        byte or sbyte or short or ushort or int or uint or long => Convert.ToInt64(raw, CultureInfo.InvariantCulture),
        ulong u => checked((long)u),
        _ => throw new InvalidOperationException($"{name}: {raw?.GetType().Name ?? "null"} has no integer form"),
    };

    /// <summary>
    /// The Julian day number the DXF writes a date as, with the day's fraction.
    /// Written here rather than taken from ACadSharp's internal helper, so the
    /// conversion is part of this contract and not of a library detail.
    /// </summary>
    /// <param name="date">The date.</param>
    /// <returns>Days, with the time of day as the fraction.</returns>
    private static double JulianDate(DateTime date)
    {
        int year = date.Year;
        int month = date.Month;
        double fraction = date.Day + (date.Hour / 24.0) + (date.Minute / 1440.0)
            + ((date.Second + (date.Millisecond / 1000.0)) / 86400.0);
        if (month < 3)
        {
            year -= 1;
            month += 12;
        }

        int a = year / 100;
        int b = 2 - a + (a / 4);
        int c = year < 0 ? (int)((365.25 * year) - 0.75) : (int)(365.25 * year);
        int d = (int)(30.6001 * (month + 1));
        return b + c + d + 1720995 + fraction;
    }

    private static double AsDouble(string name, object? raw) => raw switch
    {
        double d => d,
        float f => f,
        DateTime date => JulianDate(date),
        TimeSpan span => span.TotalDays,
        byte or sbyte or short or ushort or int or uint or long or ulong =>
            Convert.ToDouble(raw, CultureInfo.InvariantCulture),
        _ => throw new InvalidOperationException($"{name}: {raw?.GetType().Name ?? "null"} has no double form"),
    };
}
