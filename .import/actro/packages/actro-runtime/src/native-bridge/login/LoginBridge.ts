import { NativeEventEmitter, NativeModules } from 'react-native';
import {
  RUNTIME_LOGIN_RESULT_EVENT,
  RUNTIME_LOGIN_RESULT_KEYS,
  RUNTIME_LOGIN_TYPES,
  USER_INFO_BRIDGE_CONTRACT,
  type RuntimeLoginType,
} from '../../actr-host/coreContract';

type NativeUserInfoModule = {
  addListener: (eventName: string) => void;
  removeListeners: (count: number) => void;
  [USER_INFO_BRIDGE_CONTRACT.methods.loginWithType]?: (type: LoginType) => void;
};

const UserInfoModule = NativeModules[
  USER_INFO_BRIDGE_CONTRACT.moduleName
] as NativeUserInfoModule | undefined;
const eventEmitter = new NativeEventEmitter(UserInfoModule);

export type LoginType = RuntimeLoginType;

export type LoginResultEvent = Partial<{
  [RUNTIME_LOGIN_RESULT_KEYS.type]: LoginType;
  [RUNTIME_LOGIN_RESULT_KEYS.code]: string;
  [RUNTIME_LOGIN_RESULT_KEYS.idToken]: string;
  [RUNTIME_LOGIN_RESULT_KEYS.error]: boolean;
  [RUNTIME_LOGIN_RESULT_KEYS.message]: string;
}>;

export const LOGIN_TYPES = RUNTIME_LOGIN_TYPES;
export const LOGIN_RESULT_KEYS = RUNTIME_LOGIN_RESULT_KEYS;

type Unsubscribe = () => void;

export const LoginBridge = {
  loginWithType: (type: LoginType) => {
    UserInfoModule?.[USER_INFO_BRIDGE_CONTRACT.methods.loginWithType]?.(type);
  },

  onLoginResult: (callback: (event: LoginResultEvent) => void): Unsubscribe => {
    const subscription = eventEmitter.addListener(RUNTIME_LOGIN_RESULT_EVENT, callback);
    return () => subscription.remove();
  },
};
