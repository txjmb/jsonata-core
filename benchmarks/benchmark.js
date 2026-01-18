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

process.stdin.on('end', () => {
    try {
        const params = JSON.parse(inputData);
        const { expression, data, iterations } = params;

        // Compile expression once
        const compiled = jsonata(expression);

        // Warm up (10% of iterations, min 10, max 100)
        const warmupIterations = Math.min(100, Math.max(10, Math.floor(iterations / 10)));
        for (let i = 0; i < warmupIterations; i++) {
            compiled.evaluate(data);
        }

        // Measure
        const start = process.hrtime.bigint();
        for (let i = 0; i < iterations; i++) {
            compiled.evaluate(data);
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
