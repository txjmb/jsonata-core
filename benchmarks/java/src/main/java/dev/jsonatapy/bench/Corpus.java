package dev.jsonatapy.bench;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

public final class Corpus {
    public record Scenario(String name, String category, String expression, JsonNode data) {}

    private Corpus() {}

    public static List<Scenario> load(Path corpusJson) throws IOException {
        ObjectMapper mapper = new ObjectMapper();
        JsonNode root = mapper.readTree(corpusJson.toFile());
        List<Scenario> out = new ArrayList<>();
        for (JsonNode n : root) {
            out.add(new Scenario(
                    n.get("name").asText(),
                    n.get("category").asText(),
                    n.get("expression").asText(),
                    n.get("data")));
        }
        return out;
    }
}
