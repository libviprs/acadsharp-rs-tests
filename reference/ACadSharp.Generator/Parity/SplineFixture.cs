using ACadSharp.Entities;
using ACadSharp.Tables;
using CSMath;

namespace ACadSharp.Generator.Parity;

/// <summary>A cubic with six control points and a quadratic with four, knot vectors written out.</summary>
public static class SplineFixture
{
    public const string Name = "parity_spline";

    public static readonly Drawing Drawing = new(
        Name,
        ACadVersion.AC1032,
        "reference/ACadSharp.Generator/Parity/SplineFixture.cs",
        Build);

    private static Spline Make(CadDocument doc, int degree, double[][] points, double[] knots)
    {
        var s = new Spline { Degree = degree, Layer = Writer.Layer(doc) };
        foreach (double[] p in points)
        {
            s.ControlPoints.Add(new XYZ(p[0], p[1], p[2]));
        }

        foreach (double k in knots)
        {
            s.Knots.Add(k);
        }

        return s;
    }

    private static void Build(CadDocument doc)
    {
        doc.Entities.Add(Make(doc, 3,
            [[0, 0, 0], [2, 5, 0], [5, 6, 0], [8, 2, 0], [11, 4, 0], [14, 0, 0]],
            [0, 0, 0, 0, 1, 2, 3, 3, 3, 3]));
        doc.Entities.Add(Make(doc, 2,
            [[0, 20, 0], [5, 30, 1], [10, 20, 2], [15, 30, 3]],
            [0, 0, 0, 1, 2, 2, 2]));
    }
}
