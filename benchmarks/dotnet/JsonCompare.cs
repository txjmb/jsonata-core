using System.Text.Json;

namespace JsonataFfiBench;

/// <summary>
/// Semantic JSON equality: numbers compared as double with relative
/// tolerance, object key order ignored, array order significant.
/// (Mirror of the Java JsonCompare.)
/// </summary>
public static class JsonCompare
{
    public static bool SemanticEquals(JsonElement a, JsonElement b)
    {
        if (a.ValueKind == JsonValueKind.Number && b.ValueKind == JsonValueKind.Number)
        {
            double x = a.GetDouble();
            double y = b.GetDouble();
            return Math.Abs(x - y) <= 1e-9 * Math.Max(1.0, Math.Max(Math.Abs(x), Math.Abs(y)));
        }
        if (a.ValueKind != b.ValueKind)
        {
            return false;
        }
        switch (a.ValueKind)
        {
            case JsonValueKind.Array:
            {
                if (a.GetArrayLength() != b.GetArrayLength())
                {
                    return false;
                }
                using var ea = a.EnumerateArray().GetEnumerator();
                using var eb = b.EnumerateArray().GetEnumerator();
                while (ea.MoveNext() && eb.MoveNext())
                {
                    if (!SemanticEquals(ea.Current, eb.Current))
                    {
                        return false;
                    }
                }
                return true;
            }
            case JsonValueKind.Object:
            {
                var bProps = new Dictionary<string, JsonElement>();
                foreach (var p in b.EnumerateObject())
                {
                    bProps[p.Name] = p.Value;
                }
                int aCount = 0;
                foreach (var p in a.EnumerateObject())
                {
                    aCount++;
                    if (!bProps.TryGetValue(p.Name, out JsonElement bv) || !SemanticEquals(p.Value, bv))
                    {
                        return false;
                    }
                }
                return aCount == bProps.Count;
            }
            case JsonValueKind.String:
                return a.GetString() == b.GetString();
            default: // True/False/Null
                return true;
        }
    }
}
