package dev.jsonatapy.bench;

import com.dashjoin.jsonata.Jsonata;
import com.dashjoin.jsonata.json.Json;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.file.Path;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;

@State(Scope.Benchmark)
@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
public class FfiBenchmark {

    @Param({"SET_BY_RUNNER"})
    public String scenario;

    String expression;
    String dataJson;
    Object dashjoinData;
    JsonataCore coreExpr;
    Jsonata dashjoinExpr;
    ObjectMapper mapper;

    @Setup
    public void setup() throws IOException {
        Corpus.Scenario s = Corpus.load(Path.of(System.getProperty("jsonata.corpus"))).stream()
                .filter(x -> x.name().equals(scenario))
                .findFirst()
                .orElseThrow(() -> new IllegalArgumentException("unknown scenario: " + scenario));
        mapper = new ObjectMapper();
        expression = s.expression();
        dataJson = mapper.writeValueAsString(s.data());
        coreExpr = JsonataCore.compile(expression);
        dashjoinExpr = Jsonata.jsonata(expression);
        dashjoinData = Json.parseJson(dataJson);
    }

    @TearDown
    public void tearDown() {
        coreExpr.close();
    }

    @Benchmark
    public String coreSsCompiled() {
        return coreExpr.evaluate(dataJson);
    }

    @Benchmark
    public String coreSsCompileEach() {
        try (JsonataCore c = JsonataCore.compile(expression)) {
            return c.evaluate(dataJson);
        }
    }

    @Benchmark
    public String dashjoinSsCompiled() throws IOException {
        Object data = Json.parseJson(dataJson);
        Object result = dashjoinExpr.evaluate(data);
        return mapper.writeValueAsString(result);
    }

    @Benchmark
    public String dashjoinSsCompileEach() throws IOException {
        Jsonata e = Jsonata.jsonata(expression);
        Object data = Json.parseJson(dataJson);
        return mapper.writeValueAsString(e.evaluate(data));
    }

    @Benchmark
    public Object dashjoinHomeTurfCompiled() {
        return dashjoinExpr.evaluate(dashjoinData);
    }
}
