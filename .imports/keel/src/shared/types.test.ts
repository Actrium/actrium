/**
 * @rill/bridge - Type Guards Tests
 *
 * Tests for runtime type checking functions used in serialization/deserialization
 */

import { describe, expect, test } from 'bun:test';
import {
  isJSIPrimitive,
  isSerializedDate,
  isSerializedError,
  isSerializedFunction,
  isSerializedMap,
  isSerializedPromise,
  isSerializedRegExp,
  isSerializedSet,
  isSerializedSpecialType,
  type Operation,
  operationHasProps,
  type SerializedOperation,
  type SerializedValue,
} from './types';

describe('Type Guards', () => {
  describe('isJSIPrimitive()', () => {
    test('should identify null', () => {
      expect(isJSIPrimitive(null)).toBe(true);
    });

    test('should identify undefined', () => {
      expect(isJSIPrimitive(undefined)).toBe(true);
    });

    test('should identify boolean', () => {
      expect(isJSIPrimitive(true)).toBe(true);
      expect(isJSIPrimitive(false)).toBe(true);
    });

    test('should identify number', () => {
      expect(isJSIPrimitive(0)).toBe(true);
      expect(isJSIPrimitive(42)).toBe(true);
      expect(isJSIPrimitive(-3.14)).toBe(true);
      expect(isJSIPrimitive(Number.NaN)).toBe(true);
      expect(isJSIPrimitive(Number.POSITIVE_INFINITY)).toBe(true);
    });

    test('should identify string', () => {
      expect(isJSIPrimitive('')).toBe(true);
      expect(isJSIPrimitive('hello')).toBe(true);
      expect(isJSIPrimitive('123')).toBe(true);
    });

    test('should reject objects', () => {
      expect(isJSIPrimitive({})).toBe(false);
      expect(isJSIPrimitive({ key: 'value' })).toBe(false);
    });

    test('should reject arrays', () => {
      expect(isJSIPrimitive([])).toBe(false);
      expect(isJSIPrimitive([1, 2, 3])).toBe(false);
    });

    test('should reject functions', () => {
      expect(isJSIPrimitive(() => {})).toBe(false);
    });
  });

  describe('isSerializedFunction()', () => {
    test('should identify serialized function', () => {
      const value: SerializedValue = { __type: 'function', __fnId: 'fn_123' };
      expect(isSerializedFunction(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedFunction(null)).toBe(false);
      expect(isSerializedFunction(undefined)).toBe(false);
      expect(isSerializedFunction(123)).toBe(false);
      expect(isSerializedFunction('string')).toBe(false);
    });

    test('should reject objects without __type', () => {
      expect(isSerializedFunction({ __fnId: 'fn_123' })).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedFunction({ __type: 'date', __fnId: 'fn_123' })).toBe(false);
    });

    test('should reject objects without __fnId', () => {
      expect(isSerializedFunction({ __type: 'function' })).toBe(false);
    });
  });

  describe('isSerializedDate()', () => {
    test('should identify serialized date', () => {
      const value: SerializedValue = { __type: 'date', __value: '2024-01-01T00:00:00.000Z' };
      expect(isSerializedDate(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedDate(null)).toBe(false);
      expect(isSerializedDate(123)).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedDate({ __type: 'function', __value: '2024-01-01' })).toBe(false);
    });

    test('should accept date even without __value (permissive check)', () => {
      // Type guard only checks __type field, not all required fields
      expect(isSerializedDate({ __type: 'date' })).toBe(true);
    });
  });

  describe('isSerializedRegExp()', () => {
    test('should identify serialized regexp', () => {
      const value: SerializedValue = { __type: 'regexp', __source: 'test', __flags: 'gi' };
      expect(isSerializedRegExp(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedRegExp(null)).toBe(false);
      expect(isSerializedRegExp(/test/g)).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedRegExp({ __type: 'date', __source: 'test' })).toBe(false);
    });

    test('should accept regexp even without all fields', () => {
      expect(isSerializedRegExp({ __type: 'regexp' })).toBe(true);
    });
  });

  describe('isSerializedError()', () => {
    test('should identify serialized error', () => {
      const value: SerializedValue = {
        __type: 'error',
        __name: 'TypeError',
        __message: 'test error',
        __stack: 'stack trace',
      };
      expect(isSerializedError(value)).toBe(true);
    });

    test('should identify serialized error without stack', () => {
      const value: SerializedValue = {
        __type: 'error',
        __name: 'Error',
        __message: 'message',
      };
      expect(isSerializedError(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedError(null)).toBe(false);
      expect(isSerializedError(new Error('test'))).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedError({ __type: 'function', __message: 'err' })).toBe(false);
    });
  });

  describe('isSerializedMap()', () => {
    test('should identify serialized map', () => {
      const value: SerializedValue = {
        __type: 'map',
        __entries: [
          ['key1', 'value1'],
          ['key2', 123],
        ],
      };
      expect(isSerializedMap(value)).toBe(true);
    });

    test('should identify empty serialized map', () => {
      const value: SerializedValue = { __type: 'map', __entries: [] };
      expect(isSerializedMap(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedMap(null)).toBe(false);
      expect(isSerializedMap(new Map())).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedMap({ __type: 'set', __entries: [] })).toBe(false);
    });

    test('should accept map even without __entries', () => {
      expect(isSerializedMap({ __type: 'map' })).toBe(true);
    });
  });

  describe('isSerializedSet()', () => {
    test('should identify serialized set', () => {
      const value: SerializedValue = { __type: 'set', __values: [1, 2, 3] };
      expect(isSerializedSet(value)).toBe(true);
    });

    test('should identify empty serialized set', () => {
      const value: SerializedValue = { __type: 'set', __values: [] };
      expect(isSerializedSet(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedSet(null)).toBe(false);
      expect(isSerializedSet(new Set())).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedSet({ __type: 'map', __values: [] })).toBe(false);
    });

    test('should accept set even without __values', () => {
      expect(isSerializedSet({ __type: 'set' })).toBe(true);
    });
  });

  describe('isSerializedPromise()', () => {
    test('should identify serialized promise', () => {
      const value: SerializedValue = { __type: 'promise', __promiseId: 'p_123' };
      expect(isSerializedPromise(value)).toBe(true);
    });

    test('should reject non-objects', () => {
      expect(isSerializedPromise(null)).toBe(false);
      expect(isSerializedPromise(Promise.resolve())).toBe(false);
    });

    test('should reject objects with wrong __type', () => {
      expect(isSerializedPromise({ __type: 'function', __promiseId: 'p_1' })).toBe(false);
    });

    test('should reject objects without __promiseId', () => {
      expect(isSerializedPromise({ __type: 'promise' })).toBe(false);
    });
  });

  describe('isSerializedSpecialType()', () => {
    test('should identify all serialized types', () => {
      expect(isSerializedSpecialType({ __type: 'function', __fnId: 'fn_1' })).toBe(true);
      expect(isSerializedSpecialType({ __type: 'date', __value: '2024-01-01' })).toBe(true);
      expect(isSerializedSpecialType({ __type: 'regexp', __source: 'test', __flags: 'g' })).toBe(
        true
      );
      expect(isSerializedSpecialType({ __type: 'error', __name: 'Error', __message: 'test' })).toBe(
        true
      );
      expect(isSerializedSpecialType({ __type: 'map', __entries: [] })).toBe(true);
      expect(isSerializedSpecialType({ __type: 'set', __values: [] })).toBe(true);
      expect(isSerializedSpecialType({ __type: 'promise', __promiseId: 'p_1' })).toBe(true);
    });

    test('should reject primitives', () => {
      expect(isSerializedSpecialType(null)).toBe(false);
      expect(isSerializedSpecialType(undefined)).toBe(false);
      expect(isSerializedSpecialType(123)).toBe(false);
      expect(isSerializedSpecialType('string')).toBe(false);
      expect(isSerializedSpecialType(true)).toBe(false);
    });

    test('should reject plain objects', () => {
      expect(isSerializedSpecialType({})).toBe(false);
      expect(isSerializedSpecialType({ key: 'value' })).toBe(false);
    });

    test('should reject arrays', () => {
      expect(isSerializedSpecialType([])).toBe(false);
      expect(isSerializedSpecialType([1, 2, 3])).toBe(false);
    });

    test('should reject objects with unknown __type', () => {
      expect(isSerializedSpecialType({ __type: 'unknown' })).toBe(false);
    });
  });

  describe('operationHasProps()', () => {
    test('should identify CREATE operation', () => {
      const op: Operation = { op: 'CREATE', id: 1, type: 'View', props: {} };
      expect(operationHasProps(op)).toBe(true);
    });

    test('should identify UPDATE operation', () => {
      const op: Operation = { op: 'UPDATE', id: 1, props: {} };
      expect(operationHasProps(op)).toBe(true);
    });

    test('should identify serialized CREATE operation', () => {
      const op: SerializedOperation = { op: 'CREATE', id: 1, type: 'View', props: {} };
      expect(operationHasProps(op)).toBe(true);
    });

    test('should identify serialized UPDATE operation', () => {
      const op: SerializedOperation = { op: 'UPDATE', id: 1, props: {} };
      expect(operationHasProps(op)).toBe(true);
    });

    test('should reject DELETE operation', () => {
      const op: Operation = { op: 'DELETE', id: 1 };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject APPEND operation', () => {
      const op: Operation = { op: 'APPEND', id: 1, parentId: 0, childId: 2 };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject INSERT operation', () => {
      const op: Operation = { op: 'INSERT', id: 1, parentId: 0, childId: 2, index: 0 };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject REMOVE operation', () => {
      const op: Operation = { op: 'REMOVE', id: 1, parentId: 0, childId: 2 };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject REORDER operation', () => {
      const op: Operation = { op: 'REORDER', id: 1, parentId: 0, childIds: [2, 3] };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject TEXT operation', () => {
      const op: Operation = { op: 'TEXT', id: 1, text: 'hello' };
      expect(operationHasProps(op)).toBe(false);
    });

    test('should reject REF_CALL operation', () => {
      const op: Operation = {
        op: 'REF_CALL',
        id: 1,
        refId: 2,
        method: 'focus',
        args: [],
        callId: 'call_1',
      };
      expect(operationHasProps(op)).toBe(false);
    });
  });
});
