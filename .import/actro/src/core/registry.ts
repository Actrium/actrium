/**
 * AskIt Registry - Component and Module Registration
 *
 * This module provides the registry that connects askit components
 * to the Rill engine. Host apps import this to inject native implementations.
 */

import { ChatBubble } from '../ui/ChatBubble/ChatBubble.host';
// Import host component implementations
import { PanelMarker } from '../ui/Panel/PanelMarker.host';
import { StepList } from '../ui/StepList/StepList.host';
import { ThemeView } from '../ui/ThemeView/ThemeView.host';
import { UserAvatar } from '../ui/UserAvatar/UserAvatar.host';
import {
  clearHapticHandler,
  clearToastHandler,
  configureHaptic,
  configureToast,
  modules,
} from './registry.modules';

/**
 * Component registry for Rill engine
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { components } from 'askit/core';
 *
 * const engine = new Engine();
 * engine.register(components);
 * ```
 */
export const components = {
  PanelMarker,
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble,
} as const;

export { clearHapticHandler, clearToastHandler, configureHaptic, configureToast, modules };
