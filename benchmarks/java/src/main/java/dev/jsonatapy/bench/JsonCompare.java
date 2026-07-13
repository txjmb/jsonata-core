package dev.jsonatapy.bench;

import com.fasterxml.jackson.databind.JsonNode;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Set;

/**
 * Semantic JSON equality: numbers compared as double with relative tolerance
 * (JSONata numbers are IEEE doubles; implementations differ on int-vs-double
 * node types), object key order ignored, array order significant.
 */
public final class JsonCompare {
    private JsonCompare() {}

    public static boolean semanticEquals(JsonNode a, JsonNode b) {
        if (a == null || b == null) {
            return a == b;
        }
        if (a.isNumber() && b.isNumber()) {
            double x = a.doubleValue();
            double y = b.doubleValue();
            return Math.abs(x - y) <= 1e-9 * Math.max(1.0, Math.max(Math.abs(x), Math.abs(y)));
        }
        if (a.isArray() && b.isArray()) {
            if (a.size() != b.size()) {
                return false;
            }
            for (int i = 0; i < a.size(); i++) {
                if (!semanticEquals(a.get(i), b.get(i))) {
                    return false;
                }
            }
            return true;
        }
        if (a.isObject() && b.isObject()) {
            if (a.size() != b.size()) {
                return false;
            }
            Set<String> keys = new HashSet<>();
            for (Iterator<String> it = a.fieldNames(); it.hasNext(); ) {
                keys.add(it.next());
            }
            for (String k : keys) {
                if (!b.has(k) || !semanticEquals(a.get(k), b.get(k))) {
                    return false;
                }
            }
            return true;
        }
        return a.equals(b);
    }
}
