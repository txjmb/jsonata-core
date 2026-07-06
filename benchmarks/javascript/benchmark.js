#!/usr/bin/env node
/**
 * JavaScript benchmark for JSONata reference implementation
 *
 * Reads benchmark parameters from stdin as JSON:
 * {
 *   "expression": "user.name",
 *   "data": {"user": {"name": "Alice"}},
 *   "iterations": 1000
 * }
 *
 * Outputs elapsed time in milliseconds to stdout
 */

const jsonata = require('jsonata');

// Read input from stdin
let inputData = '';

process.stdin.on('data', (chunk) => {
    inputData += chunk;
});

process.stdin.on('end', async () => {
    try {
        const params = JSON.parse(inputData);
        const { expression, data, iterations } = params;

        // Compile expression once
        const compiled = jsonata(expression);

        // Warm up (10% of iterations, min 10, max 100)
        const warmupIterations = Math.min(100, Math.max(10, Math.floor(iterations / 10)));
        for (let i = 0; i < warmupIterations; i++) {
            await compiled.evaluate(data);
        }

        // Measure
        // NOTE: evaluate() is async (jsonata-js recurses via real `await` internally,
        // including once per array element) — it MUST be awaited here, or the loop
        // only runs each call up to its first internal suspension point and defers
        // the rest of the real work to microtasks that drain after `end` is captured,
        // making the measured time meaningless for anything but trivial expressions.
        const start = process.hrtime.bigint();
        for (let i = 0; i < iterations; i++) {
            await compiled.evaluate(data);
        }
        const end = process.hrtime.bigint();

        // Calculate elapsed time in milliseconds
        const elapsedNs = Number(end - start);
        const elapsedMs = elapsedNs / 1_000_000;

        // Output just the number
        console.log(elapsedMs.toFixed(2));

    } catch (error) {
        console.error('Benchmark error:', error.message);
        process.exit(1);
    }
});
