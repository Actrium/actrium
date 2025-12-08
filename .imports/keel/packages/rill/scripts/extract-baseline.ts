/**
 * Extract Baseline Script
 *
 * Extracts benchmark results from output and saves as baseline JSON
 */

import * as fs from 'fs';

interface BenchmarkResult {
  name: string;
  mean: number;
  median: number;
  min: number;
  max: number;
  stdDev: number;
  ops: number;
  samples: number;
}

/**
 * Parse benchmark output and extract results
 */
function parseBenchmarkOutput(output: string): BenchmarkResult[] {
  const results: BenchmarkResult[] = [];
  const lines = output.split('\n');

  let currentResult: Partial<BenchmarkResult> = {};

  for (const line of lines) {
    // Match result name
    if (line.endsWith(':') && !line.includes('Mean') && !line.includes('Median')) {
      if (currentResult.name && currentResult.mean !== undefined) {
        results.push(currentResult as BenchmarkResult);
      }
      currentResult = { name: line.slice(0, -1).trim() };
    }

    // Parse metrics
    if (line.includes('Mean:')) {
      const match = line.match(/Mean:\s+([\d.]+)ms/);
      if (match && match[1]) currentResult.mean = parseFloat(match[1]);
    }
    if (line.includes('Median:')) {
      const match = line.match(/Median:\s+([\d.]+)ms/);
      if (match && match[1]) currentResult.median = parseFloat(match[1]);
    }
    if (line.includes('Min:')) {
      const match = line.match(/Min:\s+([\d.]+)ms/);
      if (match && match[1]) currentResult.min = parseFloat(match[1]);
    }
    if (line.includes('Max:')) {
      const match = line.match(/Max:\s+([\d.]+)ms/);
      if (match && match[1]) currentResult.max = parseFloat(match[1]);
    }
    if (line.includes('Std Dev:')) {
      const match = line.match(/Std Dev:\s+([\d.]+)ms/);
      if (match && match[1]) currentResult.stdDev = parseFloat(match[1]);
    }
    if (line.includes('Ops/sec:')) {
      const match = line.match(/Ops\/sec:\s+([\d.]+)/);
      if (match && match[1]) currentResult.ops = parseFloat(match[1]);
    }
    if (line.includes('Samples:')) {
      const match = line.match(/Samples:\s+(\d+)/);
      if (match && match[1]) currentResult.samples = parseInt(match[1]);
    }
  }

  // Add last result
  if (currentResult.name && currentResult.mean !== undefined) {
    results.push(currentResult as BenchmarkResult);
  }

  return results;
}

/**
 * Main function
 */
async function main() {
  const args = process.argv.slice(2);
  const inputPath = args[0];
  const outputPath = args[1];

  if (!inputPath || !outputPath) {
    console.error('Usage: node extract-baseline.js <input-path> <output-path>');
    process.exit(1);
  }

  // Read benchmark output
  const output = fs.readFileSync(inputPath, 'utf-8');

  // Parse results
  const results = parseBenchmarkOutput(output);

  if (results.length === 0) {
    console.error('Error: No benchmark results found in output');
    process.exit(1);
  }

  // Convert to baseline format (name -> result mapping)
  const baseline: Record<string, BenchmarkResult> = {};
  for (const result of results) {
    baseline[result.name] = result;
  }

  // Write baseline JSON
  fs.writeFileSync(outputPath, JSON.stringify(baseline, null, 2));

  console.log(`✅ Extracted ${results.length} benchmark results to ${outputPath}`);
}

main().catch((error) => {
  console.error('Error:', error);
  process.exit(1);
});
