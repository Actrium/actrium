/**
 * Test Guest: useEffect with Timers
 *
 * Minimal React component that uses useEffect with setTimeout.
 * Built to run in WASM QuickJS sandbox.
 */

// This code will be evaluated in the QuickJS sandbox
// React and ReactReconciler are injected as globals

(() => {
  const React = globalThis.React;
  const { useState, useEffect, useRef } = React;

  // Results collector for test verification
  globalThis.testResults = {
    events: [],
    log: function (event, data) {
      this.events.push({ event, data, timestamp: Date.now() });
      if (globalThis.__sendEventToHost) {
        globalThis.__sendEventToHost(event, data);
      }
    },
  };

  // Test 1: Basic useEffect with setTimeout
  function TestAsyncStateUpdate() {
    const [data, setData] = useState('loading');

    useEffect(() => {
      globalThis.testResults.log('EFFECT_START', { component: 'TestAsyncStateUpdate' });

      const fetchData = async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        setData('loaded');
        globalThis.testResults.log('DATA_LOADED', { data: 'loaded' });
      };
      fetchData();

      return () => {
        globalThis.testResults.log('EFFECT_CLEANUP', { component: 'TestAsyncStateUpdate' });
      };
    }, []);

    return React.createElement(
      'View',
      { testID: 'async-test' },
      React.createElement('Text', { testID: 'status' }, data)
    );
  }

  // Test 2: useEffect with setInterval and cleanup
  function TestIntervalWithCleanup() {
    const [count, setCount] = useState(0);
    const countRef = useRef(0);

    useEffect(() => {
      globalThis.testResults.log('INTERVAL_START', {});

      const interval = setInterval(() => {
        countRef.current++;
        setCount(countRef.current);
        globalThis.testResults.log('TICK', { count: countRef.current });

        if (countRef.current >= 3) {
          clearInterval(interval);
          globalThis.testResults.log('INTERVAL_DONE', { finalCount: countRef.current });
        }
      }, 30);

      return () => {
        clearInterval(interval);
        globalThis.testResults.log('INTERVAL_CLEANUP', {});
      };
    }, []);

    return React.createElement(
      'View',
      { testID: 'interval-test' },
      React.createElement('Text', { testID: 'count' }, String(count))
    );
  }

  // Test 3: Promise.all in useEffect
  function TestPromiseAll() {
    const [results, setResults] = useState([]);

    useEffect(() => {
      globalThis.testResults.log('PROMISE_ALL_START', {});

      const fetchAll = async () => {
        const promises = [
          new Promise((resolve) => setTimeout(() => resolve('A'), 30)),
          new Promise((resolve) => setTimeout(() => resolve('B'), 20)),
          new Promise((resolve) => setTimeout(() => resolve('C'), 10)),
        ];

        const values = await Promise.all(promises);
        setResults(values);
        globalThis.testResults.log('PROMISE_ALL_DONE', { values });
      };
      fetchAll();
    }, []);

    return React.createElement(
      'View',
      { testID: 'promise-all-test' },
      React.createElement('Text', { testID: 'results' }, results.join(','))
    );
  }

  // Test 4: Promise.race in useEffect
  function TestPromiseRace() {
    const [winner, setWinner] = useState(null);

    useEffect(() => {
      globalThis.testResults.log('PROMISE_RACE_START', {});

      const race = async () => {
        const result = await Promise.race([
          new Promise((resolve) => setTimeout(() => resolve('slow'), 100)),
          new Promise((resolve) => setTimeout(() => resolve('fast'), 10)),
        ]);
        setWinner(result);
        globalThis.testResults.log('PROMISE_RACE_DONE', { winner: result });
      };
      race();
    }, []);

    return React.createElement(
      'View',
      { testID: 'promise-race-test' },
      React.createElement('Text', { testID: 'winner' }, winner || 'racing...')
    );
  }

  // Test 5: Effect dependency tracking
  function TestEffectDeps() {
    const [value, setValue] = useState(0);
    const [effectRunCount, setEffectRunCount] = useState(0);

    useEffect(() => {
      setEffectRunCount((c) => c + 1);
      globalThis.testResults.log('EFFECT_WITH_DEPS', { value, runCount: effectRunCount + 1 });
    }, [value]);

    // Auto-increment value twice
    useEffect(() => {
      if (value < 2) {
        setTimeout(() => setValue((v) => v + 1), 20);
      } else {
        globalThis.testResults.log('DEPS_TEST_DONE', {
          finalValue: value,
          totalRuns: effectRunCount,
        });
      }
    }, [value, effectRunCount]);

    return React.createElement(
      'View',
      { testID: 'deps-test' },
      React.createElement('Text', { testID: 'value' }, String(value)),
      React.createElement('Text', { testID: 'runs' }, String(effectRunCount))
    );
  }

  // Main App - runs selected test
  function App({ testName }) {
    globalThis.testResults.log('APP_RENDER', { testName });

    switch (testName) {
      case 'async-state':
        return React.createElement(TestAsyncStateUpdate);
      case 'interval-cleanup':
        return React.createElement(TestIntervalWithCleanup);
      case 'promise-all':
        return React.createElement(TestPromiseAll);
      case 'promise-race':
        return React.createElement(TestPromiseRace);
      case 'effect-deps':
        return React.createElement(TestEffectDeps);
      default:
        return React.createElement(
          'View',
          {},
          React.createElement('Text', {}, `Unknown test: ${testName}`)
        );
    }
  }

  // Export for test runner
  globalThis.TestApp = App;
  globalThis.testResults.log('GUEST_LOADED', {});
})();
