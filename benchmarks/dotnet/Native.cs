using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace JsonataFfiBench;

// Raw C ABI. String RETURNS from the library cross as IntPtr and are freed
// with jsonata_free_string — never as marshalled string returns, whose
// unmarshaller frees with the CoTaskMem allocator (wrong allocator, UB).
// String ARGUMENTS use Utf8 marshalling (runtime-owned temp buffer, safe).
internal static partial class Native
{
    private const string Lib = "jsonata_core";

    [ModuleInitializer]
    internal static void Init()
    {
        NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, (name, _, _) =>
            name == Lib
                ? NativeLibrary.Load(
                    Environment.GetEnvironmentVariable("JSONATA_CORE_LIB")
                    ?? throw new InvalidOperationException(
                        "Set JSONATA_CORE_LIB=/path/to/libjsonata_core.so"))
                : IntPtr.Zero);
    }

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr jsonata_compile(string exprUtf8);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr jsonata_evaluate(IntPtr expr, string jsonUtf8);

    [LibraryImport(Lib)]
    internal static partial void jsonata_free_expr(IntPtr expr);

    [LibraryImport(Lib)]
    internal static partial void jsonata_free_string(IntPtr s);

    [LibraryImport(Lib)]
    internal static partial IntPtr jsonata_last_error_message();

    [LibraryImport(Lib)]
    internal static partial IntPtr jsonata_version();
}
