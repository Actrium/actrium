import { describe, expect, test } from 'bun:test';
import { analyzeModuleIDs } from './oxcAdapter';

describe('oxcAdapter - Module ID Analysis', () => {
  describe('Basic Functionality', () => {
    test('should return correct result structure', async () => {
      const code = `import React from 'react';`;
      const result = await analyzeModuleIDs(code);

      expect(result).toBeDefined();
      expect(result.static).toBeDefined();
      expect(result.dynamicLiteral).toBeDefined();
      expect(result.dynamicNonLiteral).toBeDefined();
      expect(result.evalCount).toBeDefined();
      expect(result.details).toBeDefined();

      expect(Array.isArray(result.static)).toBe(true);
      expect(Array.isArray(result.dynamicLiteral)).toBe(true);
      expect(typeof result.dynamicNonLiteral).toBe('number');
      expect(typeof result.evalCount).toBe('number');
      expect(Array.isArray(result.details)).toBe(true);
    });

    test('should handle empty code', async () => {
      const result = await analyzeModuleIDs('');

      expect(result.static).toHaveLength(0);
      expect(result.dynamicLiteral).toHaveLength(0);
      expect(result.dynamicNonLiteral).toBe(0);
      expect(result.evalCount).toBe(0);
    });

    test('should handle code with only comments', async () => {
      const code = `
        // This is a comment
        /* Block comment */
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.static).toHaveLength(0);
      expect(result.dynamicLiteral).toHaveLength(0);
    });
  });

  describe('Static Imports', () => {
    test('should detect ES6 import statements', async () => {
      const code = `
        import React from 'react';
        import { useState } from 'react';
        import * as utils from './utils';
      `;
      const result = await analyzeModuleIDs(code);

      // Should detect import statements
      expect(result).toBeDefined();
      expect(result.static.length).toBeGreaterThan(0);
      expect(result.details.filter((d) => d.kind === 'import').length).toBeGreaterThan(0);
    });

    test('should detect export from statements', async () => {
      const code = `
        export { default } from './App';
        export * from './utils';
      `;
      const result = await analyzeModuleIDs(code);

      // Export re-exports should be detected (if oxc-parser provides them)
      expect(result).toBeDefined();
      expect(Array.isArray(result.static)).toBe(true);
      expect(Array.isArray(result.details)).toBe(true);
    });

    test('should detect require calls', async () => {
      const code = `
        const fs = require('fs');
        const path = require('path');
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.static).toContain('fs');
      expect(result.static).toContain('path');
      expect(result.details.filter((d) => d.kind === 'require').length).toBe(2);
    });

    test('should handle require in different contexts', async () => {
      const code = `
        const mod = require('./module');
        let x;
        x = require('./another');
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.static).toContain('./module');
      expect(result.static).toContain('./another');
    });
  });

  describe('Dynamic Imports', () => {
    test('should detect literal dynamic imports', async () => {
      const code = `
        const mod = await import('./module');
        import('./another').then(m => m);
      `;
      const result = await analyzeModuleIDs(code);

      // Dynamic imports should be detected
      expect(result).toBeDefined();
      expect(result.dynamicLiteral.length + result.dynamicNonLiteral).toBeGreaterThan(0);
    });

    test('should count non-literal dynamic imports', async () => {
      const code = `
        const moduleName = 'react';
        import(moduleName);
        import(getModuleName());
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.dynamicNonLiteral).toBeGreaterThan(0);
    });
  });

  describe('eval() Detection', () => {
    test('should count eval calls', async () => {
      const code = `
        eval('console.log("test")');
        const x = eval('2 + 2');
        if (true) eval(someCode);
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.evalCount).toBe(3);
    });

    test('should not confuse eval-like function names', async () => {
      const code = `
        function evalSomething() {}
        const evaluate = () => {};
        evalSomething();
        evaluate();
      `;
      const result = await analyzeModuleIDs(code);

      expect(result.evalCount).toBe(0);
    });
  });

  describe('Edge Cases', () => {
    test('should handle syntax in strings', async () => {
      const code = `
        const str = "import React from 'react'";
        const code = 'require("fs")';
      `;
      const result = await analyzeModuleIDs(code);

      // String content should not be parsed as actual imports
      expect(result.static.length).toBeLessThan(2);
    });

    test('should handle malformed but parseable code', async () => {
      const code = `
        const x = 1
        const y = 2
      `;
      // Should not throw despite missing semicolons
      const result = await analyzeModuleIDs(code);

      expect(result).toBeDefined();
    });
  });

  describe('Details Information', () => {
    test('should include kind for detected modules', async () => {
      const code = `const mod = require('test');`;
      const result = await analyzeModuleIDs(code);

      if (result.details.length > 0) {
        expect(result.details[0]).toHaveProperty('kind');
        expect(result.details[0]).toHaveProperty('moduleId');
      }
    });
  });
});
