(() => {
  // This fixture only runs in the standalone native harness.
  if (typeof globalThis.__JSCSandboxJSI === 'undefined') return true;
  const runtime = globalThis.__JSCSandboxJSI.createRuntime();
  const context = runtime.createContext();
  const records = { '': 'empty-key-original', protected: 'original' };
  let writes = 0;
  let failures = 0;
  function check(condition, message) {
    if (!condition) {
      failures++;
      console.error(`FAIL: ${message}`);
    }
  }
  context.inject('write', (input) => {
    writes++;
    records[input.key] = input.value;
  });
  for (const code of [0xd800, 0xdc00]) {
    check(
      context.eval(`
      (() => {
        try {
          write({key: String.fromCharCode(${code}), value: 'replacement'});
          return false;
        } catch (error) {
          return error instanceof Error && /UTF-8/.test(error.message);
        }
      })()
    `),
      'malformed key fails visibly before the Host write'
    );
    check(
      context.eval(`
      (() => {
        try {
          write({key: 'protected', value: String.fromCharCode(${code})});
          return false;
        } catch (error) {
          return error instanceof Error && /UTF-8/.test(error.message);
        }
      })()
    `),
      'malformed value fails visibly before the Host write'
    );
  }
  check(writes === 0, 'malformed strings never reach the Host');
  check(records[''] === 'empty-key-original', 'empty-key data remains intact');
  check(records.protected === 'original', 'existing value remains intact');
  context.eval("write({key: '', value: ''}); write({key: 'unicode', value: '\\ud83c\\udf32'});");
  check(records[''] === '', 'valid empty strings remain supported');
  check(records.unicode === '\ud83c\udf32', 'valid surrogate pairs remain supported');
  context.dispose();
  runtime.dispose();
  console.log(`Storage string regressions: ${9 - failures}/9 passed`);
  return failures === 0;
})();
