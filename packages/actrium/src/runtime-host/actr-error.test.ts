import { expect, test } from 'bun:test';
import { describeActrError } from './actr-error';

test('presents a Native package rejection with user-facing copy', () => {
  expect(describeActrError('PACKAGE_SIGNATURE_INVALID')).toEqual({
    title: 'Actrium blocked this application',
    message: 'The application package could not be verified.',
    code: 'PACKAGE_SIGNATURE_INVALID',
  });
});

test('presents an unsupported Guest component as a compatibility error', () => {
  expect(describeActrError('component_not_allowed')).toEqual({
    title: 'This application is incompatible',
    message: 'This version of Actrium does not support a feature the application needs.',
    code: 'component_not_allowed',
  });
});

test('uses generic copy for unknown or malformed error codes', () => {
  expect(describeActrError('FUTURE_CODE')).toEqual({
    title: 'Could not open the application',
    message: 'An unexpected error occurred while opening this application.',
    code: 'FUTURE_CODE',
  });
  expect(describeActrError('a raw exception\nwith a stack').code).toBe('ACTR_OPEN_FAILED');
  expect(describeActrError('constructor').title).toBe('Could not open the application');
});
