package dev.jsonatapy.bench;

import java.nio.file.Path;
import java.util.List;
import org.openjdk.jmh.results.format.ResultFormatType;
import org.openjdk.jmh.runner.Runner;
import org.openjdk.jmh.runner.options.Options;
import org.openjdk.jmh.runner.options.OptionsBuilder;
import org.openjdk.jmh.runner.options.TimeValue;

/** Usage: BenchRunner <corpus.json> <libjsonata_core.so> <out.json> */
public final class BenchRunner {

    public static void main(String[] args) throws Exception {
        String corpus = args[0];
        String lib = args[1];
        String out = args[2];
        List<Corpus.Scenario> scenarios = Corpus.load(Path.of(corpus));
        Options opt = new OptionsBuilder()
                .include(FfiBenchmark.class.getName())
                .param("scenario", scenarios.stream().map(Corpus.Scenario::name).toArray(String[]::new))
                .forks(1)
                .warmupIterations(3)
                .warmupTime(TimeValue.seconds(1))
                .measurementIterations(5)
                .measurementTime(TimeValue.seconds(1))
                .jvmArgsAppend(
                        "--enable-native-access=ALL-UNNAMED",
                        "-Djsonata.core.lib=" + lib,
                        "-Djsonata.corpus=" + corpus)
                .resultFormat(ResultFormatType.JSON)
                .result(out)
                .build();
        new Runner(opt).run();
    }
}
