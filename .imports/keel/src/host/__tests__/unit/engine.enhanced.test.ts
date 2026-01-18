import { beforeAll, describe, expect, it, mock } from 'bun:test';
import { Engine } from '../../engine';
import { createMockJSEngineProvider } from '../test-utils';

function buildBundle(code: string) {
  // Use var instead of const to avoid redeclaration error since React is already injected as global
  return `
    var _React = require('react');
    var _jsx = require('react/jsx-runtime');
    var _rill = require('rill/sdk');
    ${code}
  `;
}

// Silent logger for tests - prevents expected error logs from cluttering output
const silentLogger = {
  log: () => {},
  warn: () => {},
  error: () => {},
};

describe('Engine enhanced behaviors', () => {
  let provider: ReturnType<typeof createMockJSEngineProvider>;
  beforeAll(() => {
    provider = createMockJSEngineProvider();
  });
  it('enforces require whitelist', async () => {
    const engine = new Engine({
      quickjs: provider,
      debug: false,
      logger: silentLogger,
      requireWhitelist: ['react'],
    });
    const src = "require('react-native')";
    await expect(engine.loadBundle(src)).rejects.toThrow(/Unsupported require/);
  });

  it('reports metrics via onMetric', async () => {
    const onMetric = mock();
    const engine = new Engine({ quickjs: provider, debug: false, logger: silentLogger, onMetric });
    const src = buildBundle(`console.log('hello')`);
    await engine.loadBundle(src);
    // Should have at least these metrics
    const names = onMetric.mock.calls.map((c) => c[0]);
    expect(names).toContain('engine.initializeRuntime');
    expect(names).toContain('engine.executeBundle');
  });

  it('throws ExecutionError for runtime errors', async () => {
    const engine = new Engine({ quickjs: provider, debug: false, logger: silentLogger });
    const src = buildBundle(`throw new Error('boom')`);
    await expect(engine.loadBundle(src)).rejects.toThrow('boom');
  });
});
