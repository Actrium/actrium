/**
 * Component Helpers for Guest Components
 *
 * Provides runtime validation and type-safe component creation
 */

import { logger } from './logger';

/**
 * Guest component descriptor
 */
export interface GuestComponent<T = unknown> {
  __component: string;
  props: T;
}

/**
 * Development mode detection
 */
const isDev = (globalThis as any).process?.env?.NODE_ENV === 'development';

/**
 * Validation rules for component props
 */
export interface ValidationRule {
  field: string;
  type?: 'string' | 'number' | 'boolean' | 'array' | 'object' | 'function';
  required?: boolean;
  validator?: (value: unknown) => boolean;
  message?: string;
}

/**
 * Validate a single field
 */
function validateField(
  componentName: string,
  props: Record<string, unknown>,
  rule: ValidationRule
): boolean {
  const { field, type, required, validator, message } = rule;
  const value = props[field];

  // Check required
  if (required && (value === undefined || value === null)) {
    logger.error('Component', `[${componentName}] Missing required prop "${field}"`, {
      componentName,
      field,
      receivedProps: Object.keys(props),
    });
    return false;
  }

  // Skip validation if not required and value is undefined/null
  if (!required && (value === undefined || value === null)) {
    return true;
  }

  // Type check
  if (type && value !== undefined && value !== null) {
    let valid = false;
    switch (type) {
      case 'string':
        valid = typeof value === 'string';
        break;
      case 'number':
        valid = typeof value === 'number';
        break;
      case 'boolean':
        valid = typeof value === 'boolean';
        break;
      case 'array':
        valid = Array.isArray(value);
        break;
      case 'object':
        valid = typeof value === 'object' && !Array.isArray(value);
        break;
      case 'function':
        valid = typeof value === 'function';
        break;
    }

    if (!valid) {
      logger.error(
        'Component',
        message || `[${componentName}] Invalid type for prop "${field}": expected ${type}`,
        {
          componentName,
          field,
          expectedType: type,
          receivedType: Array.isArray(value) ? 'array' : typeof value,
          receivedValue: value,
        }
      );
      return false;
    }
  }

  // Custom validator
  if (validator && value !== undefined && value !== null) {
    if (!validator(value)) {
      logger.error(
        'Component',
        message || `[${componentName}] Validation failed for prop "${field}"`,
        {
          componentName,
          field,
          receivedValue: value,
        }
      );
      return false;
    }
  }

  return true;
}

/**
 * Validate component props
 *
 * @param componentName - Name of the component
 * @param props - Props to validate
 * @param rules - Validation rules
 * @param throwOnError - Whether to throw on validation error (default: false in production, true in dev)
 * @returns true if valid, false otherwise
 */
export function validateProps(
  componentName: string,
  props: Record<string, unknown>,
  rules: ValidationRule[],
  throwOnError = isDev
): boolean {
  let allValid = true;

  for (const rule of rules) {
    if (!validateField(componentName, props, rule)) {
      allValid = false;
      if (throwOnError) {
        throw new Error(
          `[${componentName}] Invalid props: ${rule.field} ${
            rule.required ? 'is required' : 'failed validation'
          }`
        );
      }
    }
  }

  return allValid;
}

/**
 * Create a type-safe guest component factory
 *
 * @param componentName - Name of the component
 * @param rules - Optional validation rules
 * @returns Component factory function
 *
 * @example
 * ```typescript
 * const StepList = createGuestComponent<StepListProps>('StepList', [
 *   { field: 'items', type: 'array', required: true },
 *   { field: 'currentStep', type: 'number' },
 * ]);
 *
 * // Usage (type-safe):
 * const component = StepList({ items: [...], currentStep: 1 });
 * ```
 */
export function createGuestComponent<T extends object>(
  componentName: string,
  rules?: ValidationRule[]
): (props: T) => GuestComponent<T> {
  return (props: T): GuestComponent<T> => {
    // Validate props if rules provided
    if (rules && rules.length > 0) {
      validateProps(componentName, props as Record<string, unknown>, rules);
    }

    return {
      __component: componentName,
      props,
    };
  };
}

/**
 * Lightweight prop validation helpers
 */
export const validators = {
  /**
   * Check if value is a non-empty string
   */
  nonEmptyString: (value: unknown): boolean => typeof value === 'string' && value.length > 0,

  /**
   * Check if value is a non-empty array
   */
  nonEmptyArray: (value: unknown): boolean => Array.isArray(value) && value.length > 0,

  /**
   * Check if value is a positive number
   */
  positiveNumber: (value: unknown): boolean => typeof value === 'number' && value > 0,

  /**
   * Check if value is a non-negative number
   */
  nonNegativeNumber: (value: unknown): boolean => typeof value === 'number' && value >= 0,

  /**
   * Create an enum validator
   */
  enum:
    <T extends string | number>(allowedValues: readonly T[]) =>
    (value: unknown): boolean =>
      allowedValues.includes(value as T),
};
