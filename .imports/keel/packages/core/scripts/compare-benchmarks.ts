/**
 * Benchmark Comparison Script
 *
 * Compares current benchmark results against baseline
 * and reports performance regressions.
 */

import * as fs from 'fs';
import * as path from 'path';

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

interface ComparisonResult {
  name: string;
  baseline: BenchmarkResult;
  current: BenchmarkResult;
  delta: number; // percentage change
  regression: boolean;
}

const REGRESSION_THRESHOLD = 10; // 10% slower is considered a regression

/**
 * Load baseline results from file
 */
function loadBaseline(baselinePath: string): Record<string, BenchmarkResult> {
  try {
    const content = fs.readFileSync(baselinePath, 'utf-8');
    return JSON.parse(content);
  } catch (_error) {
    console.warn(`Warning: Could not load baseline from ${baselinePath}`);
    return {};
  }
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
      if (currentResult.name) {
        results.push(currentResult as BenchmarkResult);
      }
      currentResult = { name: line.slice(0, -1).trim() };
    }

    // Parse metrics
    if (line.includes('Mean:')) {
      const match = line.match(/Mean:\s+([\d.]+)ms/);
      if (match?.[1]) currentResult.mean = parseFloat(match[1]);
    }
    if (line.includes('Median:')) {
      const match = line.match(/Median:\s+([\d.]+)ms/);
      if (match?.[1]) currentResult.median = parseFloat(match[1]);
    }
    if (line.includes('Min:')) {
      const match = line.match(/Min:\s+([\d.]+)ms/);
      if (match?.[1]) currentResult.min = parseFloat(match[1]);
    }
    if (line.includes('Max:')) {
      const match = line.match(/Max:\s+([\d.]+)ms/);
      if (match?.[1]) currentResult.max = parseFloat(match[1]);
    }
    if (line.includes('Std Dev:')) {
      const match = line.match(/Std Dev:\s+([\d.]+)ms/);
      if (match?.[1]) currentResult.stdDev = parseFloat(match[1]);
    }
    if (line.includes('Ops/sec:')) {
      const match = line.match(/Ops\/sec:\s+([\d.]+)/);
      if (match?.[1]) currentResult.ops = parseFloat(match[1]);
    }
    if (line.includes('Samples:')) {
      const match = line.match(/Samples:\s+(\d+)/);
      if (match?.[1]) currentResult.samples = parseInt(match[1], 10);
    }
  }

  // Add last result
  if (currentResult.name) {
    results.push(currentResult as BenchmarkResult);
  }

  return results;
}

/**
 * Compare current results against baseline
 */
function compareResults(
  current: BenchmarkResult[],
  baseline: Record<string, BenchmarkResult>
): ComparisonResult[] {
  const comparisons: ComparisonResult[] = [];

  for (const currentResult of current) {
    const baselineResult = baseline[currentResult.name];
    if (!baselineResult) {
      console.log(`ℹ️  No baseline found for: ${currentResult.name}`);
      continue;
    }

    const delta = ((currentResult.mean - baselineResult.mean) / baselineResult.mean) * 100;
    const regression = delta > REGRESSION_THRESHOLD;

    comparisons.push({
      name: currentResult.name,
      baseline: baselineResult,
      current: currentResult,
      delta,
      regression,
    });
  }

  return comparisons;
}

/**
 * Format comparison report
 */
