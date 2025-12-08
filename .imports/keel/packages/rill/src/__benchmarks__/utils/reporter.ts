/**
 * Benchmark reporter utilities
 */

import type { BenchmarkResult } from './benchmark';
import { compareWithBaseline, formatResult } from './benchmark';

export interface BenchmarkReport {
  timestamp: string;
  results: BenchmarkResult[];
  baseline?: Record<string, BenchmarkResult>;
  summary: {
    totalTests: number;
    regressions: number;
    improvements: number;
  };
}

/**
 * Generate benchmark report
 */
export function generateReport(
  results: BenchmarkResult[],
  baseline?: Record<string, BenchmarkResult>
): BenchmarkReport {
  let regressions = 0;
  let improvements = 0;

  results.forEach((result) => {
    const baselineResult = baseline?.[result.name];
    if (baselineResult) {
      const comparison = compareWithBaseline(result, baselineResult);
      if (comparison.regression) regressions++;
      if (comparison.faster && Math.abs(comparison.delta) > 5) improvements++;
    }
  });

  return {
    timestamp: new Date().toISOString(),
    results,
    baseline,
    summary: {
      totalTests: results.length,
      regressions,
      improvements,
    },
  };
}

/**
 * Format report as text
 */
export function formatReport(report: BenchmarkReport): string {
  const lines: string[] = [
    '='.repeat(80),
    'BENCHMARK REPORT',
    `Timestamp: ${report.timestamp}`,
    `Total Tests: ${report.summary.totalTests}`,
    `Regressions: ${report.summary.regressions}`,
    `Improvements: ${report.summary.improvements}`,
    '='.repeat(80),
    '',
  ];

  report.results.forEach((result) => {
    lines.push(formatResult(result));

    if (report.baseline) {
      const baselineResult = report.baseline[result.name];
      if (baselineResult) {
        const comparison = compareWithBaseline(result, baselineResult);
        const symbol = comparison.faster ? '✓' : '✗';
        const change = comparison.faster ? 'faster' : 'slower';
        const color = comparison.regression ? '🔴' : comparison.faster ? '🟢' : '🟡';

        lines.push(
          `  ${color} ${symbol} ${Math.abs(comparison.delta).toFixed(2)}% ${change} than baseline`
        );
      }
    }

    lines.push('');
  });

  return lines.join('\n');
}

/**
 * Format report as JSON
 */
export function formatReportJSON(report: BenchmarkReport): string {
  return JSON.stringify(report, null, 2);
}

/**
 * Format report as Markdown
 */
export function formatReportMarkdown(report: BenchmarkReport): string {
  const lines: string[] = [
    '# Benchmark Report',
    '',
    `**Timestamp:** ${report.timestamp}`,
    `**Total Tests:** ${report.summary.totalTests}`,
    `**Regressions:** ${report.summary.regressions}`,
    `**Improvements:** ${report.summary.improvements}`,
    '',
    '## Results',
    '',
    '| Test | Mean (ms) | Median (ms) | Ops/sec | vs Baseline |',
    '|------|-----------|-------------|---------|-------------|',
  ];

  report.results.forEach((result) => {
    let comparison = '-';
    if (report.baseline) {
      const baselineResult = report.baseline[result.name];
      if (baselineResult) {
        const comp = compareWithBaseline(result, baselineResult);
        const symbol = comp.faster ? '✓' : '✗';
        const emoji = comp.regression ? '🔴' : comp.faster ? '🟢' : '🟡';
        comparison = `${emoji} ${symbol} ${Math.abs(comp.delta).toFixed(1)}%`;
      }
    }

    lines.push(
      `| ${result.name} | ${result.mean.toFixed(2)} | ${result.median.toFixed(2)} | ${result.ops.toFixed(0)} | ${comparison} |`
    );
  });

  return lines.join('\n');
}
