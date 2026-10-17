using ACadSharp;
using ACadSharp.Generator;

return Cli.Run(args);

internal static class Cli
{
    private const string Usage =
        "usage: ACadSharp.Generator list\n"
        + "       ACadSharp.Generator generate <out-dir> [name...]   write drawings, print name<TAB>version<TAB>bytes<TAB>sha256\n"
        + "       ACadSharp.Generator check <dir> [name...]          rewrite in memory, exit 1 if a file in <dir> differs\n"
        + "       ACadSharp.Generator selftest                       write one drawing twice, an interval apart, exit 1 if the bytes differ\n";

    public static int Run(string[] args)
    {
        if (args.Length == 0)
        {
            Console.Error.Write(Usage);
            return 64;
        }

        try
        {
            return args[0] switch
            {
                "list" => List(),
                "generate" when args.Length >= 2 => Generate(args[1], args[2..], write: true),
                "check" when args.Length >= 2 => Generate(args[1], args[2..], write: false),
                "selftest" => SelfTest(),
                _ => Bad(),
            };
        }
        catch (Exception e) when (e is InvalidOperationException or IOException or ArgumentException)
        {
            Console.Error.WriteLine($"generator: {e.Message}");
            return 2;
        }
    }

    private static int Bad()
    {
        Console.Error.Write(Usage);
        return 64;
    }

    private static int List()
    {
        foreach (Drawing d in Catalog.All)
        {
            Console.Out.Write($"{d.Name}\t{d.Version}\t{d.Source}\n");
        }

        return 0;
    }

    private static int Generate(string dir, string[] names, bool write)
    {
        var chosen = new List<Drawing>();
        foreach (string name in names)
        {
            chosen.Add(Catalog.All.FirstOrDefault(d => d.Name == name)
                ?? throw new InvalidOperationException($"no drawing called {name}"));
        }

        if (names.Length == 0)
        {
            chosen.AddRange(Catalog.All);
        }

        if (write)
        {
            Directory.CreateDirectory(dir);
        }

        int bad = 0;
        foreach (Drawing d in chosen)
        {
            byte[] bytes = Writer.Render(d);
            string path = Path.Combine(dir, d.Name + ".dwg");
            if (write)
            {
                File.WriteAllBytes(path, bytes);
            }
            else if (!File.Exists(path) || !File.ReadAllBytes(path).AsSpan().SequenceEqual(bytes))
            {
                Console.Error.WriteLine($"differs: {path}");
                bad++;
            }

            Console.Out.Write($"{d.Name}\t{d.Version}\t{bytes.Length}\t{Writer.Sha256(bytes)}\n");
        }

        return bad == 0 ? 0 : 1;
    }

    private static int SelfTest()
    {
        byte[] first = Writer.Render(Catalog.SelfTest);
        Thread.Sleep(1500);
        byte[] second = Writer.Render(Catalog.SelfTest);
        bool same = first.AsSpan().SequenceEqual(second);
        Console.Out.Write($"selftest {(same ? "identical" : "DIFFERENT")} {first.Length} {second.Length} {Writer.Sha256(first)}\n");

        // The control: with the stamps left as DwgWriter makes them, the same two
        // writes must differ. If they did not, "identical" above would prove nothing.
        byte[] a = WriteUnpinned();
        Thread.Sleep(1500);
        byte[] b = WriteUnpinned();
        bool unpinnedDiffers = !a.AsSpan().SequenceEqual(b);
        Console.Out.Write($"control unpinned {(unpinnedDiffers ? "differs" : "IDENTICAL")}\n");
        return same && unpinnedDiffers ? 0 : 1;
    }

    private static byte[] WriteUnpinned()
    {
        var doc = new CadDocument();
        doc.Header.Version = ACadVersion.AC1032;
        doc.Entities.Add(new ACadSharp.Entities.Line(new CSMath.XYZ(0, 0, 0), new CSMath.XYZ(1, 1, 0)));
        using var stream = new MemoryStream();
        using (var w = new ACadSharp.IO.DwgWriter(stream, doc))
        {
            w.Write();
        }

        return stream.ToArray();
    }
}
