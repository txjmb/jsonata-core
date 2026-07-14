using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Configs;
using BenchmarkDotNet.Exporters.Json;
using BenchmarkDotNet.Jobs;
using BenchmarkDotNet.Running;
using Jsonata.Net.Native;
using Jsonata.Net.Native.Json;

namespace JsonataFfiBench;

public class FfiBenchmarks
{
    internal static string CorpusPath =
        Environment.GetEnvironmentVariable("JSONATA_CORPUS")
        ?? throw new InvalidOperationException("Set JSONATA_CORPUS=/path/to/corpus.json");

    public static IEnumerable<string> ScenarioNames =>
        CorpusFile.Load(CorpusPath).Select(s => s.Name);

    [ParamsSource(nameof(ScenarioNames))]
    public string Scenario = "";

    private string _expression = "";
    private string _dataJson = "";
    private JsonataCoreExpression _core = null!;
    private JsonataQuery _jnn = null!;
    private JToken _dataToken = null!;

    [GlobalSetup]
    public void Setup()
    {
        var s = CorpusFile.Load(CorpusPath).First(x => x.Name == Scenario);
        _expression = s.Expression;
        _dataJson = s.DataJson;
        _core = JsonataCoreExpression.Compile(_expression);
        _jnn = new JsonataQuery(_expression);
        _dataToken = JToken.Parse(_dataJson);
    }

    [GlobalCleanup]
    public void Cleanup() => _core.Dispose();

    [Benchmark]
    public string? CoreSsCompiled() => _core.Evaluate(_dataJson);

    [Benchmark]
    public string? CoreSsCompileEach()
    {
        using var c = JsonataCoreExpression.Compile(_expression);
        return c.Evaluate(_dataJson);
    }

    [Benchmark]
    public string JnnSsCompiled() => _jnn.Eval(_dataJson, indentResult: false);

    [Benchmark]
    public string JnnSsCompileEach() => new JsonataQuery(_expression).Eval(_dataJson, indentResult: false);

    [Benchmark]
    public JToken JnnHomeTurfCompiled() => _jnn.Eval(_dataToken);
}

public static class Benchmarks
{
    public static int RunAll(string corpusPath)
    {
        Environment.SetEnvironmentVariable("JSONATA_CORPUS", Path.GetFullPath(corpusPath));
        var config = ManualConfig.CreateMinimumViable()
            .AddJob(Job.ShortRun)   // 3 warmup + 3 measurement iterations; spike scale, disclosed in report
            .AddExporter(JsonExporter.Full);
        BenchmarkRunner.Run<FfiBenchmarks>(config);
        return 0;
    }
}
