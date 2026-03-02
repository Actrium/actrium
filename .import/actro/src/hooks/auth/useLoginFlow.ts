import { useState, useEffect, useCallback, useRef } from 'react';
import { useLogin, LoginStep } from './useLogin';
import { useAuthorize, AuthorizeStep } from './useAuthorize';
import { useTab } from '../../actr-host/contexts/TabContext';

export type LoginFlowMode =
  | 'login_only'           // 设置页：仅登录
  | 'login_and_authorize'  // 输入框（未登录）：登录后自动授权
  | 'authorize_only';      // 输入框（已登录未授权）：仅授权

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
  /** 自动初始化检查登录状态，默认 true */
  autoInit?: boolean;
}

interface UseLoginFlowResult {
  step: LoginFlowStep;
  loading: boolean;
  /** 手动触发授权（authorize_only 模式使用） */
  authorize: () => Promise<boolean>;
  logout: () => void;
}

/**
 * 登录流程组合器 - 根据 mode 组合 useLogin 和 useAuthorize
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

  // 获取 Tab 上下文（可能不存在，如在 Modal 中使用）
  let currentTabId = '';
  try {
    const tabCtx = useTab();
    currentTabId = tabCtx?.tab?.id || '';
  } catch {
    // 不在 TabContext 中，忽略
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
    isModal: true, // Modal 模式下不执行 Tab 切换
    currentTabId,
  });

  // 映射 loginStep 到 combinedStep
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

  // 映射 authorizeStep 到 combinedStep
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

  // 根据 mode 和子 hook 状态更新 combinedStep
  useEffect(() => {
    if (mode === 'authorize_only') {
      // 仅授权模式：直接使用 authorizeStep
      setCombinedStep(mapAuthorizeStep(authorizeStep));
    } else if (mode === 'login_only') {
      // 仅登录模式：直接使用 loginStep
      setCombinedStep(mapLoginStep(loginStep));
    } else {
      // login_and_authorize 模式
      if (loginStep === 'logged_in') {
        // 登录完成，进入授权阶段
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

  // login_and_authorize 模式：登录成功后自动授权
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

  // 自动初始化（非 authorize_only 模式）
  useEffect(() => {
    if (autoInit && mode !== 'authorize_only' && !initializedRef.current) {
      initializedRef.current = true;
      checkLoginStatus();
    }
  }, [autoInit, mode, checkLoginStatus]);

  // 手动授权方法
  const authorize = useCallback(async (): Promise<boolean> => {
    return doAuthorize();
  }, [doAuthorize]);

  // 重置状态（用于弹窗关闭后重新打开）
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
