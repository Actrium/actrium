/**
 * QuickJS Sandbox Test Suite
 *
 * This test runs in the Host JS runtime and tests the sandbox functionality.
 * Both Host and Guest are JavaScript - this is the real usage scenario.
 */

(function () {
  // Skip test if JSI module is not available (e.g., running in Bun instead of React Native)
  if (typeof globalThis.__QuickJSSandboxJSI === 'undefined') {
    console.log('⊘ Skipping QuickJS native tests (JSI module not available in this environment)');
    return;
  }

  var testsRun = 0;
  var testsPassed = 0;
  var testsFailed = 0;

  function assert(condition, testName, message) {
    testsRun++;
    if (condition) {
      testsPassed++;
      console.log('  ✓ ' + testName);
    } else {
      testsFailed++;
      console.log('  ✗ ' + testName + (message ? ' - ' + message : ''));
    }
  }

  function assertThrows(fn, testName) {
    testsRun++;
    try {
      fn();
      testsFailed++;
      console.log('  ✗ ' + testName + ' - expected exception');
    } catch (e) {
      testsPassed++;
      console.log('  ✓ ' + testName);
    }
  }

  console.log('\n=== QuickJS Sandbox Tests (JavaScript) ===\n');

  // 1. Module Installation
  console.log('1. Module Installation');
  var sandbox = globalThis.__QuickJSSandboxJSI;
  assert(typeof sandbox === 'object', 'Module installed on global');
  assert(typeof sandbox.isAvailable === 'function', 'isAvailable is a function');
  assert(sandbox.isAvailable() === true, 'isAvailable() returns true');

  // 2. Runtime & Context Creation
  console.log('\n2. Runtime & Context Creation');
  var runtime = sandbox.createRuntime();
  assert(typeof runtime === 'object', 'createRuntime() returns object');

  var ctx = runtime.createContext();
  assert(typeof ctx === 'object', 'createContext() returns object');
  assert(typeof ctx.eval === 'function', 'context has eval method');
  assert(typeof ctx.setGlobal === 'function', 'context has setGlobal method');
  assert(typeof ctx.getGlobal === 'function', 'context has getGlobal method');

  // 3. Code Evaluation
  console.log('\n3. Code Evaluation');
  assert(ctx.eval('1 + 2') === 3, "eval('1 + 2') returns 3");
  assert(ctx.eval("'hello' + ' world'") === 'hello world', 'eval string concatenation');

  ctx.eval('function add(a, b) { return a + b; }');
  assert(ctx.eval('add(10, 20)') === 30, 'Define and call function');

  var arr = ctx.eval('[1, 2, 3].map(function(x) { return x * 2; })');
  assert(Array.isArray(arr), 'Array map returns array');
  assert(
    arr.length === 3 && arr[0] === 2 && arr[1] === 4 && arr[2] === 6,
    'Array map values correct'
  );

  // 4. setGlobal / getGlobal
  console.log('\n4. setGlobal / getGlobal');
  ctx.setGlobal('myNumber', 42);
  assert(ctx.getGlobal('myNumber') === 42, 'setGlobal/getGlobal number');
  assert(ctx.eval('myNumber') === 42, 'eval can access setGlobal value');

  ctx.setGlobal('myString', 'test string');
  assert(ctx.getGlobal('myString') === 'test string', 'setGlobal/getGlobal string');

  ctx.setGlobal('myObj', { x: 10, y: 20 });
  assert(ctx.eval('myObj.x + myObj.y') === 30, 'setGlobal/getGlobal object');

  ctx.setGlobal('myArray', [1, 2, 3]);
  assert(
    ctx.eval('myArray.reduce(function(a, b) { return a + b; }, 0)') === 6,
    'setGlobal/getGlobal array'
  );

  // 5. Host Function Callbacks (the key feature!)
  console.log('\n5. Host Function Callbacks');

  var callbackInvoked = false;
  var receivedValue = 0;
  ctx.setGlobal('hostCallback', function (val) {
    callbackInvoked = true;
    receivedValue = val;
    return val * 2;
  });

  var result = ctx.eval('hostCallback(21)');
  assert(
    callbackInvoked && receivedValue === 21 && result === 42,
    'Host function called from sandbox'
  );

  ctx.setGlobal('multiArg', function (a, b, c, d, e) {
    return a + b + c + d + e;
  });
  assert(ctx.eval('multiArg(1, 2, 3, 4, 5)') === 15, 'Host function with multiple args');

  var receivedObj = null;
  ctx.setGlobal('objCallback', function (obj) {
    receivedObj = obj;
    return obj.name === 'Alice' && obj.age === 30;
  });
  assert(
    ctx.eval("objCallback({ name: 'Alice', age: 30 })") === true,
    'Host function receiving object'
  );

  // 6. Bidirectional: Guest function called by Host
  console.log('\n6. Guest Functions Callable from Host');
  ctx.eval('function guestAdd(a, b) { return a + b; }');
  var guestAdd = ctx.getGlobal('guestAdd');
  assert(typeof guestAdd === 'function', 'Can get guest function');
  assert(guestAdd(5, 7) === 12, 'Can call guest function from host');

  // 7. Sandbox Isolation
  console.log('\n7. Sandbox Isolation');

  globalThis.hostGlobal = 100;
  ctx.eval('hostGlobal = 999'); // Guest tries to modify
  assert(globalThis.hostGlobal === 100, "Sandbox doesn't modify host globals");

  var ctx2 = runtime.createContext();
  ctx.eval("var sandboxVar = 'context1'");
  ctx2.eval("var sandboxVar = 'context2'");
  assert(ctx.eval('sandboxVar') === 'context1', 'Context 1 isolated');
  assert(ctx2.eval('sandboxVar') === 'context2', 'Context 2 isolated');
  ctx2.dispose();

  // 8. Error Handling
  console.log('\n8. Error Handling');
  assertThrows(function () {
    ctx.eval('function { invalid');
  }, 'Syntax error is caught');
  assertThrows(function () {
    ctx.eval("throw new Error('test error')");
  }, 'Runtime error is caught');

  // 9. Disposal
  console.log('\n9. Disposal');
  var tempCtx = runtime.createContext();
  tempCtx.eval('var x = 1');
  tempCtx.dispose();
  assertThrows(function () {
    tempCtx.eval('x + 1');
  }, 'Disposed context throws error');

  // 10. Primitive Types - null, undefined, boolean
  console.log('\n10. Primitive Types');
  ctx.setGlobal('myNull', null);
  assert(ctx.getGlobal('myNull') === null, 'setGlobal/getGlobal null');
  assert(ctx.eval('myNull === null') === true, 'eval null comparison');

  ctx.setGlobal('myUndef', undefined);
  assert(ctx.getGlobal('myUndef') === undefined, 'setGlobal/getGlobal undefined');
  assert(ctx.eval('myUndef === undefined') === true, 'eval undefined comparison');

  ctx.setGlobal('myTrue', true);
  ctx.setGlobal('myFalse', false);
  assert(ctx.getGlobal('myTrue') === true, 'setGlobal/getGlobal true');
  assert(ctx.getGlobal('myFalse') === false, 'setGlobal/getGlobal false');
  assert(ctx.eval('myTrue && !myFalse') === true, 'eval boolean logic');

  // 11. Special Numbers - NaN, Infinity
  console.log('\n11. Special Numbers');
  ctx.setGlobal('myNaN', NaN);
  assert(Number.isNaN(ctx.getGlobal('myNaN')), 'setGlobal/getGlobal NaN');
  assert(ctx.eval('Number.isNaN(myNaN)') === true, 'eval NaN check');

  ctx.setGlobal('myInf', Infinity);
  assert(ctx.getGlobal('myInf') === Infinity, 'setGlobal/getGlobal Infinity');
  assert(ctx.eval('myInf === Infinity') === true, 'eval Infinity comparison');

  ctx.setGlobal('myNegInf', -Infinity);
  assert(ctx.getGlobal('myNegInf') === -Infinity, 'setGlobal/getGlobal -Infinity');

  // 12. BigInt (if supported)
  console.log('\n12. BigInt');
  try {
    // Test BigInt operations within sandbox (not crossing boundary)
    assert(ctx.eval('typeof BigInt') === 'function', 'BigInt function exists in sandbox');
    assert(ctx.eval('typeof BigInt(123)') === 'bigint', 'BigInt type in sandbox');
    assert(
      ctx.eval('BigInt(10) + BigInt(20) === BigInt(30)') === true,
      'BigInt arithmetic in sandbox'
    );
    // Test that BigInt preserves precision beyond Number.MAX_SAFE_INTEGER
    assert(
      ctx.eval("BigInt('9007199254740993') > BigInt('9007199254740992')") === true,
      'BigInt large number comparison'
    );
  } catch (e) {
    console.log('  ⚠ BigInt tests skipped: ' + e.message);
  }

  // 13. Nested Objects
  console.log('\n13. Nested Objects');
  var nested = {
    level1: {
      level2: {
        level3: {
          value: 'deep',
        },
      },
    },
  };
  ctx.setGlobal('nested', nested);
  assert(ctx.eval('nested.level1.level2.level3.value') === 'deep', 'Deep nested object access');

  var deepResult = ctx.getGlobal('nested');
  assert(deepResult.level1.level2.level3.value === 'deep', 'getGlobal deep nested object');

  // 14. Arrays with Mixed Types
  console.log('\n14. Arrays with Mixed Types');
  var mixedArray = [1, 'two', true, null, undefined, { x: 10 }, [1, 2, 3]];
  ctx.setGlobal('mixedArray', mixedArray);
  assert(ctx.eval('mixedArray[0]') === 1, 'Mixed array - number');
  assert(ctx.eval('mixedArray[1]') === 'two', 'Mixed array - string');
  assert(ctx.eval('mixedArray[2]') === true, 'Mixed array - boolean');
  assert(ctx.eval('mixedArray[3] === null') === true, 'Mixed array - null');
  assert(ctx.eval('mixedArray[4] === undefined') === true, 'Mixed array - undefined');
  assert(ctx.eval('mixedArray[5].x') === 10, 'Mixed array - object');
  assert(ctx.eval('mixedArray[6][1]') === 2, 'Mixed array - nested array');

  // 15. Functions as Values
  console.log('\n15. Functions as Values');
  ctx.eval("var objWithFunc = { greet: function(name) { return 'Hello, ' + name; } }");
  var objWithFunc = ctx.getGlobal('objWithFunc');
  assert(typeof objWithFunc.greet === 'function', 'Object with function property');
  assert(objWithFunc.greet('World') === 'Hello, World', 'Call function from object');

  ctx.eval('var higherOrder = function(f, x) { return f(x * 2); }');
  var higherOrder = ctx.getGlobal('higherOrder');
  assert(
    higherOrder(function (n) {
      return n + 1;
    }, 5) === 11,
    'Higher-order function'
  );

  // 16. Host Callback with Various Return Types
  console.log('\n16. Host Callback Return Types');
  ctx.setGlobal('returnNull', function () {
    return null;
  });
  assert(ctx.eval('returnNull() === null') === true, 'Host callback returning null');

  ctx.setGlobal('returnUndefined', function () {
    return undefined;
  });
  assert(ctx.eval('returnUndefined() === undefined') === true, 'Host callback returning undefined');

  ctx.setGlobal('returnBool', function (b) {
    return !b;
  });
  assert(ctx.eval('returnBool(false)') === true, 'Host callback returning boolean');

  ctx.setGlobal('returnArray', function () {
    return [1, 2, 3];
  });
  var retArr = ctx.eval('returnArray()');
  assert(Array.isArray(retArr) && retArr.length === 3, 'Host callback returning array');

  ctx.setGlobal('returnObject', function () {
    return { a: 1, b: 2 };
  });
  assert(ctx.eval('returnObject().a + returnObject().b') === 3, 'Host callback returning object');

  // 17. Guest Callback Receiving Various Types
  console.log('\n17. Guest Callback Arguments');
  ctx.eval('function identity(x) { return x; }');
  var identity = ctx.getGlobal('identity');
  assert(identity(null) === null, 'Guest function receives null');
  assert(identity(undefined) === undefined, 'Guest function receives undefined');
  assert(identity(true) === true, 'Guest function receives boolean');
  assert(identity(3.14) === 3.14, 'Guest function receives float');
  var identObj = identity({ test: 123 });
  assert(identObj && identObj.test === 123, 'Guest function receives object');

  // 18. Error Objects
  console.log('\n18. Error Objects');
  try {
    ctx.eval("throw new TypeError('custom type error')");
    assert(false, 'Should have thrown');
  } catch (e) {
    assert(
      e.message && e.message.indexOf('custom type error') !== -1,
      'TypeError caught with message'
    );
  }

  try {
    ctx.eval("throw new RangeError('out of range')");
    assert(false, 'Should have thrown');
  } catch (e) {
    assert(e.message && e.message.indexOf('out of range') !== -1, 'RangeError caught with message');
  }

  // 19. Date Objects
  console.log('\n19. Date Objects');
  ctx.eval('var myDate = new Date(2024, 0, 15)');
  var dateObj = ctx.getGlobal('myDate');
  assert(dateObj instanceof Date || typeof dateObj === 'object', 'Date object retrieved');
  assert(ctx.eval('myDate.getFullYear()') === 2024, 'Date getFullYear');
  assert(ctx.eval('myDate.getMonth()') === 0, 'Date getMonth');
  assert(ctx.eval('myDate.getDate()') === 15, 'Date getDate');

  // 20. RegExp Objects
  console.log('\n20. RegExp Objects');
  ctx.eval('var myRegex = /hello\\s+world/i');
  assert(ctx.eval("myRegex.test('Hello World')") === true, 'RegExp test match');
  assert(ctx.eval("myRegex.test('goodbye')") === false, 'RegExp test no match');
  assert(ctx.eval("'Hello   World'.match(myRegex) !== null") === true, 'RegExp match');

  // 21. Map and Set (if supported)
  console.log('\n21. Map and Set');
  try {
    ctx.eval("var myMap = new Map(); myMap.set('key1', 'value1'); myMap.set('key2', 42);");
    assert(ctx.eval("myMap.get('key1')") === 'value1', 'Map get string value');
    assert(ctx.eval("myMap.get('key2')") === 42, 'Map get number value');
    assert(ctx.eval('myMap.size') === 2, 'Map size');

    ctx.eval('var mySet = new Set([1, 2, 3, 2, 1]);');
    assert(ctx.eval('mySet.size') === 3, 'Set size (duplicates removed)');
    assert(ctx.eval('mySet.has(2)') === true, 'Set has');
    assert(ctx.eval('mySet.has(5)') === false, 'Set has (not present)');
  } catch (e) {
    console.log('  ⚠ Map/Set tests skipped: ' + e.message);
  }

  // 22. TypedArrays (if supported)
  console.log('\n22. TypedArrays');
  try {
    ctx.eval('var uint8 = new Uint8Array([1, 2, 3, 255])');
    assert(ctx.eval('uint8.length') === 4, 'Uint8Array length');
    assert(ctx.eval('uint8[0]') === 1, 'Uint8Array element access');
    assert(ctx.eval('uint8[3]') === 255, 'Uint8Array max value');

    ctx.eval('var int32 = new Int32Array([100, -100, 2147483647])');
    assert(ctx.eval('int32.length') === 3, 'Int32Array length');
    assert(ctx.eval('int32[1]') === -100, 'Int32Array negative value');

    ctx.eval('var float64 = new Float64Array([1.5, 2.5, 3.14159])');
    assert(
      ctx.eval('float64[2]') > 3.14 && ctx.eval('float64[2]') < 3.15,
      'Float64Array precision'
    );
  } catch (e) {
    console.log('  ⚠ TypedArray tests skipped: ' + e.message);
  }

  // 23. Symbol (if supported)
  console.log('\n23. Symbol');
  try {
    ctx.eval("var sym1 = Symbol('test'); var sym2 = Symbol('test');");
    assert(ctx.eval('sym1 !== sym2') === true, 'Symbols are unique');
    assert(ctx.eval('typeof sym1') === 'symbol', 'typeof Symbol');

    ctx.eval("var symObj = {}; symObj[sym1] = 'symbol value';");
    assert(ctx.eval('symObj[sym1]') === 'symbol value', 'Symbol as object key');
  } catch (e) {
    console.log('  ⚠ Symbol tests skipped: ' + e.message);
  }

  // 24. Promise (basic, if supported)
  console.log('\n24. Promise (basic)');
  try {
    ctx.eval('var promiseResolved = false; var promiseValue = 0;');
    ctx.eval(
      'Promise.resolve(42).then(function(v) { promiseResolved = true; promiseValue = v; });'
    );
    // Note: Promises are async, so we can't directly test the resolved value synchronously
    // But we can test that Promise exists and basic operations work
    assert(ctx.eval('typeof Promise') === 'function', 'Promise exists');
    assert(ctx.eval('typeof Promise.resolve') === 'function', 'Promise.resolve exists');
    assert(ctx.eval('typeof Promise.reject') === 'function', 'Promise.reject exists');
  } catch (e) {
    console.log('  ⚠ Promise tests skipped: ' + e.message);
  }

  // 25. JSON serialization
  console.log('\n25. JSON Serialization');
  var jsonObj = { name: 'test', count: 42, active: true, items: [1, 2, 3] };
  ctx.setGlobal('jsonObj', jsonObj);
  var jsonStr = ctx.eval('JSON.stringify(jsonObj)');
  assert(typeof jsonStr === 'string', 'JSON.stringify returns string');
  var parsed = JSON.parse(jsonStr);
  assert(parsed.name === 'test' && parsed.count === 42, 'JSON roundtrip preserves data');

  ctx.eval('var parsedInGuest = JSON.parse(\'{"x": 100, "y": 200}\')');
  assert(ctx.eval('parsedInGuest.x + parsedInGuest.y') === 300, 'JSON.parse in guest');

  // 26. Object with prototype methods
  console.log('\n26. Object Methods');
  ctx.eval('var arr = [3, 1, 4, 1, 5, 9, 2, 6]');
  assert(ctx.eval("arr.sort().join(',')") === '1,1,2,3,4,5,6,9', 'Array sort and join');
  assert(ctx.eval('arr.filter(function(x) { return x > 3; }).length') === 4, 'Array filter');
  assert(ctx.eval('arr.find(function(x) { return x > 5; })') === 6, 'Array find');
  assert(ctx.eval('arr.every(function(x) { return x > 0; })') === true, 'Array every');
  assert(ctx.eval('arr.some(function(x) { return x > 8; })') === true, 'Array some');

  // 27. String methods
  console.log('\n27. String Methods');
  ctx.setGlobal('testStr', '  Hello, World!  ');
  assert(ctx.eval('testStr.trim()') === 'Hello, World!', 'String trim');
  assert(ctx.eval('testStr.toUpperCase().trim()') === 'HELLO, WORLD!', 'String toUpperCase');
  assert(ctx.eval('testStr.toLowerCase().trim()') === 'hello, world!', 'String toLowerCase');
  assert(ctx.eval("testStr.includes('World')") === true, 'String includes');
  assert(ctx.eval("testStr.indexOf('World')") === 9, 'String indexOf');
  assert(ctx.eval("testStr.split(',').length") === 2, 'String split');

  // 28. Math operations
  console.log('\n28. Math Operations');
  assert(ctx.eval('Math.floor(3.7)') === 3, 'Math.floor');
  assert(ctx.eval('Math.ceil(3.2)') === 4, 'Math.ceil');
  assert(ctx.eval('Math.round(3.5)') === 4, 'Math.round');
  assert(ctx.eval('Math.abs(-5)') === 5, 'Math.abs');
  assert(ctx.eval('Math.max(1, 5, 3)') === 5, 'Math.max');
  assert(ctx.eval('Math.min(1, 5, 3)') === 1, 'Math.min');
  assert(Math.abs(ctx.eval('Math.sqrt(2)') - 1.414) < 0.01, 'Math.sqrt');

  // 29. Empty and edge cases
  console.log('\n29. Edge Cases');
  ctx.setGlobal('emptyStr', '');
  assert(ctx.getGlobal('emptyStr') === '', 'Empty string');
  ctx.setGlobal('emptyArr', []);
  var emptyArr = ctx.getGlobal('emptyArr');
  assert(Array.isArray(emptyArr) && emptyArr.length === 0, 'Empty array');
  ctx.setGlobal('emptyObj', {});
  var emptyObj = ctx.getGlobal('emptyObj');
  assert(typeof emptyObj === 'object' && Object.keys(emptyObj).length === 0, 'Empty object');

  assert(ctx.eval('0') === 0, 'Zero');
  assert(ctx.eval('-0') === 0, 'Negative zero'); // Note: -0 === 0 in JS
  assert(ctx.eval("''") === '', 'Empty string literal');

  // Summary
  console.log('\n=== Test Summary ===');
  console.log('Total: ' + testsRun);
  console.log('Passed: ' + testsPassed);
  console.log('Failed: ' + testsFailed);

  if (testsFailed === 0) {
    console.log('\n✓ ALL TESTS PASSED\n');
  } else {
    console.log('\n✗ SOME TESTS FAILED\n');
  }

  // Return result for the C++ runner
  return testsFailed === 0;
})();
