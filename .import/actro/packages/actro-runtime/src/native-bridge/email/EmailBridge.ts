import { NativeModules, Platform } from 'react-native';
import { EMAIL_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

type NativeEmailModule = {
  [EMAIL_BRIDGE_CONTRACT.methods.composeEmail]?: (
    to: string[],
    subject: string,
  ) => Promise<void> | void;
};

const EmailModule = NativeModules[
  EMAIL_BRIDGE_CONTRACT.moduleName
] as NativeEmailModule | undefined;

interface ComposeEmailParams {
  to: string[];
  subject?: string;
}

export const EmailBridge = {
  // only by android
  composeEmail: async ({ to, subject }: ComposeEmailParams): Promise<void> => {
    if (Platform.OS !== 'android') {
      throw new Error('EmailBridge.composeEmail is only supported on Android');
    }
    const composeEmailMethod = EMAIL_BRIDGE_CONTRACT.methods.composeEmail;
    const composeEmail = EmailModule?.[composeEmailMethod];
    if (!composeEmail) {
      throw new Error(
        `${EMAIL_BRIDGE_CONTRACT.moduleName}.${composeEmailMethod} is not available`,
      );
    }

    await composeEmail(to, subject ?? '');
  },
};
