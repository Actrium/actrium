/**
 * Pre-rename spellings of the native-guest host API (issue #58).
 *
 * These five names were the public surface of `rill/host` before the
 * native-guest line got a self-describing name, so they stay exported rather
 * than breaking downstream silently. "Wasm" was never the axis that separated
 * the two guest lines: on the web BOTH compile to wasm, and what actually
 * distinguishes them is interpreted-JS versus compiled-native. See
 * docs/reference/guest-lines.md.
 *
 * They live in their own module so each `@deprecated` tag stays attached to the
 * alias it describes — re-exporting old and new spellings from one statement
 * would mark the new names deprecated too.
 */

import { NativeGuestHost, type NativeGuestHostOptions } from './native-guest-host';
import {
  createNativeGuestEngine,
  NativeGuestView,
  type NativeGuestViewOptions,
} from './native-guest-view';

/** @deprecated Renamed to {@link NativeGuestHost}. */
export const WasmGuestHost = NativeGuestHost;

/** @deprecated Renamed to `NativeGuestHostOptions`. */
export type WasmGuestHostOptions = NativeGuestHostOptions;

/** @deprecated Renamed to {@link NativeGuestView}. */
export const WasmGuestView = NativeGuestView;

/** @deprecated Renamed to `NativeGuestViewOptions`. */
export type WasmGuestViewOptions = NativeGuestViewOptions;

/** @deprecated Renamed to {@link createNativeGuestEngine}. */
export const createWasmGuestEngine = createNativeGuestEngine;
