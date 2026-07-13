package dev.jsonatapy.bench;

import com.dashjoin.jsonata.Jsonata;
import com.dashjoin.jsonata.json.Json;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Pre-benchmark correctness gate: for every corpus scenario, jsonata-core
 * (via FFI, string->string) and dashjoin/jsonata-java must produce
 * semantically equal results. Mismatches don't fail the run — they are
 * recorded so the report excludes and flags those scenarios.
 *
 * Usage: CorrectnessGate <corpus.json> <gate_results.json>
 */
public final class CorrectnessGate {

    public static void main(String[] args) throws Exception {
        Path corpusPath = Path.of(args[0]);
        Path outPath = Path.of(args[1]);
        ObjectMapper mapper = new ObjectMapper();

        // How does dashjoin represent explicit JSON null? (null or sentinel)
        Object nullRep = Jsonata.jsonata("null").evaluate(new HashMap<String, Object>());

        List<ObjectNode> results = new ArrayList<>();
        int mismatches = 0;
        for (Corpus.Scenario s : Corpus.load(corpusPath)) {
            String status;
            String detail = "";
            try {
                String dataJson = mapper.writeValueAsString(s.data());
                String ours;
                try (JsonataCore c = JsonataCore.compile(s.expression())) {
                    ours = c.evaluate(dataJson);
                }
                Object theirsRaw = Jsonata.jsonata(s.expression()).evaluate(Json.parseJson(dataJson));
                Object theirs = normalize(theirsRaw, nullRep);

                if (ours == null || theirs == null) {
                    // ours==null is undefined; theirs==null is undefined (or
                    // explicit null when nullRep==null — lenient by design,
                    // disclosed in the report).
                    status = (ours == null && theirs == null) ? "match" : "mismatch";
                    if (status.equals("mismatch")) {
                        detail = "ours=" + ours + " theirs=" + theirs;
                    }
                } else {
                    JsonNode ourNode = mapper.readTree(ours);
                    JsonNode theirNode = mapper.valueToTree(theirs);
                    if (JsonCompare.semanticEquals(ourNode, theirNode)) {
                        status = "match";
                    } else {
                        status = "mismatch";
                        detail = "ours=" + trim(ours) + " theirs=" + trim(mapper.writeValueAsString(theirs));
                    }
                }
            } catch (Exception e) {
                status = "error";
                detail = e.getClass().getSimpleName() + ": " + e.getMessage();
            }
            if (!status.equals("match")) {
                mismatches++;
            }
            ObjectNode r = mapper.createObjectNode();
            r.put("scenario", s.name());
            r.put("status", status);
            r.put("detail", detail);
            results.add(r);
            System.out.printf("%-40s %s%s%n", s.name(), status, detail.isEmpty() ? "" : "  " + detail);
        }
        mapper.writerWithDefaultPrettyPrinter().writeValue(outPath.toFile(), results);
        System.out.printf("%nGate: %d/%d match -> %s%n", results.size() - mismatches, results.size(), outPath);
    }

    /** Recursively replace dashjoin's JSON-null representation with Java null
     *  and rebuild containers so Jackson can serialize them. */
    private static Object normalize(Object v, Object nullRep) {
        if (v == null) {
            return null;
        }
        if (nullRep != null && v.equals(nullRep)) {
            return null;
        }
        if (v instanceof Map<?, ?> m) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : m.entrySet()) {
                out.put(String.valueOf(e.getKey()), normalize(e.getValue(), nullRep));
            }
            return out;
        }
        if (v instanceof List<?> l) {
            List<Object> out = new ArrayList<>(l.size());
            for (Object o : l) {
                out.add(normalize(o, nullRep));
            }
            return out;
        }
        return v;
    }

    private static String trim(String s) {
        return s.length() > 200 ? s.substring(0, 200) + "..." : s;
    }
}
