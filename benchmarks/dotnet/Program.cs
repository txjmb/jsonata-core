namespace JsonataFfiBench;

public static class Program
{
    public static int Main(string[] args)
    {
        return args switch
        {
            ["smoke"] => Smoke.Run(),
            ["gate", var corpus, var outPath] => Gate.Run(corpus, outPath),
            ["bench", var corpus] => Benchmarks.RunAll(corpus),
            _ => Usage(),
        };
    }

    private static int Usage()
    {
        Console.Error.WriteLine("usage: JsonataFfiBench smoke | gate <corpus.json> <out.json> | bench <corpus.json>");
        return 2;
    }
}
