using ACadSharp.Entities;
using CSMath;

namespace ACadSharp.Generator.Drawings;

/// <summary>
/// An entity kind the canonical stream has no record for, between two that it has.
/// </summary>
/// <remarks>
/// A MESH. The line and the circle either side of it make the claim about
/// "does not vanish" checkable: two records, one warning, and the warning names
/// the kind.
/// </remarks>
public static class UnsupportedEntity
{
    public const string Name = "regression_unsupported_entity";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Drawings/UnsupportedEntity.cs",
        doc =>
        {
            var layer = Writer.Layer(doc);
            doc.Entities.Add(new Line(new XYZ(0, 0, 0), new XYZ(10, 0, 0)) { Layer = layer });
            var mesh = new Mesh { Layer = layer };
            mesh.Vertices.Add(new XYZ(0, 0, 0));
            mesh.Vertices.Add(new XYZ(10, 0, 0));
            mesh.Vertices.Add(new XYZ(10, 10, 4));
            mesh.Faces.Add(new int[] { 0, 1, 2 });
            doc.Entities.Add(mesh);
            doc.Entities.Add(new Circle { Center = new XYZ(20, 5, 0), Radius = 3, Layer = layer });
        });
}
