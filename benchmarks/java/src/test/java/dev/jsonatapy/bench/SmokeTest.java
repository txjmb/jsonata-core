package dev.jsonatapy.bench;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class SmokeTest {

    @Test
    void versionMatchesCrate() {
        String v = JsonataCore.version();
        assertNotNull(v);
        assertEquals("2.2.4", v);
    }

    @Test
    void simplePath() {
        try (JsonataCore e = JsonataCore.compile("user.name")) {
            assertEquals("\"Alice\"", e.evaluate("{\"user\":{\"name\":\"Alice\"}}"));
        }
    }

    @Test
    void objectResult() {
        try (JsonataCore e = JsonataCore.compile("{\"n\": a + b}")) {
            assertEquals("{\"n\":3}", e.evaluate("{\"a\":1,\"b\":2}"));
        }
    }

    @Test
    void undefinedIsNull() {
        try (JsonataCore e = JsonataCore.compile("missing.path")) {
            assertNull(e.evaluate("{\"a\":1}"));
        }
    }

    @Test
    void parseErrorThrows() {
        assertThrows(JsonataException.class, () -> JsonataCore.compile("a.b["));
    }

    @Test
    void evalErrorThrows() {
        try (JsonataCore e = JsonataCore.compile("a + b")) {
            assertThrows(JsonataException.class, () -> e.evaluate("{\"a\":1,\"b\":\"x\"}"));
        }
    }

    @Test
    void multibyteUtf8() {
        try (JsonataCore e = JsonataCore.compile("$uppercase(name)")) {
            assertEquals("\"HÉLLO ✓ 日本語\"", e.evaluate("{\"name\":\"héllo ✓ 日本語\"}"));
        }
    }
}
