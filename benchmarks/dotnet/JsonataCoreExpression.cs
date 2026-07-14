using System.Runtime.InteropServices;

namespace JsonataFfiBench;

public sealed class JsonataCoreException(string message) : Exception(message);

/// <summary>
/// P/Invoke wrapper over the jsonata-core C ABI. Not thread-safe: one
/// instance per thread (engine handles are Rc-based).
/// </summary>
public sealed class JsonataCoreExpression : IDisposable
{
    private IntPtr _handle;

    private JsonataCoreExpression(IntPtr handle) => _handle = handle;

    public static JsonataCoreExpression Compile(string expression)
    {
        IntPtr h = Native.jsonata_compile(expression);
        if (h == IntPtr.Zero)
        {
            throw new JsonataCoreException(TakeLastError() ?? "compile failed");
        }
        return new JsonataCoreExpression(h);
    }

    /// <summary>Result JSON text, or null when the JSONata result is undefined.</summary>
    public string? Evaluate(string dataJson)
    {
        IntPtr r = Native.jsonata_evaluate(_handle, dataJson);
        if (r == IntPtr.Zero)
        {
            string? err = TakeLastError();
            if (err is null)
            {
                return null; // undefined
            }
            throw new JsonataCoreException(err);
        }
        try
        {
            return Marshal.PtrToStringUTF8(r)!;
        }
        finally
        {
            Native.jsonata_free_string(r);
        }
    }

    public static string Version() =>
        Marshal.PtrToStringUTF8(Native.jsonata_version())!; // static string: not freed

    private static string? TakeLastError()
    {
        IntPtr p = Native.jsonata_last_error_message();
        if (p == IntPtr.Zero)
        {
            return null;
        }
        try
        {
            return Marshal.PtrToStringUTF8(p);
        }
        finally
        {
            Native.jsonata_free_string(p);
        }
    }

    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            Native.jsonata_free_expr(_handle);
            _handle = IntPtr.Zero;
        }
    }
}
