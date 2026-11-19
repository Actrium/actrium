(() => {
  // This fixture only runs in the standalone native harness.
  if (typeof globalThis.__JSCSandboxJSI === 'undefined') return true;
  const runtime = globalThis.__JSCSandboxJSI.createRuntime();
  const context = runtime.createContext();
  const records = { '': 'empty-key-original', protected: 'original' };
  let writes = 0;
  let failures = 0;
  let checks = 0;
  function check(condition, message) {
    checks++;
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
  const propertyRecords = { A: 'original-a', B: 'original-b' };
  let propertyWrites = 0;
  context.inject('writeProperties', (input) => {
    propertyWrites++;
    propertyRecords[input.key] = input.value;
    return JSON.stringify(Object.keys(input));
  });
  const names = JSON.parse(
    context.eval(`writeProperties({
    key: 'A', ['key\\u0000extra']: 'B', value: 'saved',
    ['value\\u0000extra']: 'corrupt', ['\u96ea\ud83c\udf32']: 'unicode'
  })`)
  );
  check(names.includes('key\0extra'), 'Guest field names retain embedded NULs');
  check(names.includes('\u96ea\ud83c\udf32'), 'Guest field names retain Unicode');
  check(propertyRecords.A === 'saved', 'extra fields do not replace the requested key or value');
  check(
    propertyRecords.B === 'original-b',
    'colliding field prefixes do not overwrite another record'
  );
  context.inject('makeProperties', () => ({
    key: 'A',
    'key\0extra': 'B',
    '\u96ea\ud83c\udf32': 'unicode',
  }));
  const returned = JSON.parse(
    context.eval(`(() => {
    const input = makeProperties();
    return JSON.stringify([input.key, input['key\\u0000extra'], input['\u96ea\ud83c\udf32'], Object.keys(input)]);
  })()`)
  );
  check(returned[0] === 'A' && returned[1] === 'B', 'Host field names and values stay distinct');
  check(
    returned[2] === 'unicode' && returned[3].includes('\u96ea\ud83c\udf32'),
    'Host Unicode field names survive'
  );
  context.inject('makePropertyError', () =>
    Object.assign(new Error('message\0tail'), {
      note: 'plain',
      'note\0extra': 'detail',
    })
  );
  const errorFields = JSON.parse(
    context.eval(`(() => {
    const error = makePropertyError();
    return JSON.stringify([error.message, error.note, error['note\\u0000extra']]);
  })()`)
  );
  check(errorFields[0] === 'message\0tail', 'Host error messages retain embedded NULs');
  check(
    errorFields[1] === 'plain' && errorFields[2] === 'detail',
    'Host error field names stay distinct'
  );
  for (const code of [0xd800, 0xdc00]) {
    check(
      context.eval(`(() => {
      try {
        writeProperties({key: 'A', value: 'replacement', [String.fromCharCode(${code})]: true});
        return false;
      } catch (error) { return error instanceof Error && /UTF-8/.test(error.message); }
    })()`),
      'malformed field names fail before the Host write'
    );
  }
  check(propertyWrites === 1, 'malformed field names never reach the Host');
  check(propertyRecords.A === 'saved', 'malformed field names leave existing records intact');
  context.inject('namedGlobal', 'plain');
  context.inject('namedGlobal\0extra', 'separate');
  check(
    context.eval('globalThis.namedGlobal') === 'plain',
    'injected global names do not collide at NUL'
  );
  check(
    context.eval("globalThis['namedGlobal\\u0000extra']") === 'separate' &&
      context.extract('namedGlobal\0extra') === 'separate',
    'global names survive injection and extraction'
  );
  let quotedNameWorks = false;
  try {
    context.inject("quoted'name", 3);
    quotedNameWorks = context.eval('globalThis["quoted\'name"]') === 3;
  } catch (_) {}
  check(quotedNameWorks, 'injected names are data rather than interpolated source');
  context.dispose();
  runtime.dispose();
  console.log(`Storage string regressions: ${checks - failures}/${checks} passed`);
  return failures === 0;
})();
