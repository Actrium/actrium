import { useState, useEffect, useCallback, useRef } from 'react';
import { useLogin, LoginStep } from './useLogin';
import { useAuthorize, AuthorizeStep } from './useAuthorize';
import { useTab } from '../../actr-host/contexts/TabContext';

export type LoginFlowMode =
  | 'login_only'           // Set page: Login only
  | 'login_and_authorize'  // Input box (not login): Autoauthorization after login
  | 'authorize_only';      // Input box (not authorized for login): only authorized

export type LoginFlowStep =
  | 'idle'
  | 'checking'
  | 'login_options'
  | 'logging_in'
  | 'authorizing'
  | 'success';

interface UseLoginFlowOptions {
  mode: LoginFlowMode;
  clientId: string;
  appId?: string;
  /** Automatic Initializing check login status, default true */
  autoInit?: boolean;
}

interface UseLoginFlowResult {
  step: LoginFlowStep;
  loading: boolean;
  /** Manually trigger authorization (%1)authorize_only Mode) */
  authorize: () => Promise<boolean>;
  logout: () => void;
}

/**
 * Login Process Grouper - Based on mode Group useLogin and useAuthorize
 */
export const useLoginFlow = ({
  mode,
  clientId,
  appId = '',
  autoInit = true,
}: UseLoginFlowOptions): UseLoginFlowResult => {
  const [combinedStep, setCombinedStep] = useState<LoginFlowStep>('idle');
  const hasAutoAuthorizedRef = useRef(false);
  const initializedRef = useRef(false);

  // Access Tab Context (may not exist, e.g. Modal Used)
  let currentTabId = '';
  try {
    const tabCtx = useTab();
    currentTabId = tabCtx?.tab?.id || '';
  } catch {
    // No, I'm not. TabContext Center, Ignore
  }

  const {
    step: loginStep,
    loading: loginLoading,
    logout,
    checkLoginStatus,
  } = useLogin();

  const {
    step: authorizeStep,
    loading: authorizeLoading,
    authorize: doAuthorize,
    reset: resetAuthorize,
  } = useAuthorize({
    clientId,
    appId,
    isModal: true, // Modal Not implemented under Mode Tab Switch
    currentTabId,
  });

  // Map loginStep Present. combinedStep
  const mapLoginStep = useCallback((step: LoginStep): LoginFlowStep => {
    switch (step) {
      case 'idle':
        return 'idle';
      case 'checking':
        return 'checking';
      case 'login_options':
        return 'login_options';
      case 'logging_in':
        return 'logging_in';
      case 'logged_in':
        return mode === 'login_only' ? 'success' : 'authorizing';
      default:
        return 'idle';
    }
  }, [mode]);

  // Map authorizeStep Present. combinedStep
  const mapAuthorizeStep = useCallback((step: AuthorizeStep): LoginFlowStep => {
    switch (step) {
      case 'idle':
        return 'idle';
      case 'authorizing':
        return 'authorizing';
      case 'authorized':
        return 'success';
      case 'failed':
        return 'login_options';
      default:
        return 'idle';
    }
  }, []);

  // Based on mode Kazuko. hook Status Update combinedStep
  useEffect(() => {
    if (mode === 'authorize_only') {
      // Only authorized mode: direct use authorizeStep
      setCombinedStep(mapAuthorizeStep(authorizeStep));
    } else if (mode === 'login_only') {
      // Login mode only: Direct use loginStep
      setCombinedStep(mapLoginStep(loginStep));
    } else {
      // login_and_authorize Mode
      if (loginStep === 'logged_in') {
        // Login complete, go to the authorized stage.
        if (authorizeStep === 'idle' && !hasAutoAuthorizedRef.current) {
          setCombinedStep('authorizing');
        } else {
          setCombinedStep(mapAuthorizeStep(authorizeStep));
        }
      } else {
        setCombinedStep(mapLoginStep(loginStep));
      }
    }
  }, [mode, loginStep, authorizeStep, mapLoginStep, mapAuthorizeStep]);

  // login_and_authorize Mode: Auto-authorization after login successful
  useEffect(() => {
    if (
      mode === 'login_and_authorize' &&
      loginStep === 'logged_in' &&
      authorizeStep === 'idle' &&
      !hasAutoAuthorizedRef.current
    ) {
      hasAutoAuthorizedRef.current = true;
      doAuthorize();
    }
  }, [mode, loginStep, authorizeStep, doAuthorize]);

  // AutoInitiation (N) authorize_only Mode)
  useEffect(() => {
    if (autoInit && mode !== 'authorize_only' && !initializedRef.current) {
      initializedRef.current = true;
      checkLoginStatus();
    }
  }, [autoInit, mode, checkLoginStatus]);

  // Manual Authorisation Method
  const authorize = useCallback(async (): Promise<boolean> => {
    return doAuthorize();
  }, [doAuthorize]);

  // Reset state (for reopening after window opening)
  const handleLogout = useCallback(() => {
    hasAutoAuthorizedRef.current = false;
    resetAuthorize();
    logout();
  }, [logout, resetAuthorize]);

  return {
    step: combinedStep,
    loading: loginLoading || authorizeLoading,
    authorize,
    logout: handleLogout,
  };
};
