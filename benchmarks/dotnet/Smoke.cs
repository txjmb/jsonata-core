namespace JsonataFfiBench;

public static class Smoke
{
    public static int Run()
    {
        int failures = 0;

        void Check(string name, Func<bool> test)
        {
            bool ok;
            string? err = null;
            try { ok = test(); }
            catch (Exception e) { ok = false; err = e.Message; }
            Console.WriteLine($"{(ok ? "PASS" : "FAIL")}  {name}{(err is null ? "" : $"  ({err})")}");
            if (!ok) failures++;
        }

        Check("version non-empty", () => JsonataCoreExpression.Version().Length > 0);
        Check("simple path", () =>
        {
            using var e = JsonataCoreExpression.Compile("user.name");
            return e.Evaluate("{\"user\":{\"name\":\"Alice\"}}") == "\"Alice\"";
        });
        Check("object result", () =>
        {
            using var e = JsonataCoreExpression.Compile("{\"n\": a + b}");
            return e.Evaluate("{\"a\":1,\"b\":2}") == "{\"n\":3}";
        });
        Check("undefined is null", () =>
        {
            using var e = JsonataCoreExpression.Compile("missing.path");
            return e.Evaluate("{\"a\":1}") is null;
        });
        Check("parse error throws", () =>
        {
            try { JsonataCoreExpression.Compile("a.b["); return false; }
            catch (JsonataCoreException) { return true; }
        });
        Check("eval error throws", () =>
        {
            using var e = JsonataCoreExpression.Compile("a + b");
            try { e.Evaluate("{\"a\":1,\"b\":\"x\"}"); return false; }
            catch (JsonataCoreException) { return true; }
        });
        Check("multibyte utf8", () =>
        {
            using var e = JsonataCoreExpression.Compile("$uppercase(name)");
            return e.Evaluate("{\"name\":\"héllo ✓ 日本語\"}") == "\"HÉLLO ✓ 日本語\"";
        });

        Console.WriteLine(failures == 0 ? "SMOKE OK" : $"SMOKE FAILED ({failures})");
        return failures == 0 ? 0 : 1;
    }
}
