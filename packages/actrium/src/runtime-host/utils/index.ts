import React from 'react';
import type { TabConfig } from '../types';
import {
  evaluateContract,
  extractAppIdFromUrl as extractActroCoreAppIdFromUrl,
} from '../coreContract';
import { DevToolsBridge, BoxModelData } from '../libs/devTools';

/**
 * Evaluate if a tab can be gated based on contract support
 */
export function evaluateTabGate(tab: TabConfig): {
  status: 'ok' | 'compat' | 'blocked';
  blockedReason?: string | null;
} {
  return evaluateContract(tab.contract ?? null);
}

/**
 * Extract panels from React element tree by __panelId attribute
 */
export function extractPanelsByPanelId(element: React.ReactNode): {
  left: React.ReactElement | null;
  right: React.ReactElement | null;
} {
  if (!element || !React.isValidElement(element)) {
    return { left: null, right: null };
  }

  const result: {
    left: React.ReactElement | null;
    right: React.ReactElement | null;
  } = {
    left: null,
    right: null,
  };

  function traverse(node: React.ReactNode): void {
    if (!React.isValidElement(node)) return;

    const props = node.props as Record<string, unknown>;
    const panelId = props.__panelId;

    if (panelId === 'left') {
      result.left = props.children as React.ReactElement;
      return;
    }
    if (panelId === 'right') {
      result.right = props.children as React.ReactElement;
      return;
    }

    const children = props.children;
    if (Array.isArray(children)) {
      children.forEach(traverse);
    } else if (children) {
      traverse(children as React.ReactNode);
    }
  }

  traverse(element);
  return result;
}

/**
 * Extract margin/padding from style object for box model visualization
 */
export function extractBoxModelFromStyle(
  style: Record<string, unknown> | undefined,
  frame: { left: number; top: number; width: number; height: number }
): BoxModelData {
  // Parse numeric value (handles number or "10px" style strings)
  const parseNum = (val: unknown): number => {
    if (typeof val === 'number') return val;
    if (typeof val === 'string') return parseFloat(val) || 0;
    return 0;
  };

  // Extract margin
  const margin = {
    top: parseNum(
      style?.marginTop ?? style?.marginVertical ?? style?.margin ?? 0
    ),
    right: parseNum(
      style?.marginRight ?? style?.marginHorizontal ?? style?.margin ?? 0
    ),
    bottom: parseNum(
      style?.marginBottom ?? style?.marginVertical ?? style?.margin ?? 0
    ),
    left: parseNum(
      style?.marginLeft ?? style?.marginHorizontal ?? style?.margin ?? 0
    ),
  };

  // Extract padding
  const padding = {
    top: parseNum(
      style?.paddingTop ?? style?.paddingVertical ?? style?.padding ?? 0
    ),
    right: parseNum(
      style?.paddingRight ?? style?.paddingHorizontal ?? style?.padding ?? 0
    ),
    bottom: parseNum(
      style?.paddingBottom ?? style?.paddingVertical ?? style?.padding ?? 0
    ),
    left: parseNum(
      style?.paddingLeft ?? style?.paddingHorizontal ?? style?.padding ?? 0
    ),
  };

  // Extract border
  const border = {
    top: parseNum(style?.borderTopWidth ?? style?.borderWidth ?? 0),
    right: parseNum(style?.borderRightWidth ?? style?.borderWidth ?? 0),
    bottom: parseNum(style?.borderBottomWidth ?? style?.borderWidth ?? 0),
    left: parseNum(style?.borderLeftWidth ?? style?.borderWidth ?? 0),
  };

  // Extract computed styles
  const computedStyle: BoxModelData['style'] = {};

  if (style?.position) computedStyle.position = String(style.position);
  if (style?.backgroundColor)
    computedStyle.backgroundColor = String(style.backgroundColor);
  if (style?.top !== undefined) computedStyle.top = parseNum(style.top);
  if (style?.left !== undefined) computedStyle.left = parseNum(style.left);
  if (style?.right !== undefined) computedStyle.right = parseNum(style.right);
  if (style?.bottom !== undefined)
    computedStyle.bottom = parseNum(style.bottom);
  if (style?.display) computedStyle.display = String(style.display);
  if (style?.flexDirection)
    computedStyle.flexDirection = String(style.flexDirection);
  if (style?.alignItems) computedStyle.alignItems = String(style.alignItems);
  if (style?.justifyContent)
    computedStyle.justifyContent = String(style.justifyContent);
  if (style?.flex !== undefined) computedStyle.flex = parseNum(style.flex);
  if (style?.opacity !== undefined)
    computedStyle.opacity = parseNum(style.opacity);
  if (style?.overflow) computedStyle.overflow = String(style.overflow);

  return {
    frame: {
      x: frame.left,
      y: frame.top,
      width: frame.width,
      height: frame.height,
    },
    margin,
    padding,
    border,
    style: Object.keys(computedStyle).length > 0 ? computedStyle : undefined,
  };
}

/**
 * Extract appId from URL (e.g., https://.../with/{appId})
 * Ignores query parameters and hashes.
 */
export function extractAppIdFromUrl(url: string): string | null {
  return extractActroCoreAppIdFromUrl(url);
}

/**
 * ActrPackageLoader - .actr File Loader
 */
export { ActrPackageLoader } from './ActrPackageLoader';
export { DevActrPackageLoader, ENABLE_DEV_LOCAL_ACTR_SCRIPT } from './ActrPackageLoader.dev';
export type { ActrPackageManifest, ActrPackage, ActrPackageLoaderOptions } from './ActrPackageLoader.types';

// Create Default Examples
import { createActrPackageLoader } from './ActrPackageLoader.dev';
export const actrLoader = createActrPackageLoader({
  enableCache: false,
  cacheExpiry: 10 * 60 * 1000, // 10min
  timeout: 30000, // 30sec
});

/**
 * AudioStreamManager - Real-time voice streaming
 */
export { AudioStreamManager, createAudioStream } from './AudioStreamManager';
export type { AudioStreamOptions } from './AudioStreamManager';

/**
 * SpeechRecognitionService - Voice recognition tool
 */
export { SpeechRecognitionService, speechRecognitionService } from './SpeechRecognitionService';
export type {
  SpeechRecognitionCallbacks,
  SpeechRecognitionErrorEvent,
  SpeechRecognitionState,
  SpeechRecognitionTranscriptEvent,
  StartSpeechRecognitionOptions,
} from './SpeechRecognitionService/index.types';
