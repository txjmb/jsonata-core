using System.Text.Json;
using Jsonata.Net.Native;
using Jsonata.Net.Native.Json;

namespace JsonataFfiBench;

public static class Gate
{
    public static int Run(string corpusPath, string outPath)
    {
        var results = new List<object>();
        int mismatches = 0;
        foreach (Scenario s in CorpusFile.Load(corpusPath))
        {
            string status;
            string detail = "";
            try
            {
                string? ours;
                using (var c = JsonataCoreExpression.Compile(s.Expression))
                {
                    ours = c.Evaluate(s.DataJson);
                }
                var query = new JsonataQuery(s.Expression);
                JToken theirs = query.Eval(JToken.Parse(s.DataJson));
                bool theirsUndefined = theirs.Type == JTokenType.Undefined;

                if (ours is null || theirsUndefined)
                {
                    status = (ours is null && theirsUndefined) ? "match" : "mismatch";
                    if (status == "mismatch")
                    {
                        detail = $"ours={(ours is null ? "undefined" : Trim(ours))} theirs={(theirsUndefined ? "undefined" : Trim(theirs.ToFlatString()))}";
                    }
                }
                else
                {
                    using JsonDocument da = JsonDocument.Parse(ours);
                    using JsonDocument db = JsonDocument.Parse(theirs.ToFlatString());
                    if (JsonCompare.SemanticEquals(da.RootElement, db.RootElement))
                    {
                        status = "match";
                    }
                    else
                    {
                        status = "mismatch";
                        detail = $"ours={Trim(ours)} theirs={Trim(theirs.ToFlatString())}";
                    }
                }
            }
            catch (Exception e)
            {
                status = "error";
                detail = $"{e.GetType().Name}: {e.Message}";
            }
            if (status != "match")
            {
                mismatches++;
            }
            results.Add(new { scenario = s.Name, status, detail });
            Console.WriteLine($"{s.Name,-40} {status}{(detail.Length == 0 ? "" : "  " + detail)}");
        }
        File.WriteAllText(outPath, JsonSerializer.Serialize(results,
            new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"\nGate: {results.Count - mismatches}/{results.Count} match -> {outPath}");
        return 0;
    }

    private static string Trim(string s) => s.Length > 200 ? s[..200] + "..." : s;
}
