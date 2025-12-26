/**
 * @rill/bridge - Protocol Layer
 *
 * Host ↔ Guest 共享的协议层
 * - 类型定义
 * - 序列化规则
 * - CallbackRegistry（跨边界函数引用管理）
 */

// CallbackRegistry 实现（跨边界函数引用管理）
export {
  CallbackRegistry as CallbackRegistryImpl,
  globalCallbackRegistry,
} from './CallbackRegistry';
// Serialization utilities (共享序列化工具)
export {
  createDecoder,
  createEncoder,
  decodeObject,
  encodeObject,
} from './serialization';
// TypeRules 系统
export {
  DEFAULT_TYPE_RULES,
  type TransportStrategy,
  type TypeRule,
  type TypeRuleContext,
} from './TypeRules';
// 类型定义
export type {
  AppendOperation,
  BaseOperation,
  // Bridge 完整类型
  BridgeValue,
  BridgeValueArray,
  BridgeValueObject,
  // 接口
  CallbackRegistry,
  CallFunctionMessage,
  ConfigUpdateMessage,
  CreateOperation,
  DeleteOperation,
  DestroyMessage,
  HostEventMessage,
  HostMessage,
  // 消息类型
  HostMessageType,
  InsertOperation,
  // JSI 类型
  JSIPrimitive,
  JSISafe,
  JSISafeArray,
  JSISafeObject,
  Operation,
  OperationBatch,
  // 操作类型
  OperationType,
  PromiseSettleMessage,
  RefCallOperation,
  RefMethodResultMessage,
  RemoveOperation,
  ReorderOperation,
  // 需要 Bridge 处理的类型
  RequiresBridge,
  SendToHost,
  SerializedCreateOperation,
  SerializedDate,
  SerializedError,
  SerializedFunction,
  SerializedHostMessage,
  SerializedMap,
  SerializedOperation,
  SerializedOperationBatch,
  SerializedPromise,
  SerializedPromiseSettleMessage,
  SerializedProps,
  SerializedRefCallOperation,
  SerializedRefMethodResultMessage,
  SerializedRegExp,
  SerializedSet,
  SerializedSpecialType,
  SerializedUpdateOperation,
  // 序列化类型
  SerializedValue,
  SerializedValueArray,
  SerializedValueObject,
  TextOperation,
  UpdateOperation,
} from './types';
// Type Guards
export {
  isJSIPrimitive,
  isSerializedDate,
  isSerializedError,
  isSerializedFunction,
  isSerializedMap,
  isSerializedPromise,
  isSerializedRegExp,
  isSerializedSet,
  isSerializedSpecialType,
  operationHasProps,
} from './types';