function formatReport(comparisons: ComparisonResult[]): string {
  const lines: string[] = ['', '='.repeat(100), 'BENCHMARK COMPARISON REPORT', '='.repeat(100), ''];

  let regressionCount = 0;
  let improvementCount = 0;

  for (const comp of comparisons) {
    const symbol = comp.regression ? '🔴' : comp.delta > 0 ? '🟡' : '🟢';
    const change = comp.delta > 0 ? 'slower' : 'faster';
    const absDelta = Math.abs(comp.delta);

    lines.push(`${symbol} ${comp.name}`);
    lines.push(`  Baseline: ${comp.baseline.mean.toFixed(2)}ms`);
    lines.push(`  Current:  ${comp.current.mean.toFixed(2)}ms`);
    lines.push(`  Change:   ${absDelta.toFixed(2)}% ${change}`);

    if (comp.regression) {
      lines.push(`  ⚠️  REGRESSION DETECTED`);
      regressionCount++;
    } else if (comp.delta < -5) {
      lines.push(`  ✅ Performance improved`);
      improvementCount++;
    }

    lines.push('');

    if (comp.regression) regressionCount++;
    if (comp.delta < 0) improvementCount++;
  }

  lines.push('='.repeat(100));
  lines.push(`Summary: ${comparisons.length} tests`);
  lines.push(`  🔴 Regressions: ${regressionCount}`);
  lines.push(`  🟢 Improvements: ${improvementCount}`);
  lines.push(`  🟡 Neutral: ${comparisons.length - regressionCount - improvementCount}`);
  lines.push('='.repeat(100));
  lines.push('');

  return lines.join('\n');
}

/**
 * Format markdown report for GitHub
 */
function formatMarkdownReport(comparisons: ComparisonResult[]): string {
  const lines: string[] = [
    '# Benchmark Comparison Report',
    '',
    '| Test | Baseline | Current | Change | Status |',
    '|------|----------|---------|--------|--------|',
  ];

  let hasRegressions = false;

  for (const comp of comparisons) {
    const symbol = comp.regression ? '🔴' : comp.delta > 0 ? '🟡' : '🟢';
    const change = comp.delta > 0 ? 'slower' : 'faster';
    const absDelta = Math.abs(comp.delta);
    const status = comp.regression ? '⚠️ Regression' : comp.delta < -5 ? '✅ Improved' : 'OK';

    lines.push(
      `| ${comp.name} | ${comp.baseline.mean.toFixed(2)}ms | ${comp.current.mean.toFixed(2)}ms | ${symbol} ${absDelta.toFixed(1)}% ${change} | ${status} |`
    );

    if (comp.regression) hasRegressions = true;
  }

  lines.push('');

  if (hasRegressions) {
    lines.push('## ⚠️ Performance Regressions Detected');
    lines.push('');
    lines.push('Some benchmarks show performance degradation > 10%. Please investigate:');
    lines.push('');

    for (const comp of comparisons) {
      if (comp.regression) {
        lines.push(`- **${comp.name}**: ${Math.abs(comp.delta).toFixed(1)}% slower`);
      }
    }
  }

  return lines.join('\n');
}

/**
 * Main function
 */
async function main() {
  const args = process.argv.slice(2);
  const baselinePath = args[0] || path.join(__dirname, '..', '.benchmarks', 'baseline.json');
  const currentOutputPath = args[1];
  const format = args[2] || 'text'; // text or markdown

  if (!currentOutputPath) {
    console.error(
      'Usage: node compare-benchmarks.js <baseline-path> <current-output-path> [format]'
    );
    process.exit(1);
  }

  // Load baseline
  const baseline = loadBaseline(baselinePath);

  // Load current results
  const currentOutput = fs.readFileSync(currentOutputPath, 'utf-8');
  const currentResults = parseBenchmarkOutput(currentOutput);

  if (currentResults.length === 0) {
    console.error('Error: No benchmark results found in current output');
    process.exit(1);
  }

  // Compare
  const comparisons = compareResults(currentResults, baseline);

  if (comparisons.length === 0) {
    console.log('ℹ️  No comparable benchmarks found (no baseline data)');
    process.exit(0);
  }

  // Format and output report
  const report =
    format === 'markdown' ? formatMarkdownReport(comparisons) : formatReport(comparisons);

  console.log(report);

  // Exit with error if regressions detected
  const hasRegressions = comparisons.some((c) => c.regression);
  process.exit(hasRegressions ? 1 : 0);
}

main().catch((error) => {
  console.error('Error:', error);
  process.exit(1);
});
