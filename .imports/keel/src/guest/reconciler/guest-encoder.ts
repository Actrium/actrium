/**
 * Guest-side Encoder
 *
 * Shared encoder for serializing props using Bridge TypeRules.
 * Used by both host-config.ts and element-transform.ts.
 */

import {
  createEncoder,
  DEFAULT_TYPE_RULES,
  encodeObject,
  globalCallbackRegistry,
  type SerializedValueObject,
  type TypeRuleContext,
} from '../../shared';

/**
 * Create Guest-side TypeRuleContext using globalCallbackRegistry
 */
function createGuestEncodeContext(): TypeRuleContext {
  // Reason: Context must reference encoder which references context (circular)
  const context = {} as TypeRuleContext;

  // Use shared encoder from Bridge
  const encoder = createEncoder(DEFAULT_TYPE_RULES, context);

  // Complete context initialization
  context.encode = encoder;
  context.decode = (v) => v; // Not used in Guest encoding
  context.registerFunction = (fn) =>
    globalCallbackRegistry.register(fn as (...args: unknown[]) => unknown);
  context.invokeFunction = (fnId, args) => globalCallbackRegistry.invoke(fnId, args);

  return context;
}

/**
 * Shared Guest encoder (created once, reused)
 */
const guestContext = createGuestEncodeContext();

/**
 * Guest encoder function
 */
export const guestEncoder = guestContext.encode;

/**
 * Serialize object props using shared Bridge utilities
 * Functions automatically become { __type: 'function', __fnId }
 */
export function serializeProps(props: Record<string, unknown>): SerializedValueObject {
  return encodeObject(props, guestEncoder) as SerializedValueObject;
}
