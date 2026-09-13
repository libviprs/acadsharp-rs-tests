using System.Globalization;

using ACadSharp.Entities;
using ACadSharp.Reference.Canonical;
using ACadSharp.Tables;

namespace ACadSharp.Reference.Cad;

/// <summary>
/// The recursion policy for block references, and the transform an insert implies.
/// </summary>
/// <remarks>
/// <para>
/// Blocks are the differential case that matters most, because they are where a
/// decoder can be wrong in a way that still looks like a drawing. A block that
/// references itself, directly or through a chain, is a real thing to find in
/// the wild and is also the shortest path from "reads a DWG" to "hangs
/// forever", so both the cycle and the depth are refusals with a warning rather
/// than exceptions or a stack overflow.
/// </para>
/// <para>
/// Refusals are warnings, not silence. Geometry that was not drawn because the
/// resolver stopped has to be visible in the canonical file, or the two sides
/// of the differential could stop at different depths and still compare equal.
/// </para>
/// </remarks>
/// <param name="warnings">Where refusals are reported.</param>
/// <param name="maxDepth">How deep nesting may go before it is refused.</param>
public sealed class BlockResolver(WarningCollector warnings, int maxDepth = BlockResolver.DefaultMaxDepth)
{
    /// <summary>
    /// Default nesting limit.
    /// </summary>
    /// <remarks>
    /// Sixteen because real drawings nest a title block inside a sheet inside a
    /// border and stop, and anything past that is far more likely to be a cycle
    /// the handle check did not catch than a drawing somebody made on purpose.
    /// </remarks>
    public const int DefaultMaxDepth = 16;

    /// <summary>
    /// Ceiling on the copies one MInsert array may expand to.
    /// </summary>
    /// <remarks>
    /// A corrupt row or column count is a two-byte field that can ask for
    /// billions of copies, and expanding it is indistinguishable from a hang.
    /// </remarks>
    public const int MaxArrayCopies = 1024;

    private readonly WarningCollector _warnings = warnings;

    // Keyed on the block record itself, not on its handle. A document built in
    // memory has not been assigned handles yet, so every one of its blocks
    // would answer zero and the first nested block would read as a cycle.
    // Identity is also what "already on the path being expanded" actually
    // means, so this is the narrower claim as well as the safer one.
    private readonly HashSet<BlockRecord> _active =
        new(ReferenceEqualityComparer.Instance);

    /// <summary>How deep nesting may go.</summary>
    public int MaxDepth { get; } = maxDepth;

    /// <summary>
    /// Decides whether a block reference may be expanded, and records the reason when it may not.
    /// </summary>
    /// <param name="block">The block record the insert points at.</param>
    /// <param name="depth">Current nesting depth, zero at the top of a view.</param>
    /// <returns><see langword="true"/> when the caller should recurse.</returns>
    public bool TryEnter(BlockRecord? block, int depth)
    {
        if (block is null)
        {
            _warnings.Add(WarningCategory.MissingReference, nameof(Insert), "block_record_missing");
            return false;
        }

        if (depth >= MaxDepth)
        {
            _warnings.Add(
                WarningCategory.MalformedGeometry,
                nameof(Insert),
                string.Create(CultureInfo.InvariantCulture, $"recursion_limit_{MaxDepth}"));
            return false;
        }

        if (!_active.Add(block))
        {
            // Self-reference and longer cycles look the same from here: the
            // block is already on the path being expanded.
            _warnings.Add(WarningCategory.MalformedGeometry, nameof(Insert), "cyclic_block_reference");
            return false;
        }

        return true;
    }

    /// <summary>Pops a block off the active path.</summary>
    /// <param name="block">The block that was entered.</param>
    public void Leave(BlockRecord block)
    {
        ArgumentNullException.ThrowIfNull(block);
        _active.Remove(block);
    }

    /// <summary>
    /// The map from a block's own coordinates into the space containing the insert.
    /// </summary>
    /// <param name="insert">The block reference.</param>
    /// <param name="column">Column index within an MInsert array, zero for a plain insert.</param>
    /// <param name="row">Row index within an MInsert array, zero for a plain insert.</param>
    /// <returns>The composed map.</returns>
    /// <remarks>
    /// Base point, then scale, then the array offset, then rotation about the
    /// insert's own Z, then the insertion point, then the insert's OCS. Written
    /// in that order because it is the order DXF defines and the order a
    /// reviewer can check against the specification, not because it composes
    /// most conveniently.
    /// </remarks>
    public static CadTransform InsertTransform(Insert insert, int column = 0, int row = 0)
    {
        ArgumentNullException.ThrowIfNull(insert);

        Point3 basePoint = insert.Block?.BlockEntity is { } blockEntity
            ? new Point3(blockEntity.BasePoint.X, blockEntity.BasePoint.Y, blockEntity.BasePoint.Z)
            : Point3.Zero;

        CadTransform t = CadTransform.Translation(
            new Point3(-basePoint.X, -basePoint.Y, -basePoint.Z));
        t = CadTransform.Compose(
            CadTransform.Scale(insert.XScale, insert.YScale, insert.ZScale), t);

        if (column != 0 || row != 0)
        {
            t = CadTransform.Compose(
                CadTransform.Translation(new Point3(
                    column * insert.ColumnSpacing,
                    row * insert.RowSpacing,
                    0.0)),
                t);
        }

        t = CadTransform.Compose(CadTransform.RotationZ(insert.Rotation), t);
        t = CadTransform.Compose(
            CadTransform.Translation(new Point3(
                insert.InsertPoint.X, insert.InsertPoint.Y, insert.InsertPoint.Z)),
            t);

        return CadTransform.Compose(
            CadTransform.ArbitraryAxis(
                new Point3(insert.Normal.X, insert.Normal.Y, insert.Normal.Z)),
            t);
    }

    /// <summary>
    /// The (column, row) pairs an insert expands to, refusing an implausible array.
    /// </summary>
    /// <param name="insert">The block reference.</param>
    /// <returns>At least one pair, always including (0, 0).</returns>
    public IReadOnlyList<(int Column, int Row)> ArrayCells(Insert insert)
    {
        ArgumentNullException.ThrowIfNull(insert);

        int columns = Math.Max(1, (int)insert.ColumnCount);
        int rows = Math.Max(1, (int)insert.RowCount);
        long total = (long)columns * rows;
        if (total > MaxArrayCopies)
        {
            _warnings.Add(
                WarningCategory.MalformedGeometry,
                nameof(Insert),
                string.Create(CultureInfo.InvariantCulture, $"array_too_large_{MaxArrayCopies}"));
            return [(0, 0)];
        }

        var cells = new List<(int Column, int Row)>((int)total);
        for (int row = 0; row < rows; row++)
        {
            for (int column = 0; column < columns; column++)
            {
                cells.Add((column, row));
            }
        }

        return cells;
    }
}
