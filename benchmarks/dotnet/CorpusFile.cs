using System.Text.Json;

namespace JsonataFfiBench;

public sealed record Scenario(string Name, string Category, string Expression, string DataJson);

public static class CorpusFile
{
    public static List<Scenario> Load(string path)
    {
        using JsonDocument doc = JsonDocument.Parse(File.ReadAllText(path));
        var result = new List<Scenario>();
        foreach (JsonElement e in doc.RootElement.EnumerateArray())
        {
            result.Add(new Scenario(
                e.GetProperty("name").GetString()!,
                e.GetProperty("category").GetString()!,
                e.GetProperty("expression").GetString()!,
                e.GetProperty("data").GetRawText()));
        }
        return result;
    }
}
