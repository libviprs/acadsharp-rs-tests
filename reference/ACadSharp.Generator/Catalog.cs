using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator;

/// <summary>Every drawing the generator writes, by name.</summary>
public static class Catalog
{
    /// <summary>
    /// Written by <c>selftest</c> only, never committed: one line, enough to prove the
    /// writer is deterministic without the catalog having to contain anything.
    /// </summary>
    public static readonly Drawing SelfTest = new(
        "generator_selftest",
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Catalog.cs",
        doc => doc.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(1, 1, 0)) { Layer = Writer.Layer(doc) }));

    /// <summary>The committed fixtures, in the order they are listed.</summary>
    public static readonly IReadOnlyList<Drawing> All = [];
}
