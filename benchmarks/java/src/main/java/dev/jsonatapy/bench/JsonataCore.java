package dev.jsonatapy.bench;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;

/**
 * FFM wrapper over the jsonata-core C ABI (spike scope: loads the .so from
 * -Djsonata.core.lib or $JSONATA_CORE_LIB; no resource bundling).
 * Handles are not thread-safe: one instance per thread.
 */
public final class JsonataCore implements AutoCloseable {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final MethodHandle COMPILE;
    private static final MethodHandle EVALUATE;
    private static final MethodHandle FREE_EXPR;
    private static final MethodHandle FREE_STRING;
    private static final MethodHandle LAST_ERROR;
    private static final MethodHandle VERSION;

    static {
        String libPath = System.getProperty("jsonata.core.lib");
        if (libPath == null || libPath.isEmpty()) {
            libPath = System.getenv("JSONATA_CORE_LIB");
        }
        if (libPath == null || libPath.isEmpty()) {
            throw new IllegalStateException(
                    "Set -Djsonata.core.lib=/path/to/libjsonata_core.so or $JSONATA_CORE_LIB");
        }
        SymbolLookup lib = SymbolLookup.libraryLookup(Path.of(libPath), Arena.global());
        COMPILE = LINKER.downcallHandle(lib.find("jsonata_compile").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        EVALUATE = LINKER.downcallHandle(lib.find("jsonata_evaluate").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        FREE_EXPR = LINKER.downcallHandle(lib.find("jsonata_free_expr").orElseThrow(),
                FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        FREE_STRING = LINKER.downcallHandle(lib.find("jsonata_free_string").orElseThrow(),
                FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        LAST_ERROR = LINKER.downcallHandle(lib.find("jsonata_last_error_message").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
        VERSION = LINKER.downcallHandle(lib.find("jsonata_version").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
    }

    private MemorySegment handle;

    private JsonataCore(MemorySegment handle) {
        this.handle = handle;
    }

    public static JsonataCore compile(String expression) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment h = (MemorySegment) COMPILE.invokeExact(arena.allocateFrom(expression));
            if (h.equals(MemorySegment.NULL)) {
                throw new JsonataException(String.valueOf(takeLastError()));
            }
            return new JsonataCore(h);
        } catch (JsonataException e) {
            throw e;
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    /** Result JSON text, or {@code null} when the JSONata result is undefined. */
    public String evaluate(String dataJson) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment r = (MemorySegment) EVALUATE.invokeExact(handle, arena.allocateFrom(dataJson));
            if (r.equals(MemorySegment.NULL)) {
                String err = takeLastError();
                if (err == null) {
                    return null; // undefined
                }
                throw new JsonataException(err);
            }
            return readAndFree(r);
        } catch (JsonataException e) {
            throw e;
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    public static String version() {
        try {
            MemorySegment v = (MemorySegment) VERSION.invokeExact();
            return v.reinterpret(Long.MAX_VALUE).getString(0); // static string: do NOT free
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    private static String takeLastError() {
        try {
            MemorySegment p = (MemorySegment) LAST_ERROR.invokeExact();
            if (p.equals(MemorySegment.NULL)) {
                return null;
            }
            return readAndFree(p);
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    private static String readAndFree(MemorySegment cstr) throws Throwable {
        String s = cstr.reinterpret(Long.MAX_VALUE).getString(0);
        FREE_STRING.invokeExact(cstr);
        return s;
    }

    @Override
    public void close() {
        if (handle != null) {
            try {
                FREE_EXPR.invokeExact(handle);
            } catch (Throwable t) {
                throw new RuntimeException(t);
            }
            handle = null;
        }
    }
}
