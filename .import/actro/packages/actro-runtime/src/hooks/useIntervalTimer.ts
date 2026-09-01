import { useState, useRef, useCallback } from 'react';

/**
 * useIntervalTimer Hook
 * 用于实时记录并获取时间间隔
 */
export const useIntervalTimer = () => {
    const [duration, setDuration] = useState<number>(0);
    const [isRunning, setIsRunning] = useState<boolean>(false);
    
    const startTimeRef = useRef<number | null>(null);
    const intervalIdRef = useRef<NodeJS.Timeout | null>(null);

    /**
     * 开始计时
     */
    const start = useCallback(() => {
        if (isRunning) return;

        startTimeRef.current = Date.now();
        setIsRunning(true);
        setDuration(0);

        intervalIdRef.current = setInterval(() => {
            if (startTimeRef.current) {
                setDuration(Date.now() - startTimeRef.current);
            }
        }, 100); // 100ms 刷新一次，保证相对实时
    }, [isRunning]);

    /**
     * 结束计时
     * @returns 最终持续时间（毫秒）
     */
    const end = useCallback((): number => {
        if (intervalIdRef.current) {
            clearInterval(intervalIdRef.current);
            intervalIdRef.current = null;
        }

        const finalDuration = startTimeRef.current ? Date.now() - startTimeRef.current : 0;
        
        setDuration(finalDuration);
        setIsRunning(false);
        startTimeRef.current = null;

        return finalDuration;
    }, []);

    /**
     * 重置计时器
     */
    const reset = useCallback(() => {
        if (intervalIdRef.current) {
            clearInterval(intervalIdRef.current);
            intervalIdRef.current = null;
        }
        startTimeRef.current = null;
        setDuration(0);
        setIsRunning(false);
    }, []);

    return {
        duration,     // 当前已流逝的时间（毫秒）
        isRunning,    // 是否正在计时
        start,        // 开始方法
        end,          // 结束方法
        reset         // 重置方法
    };
};
