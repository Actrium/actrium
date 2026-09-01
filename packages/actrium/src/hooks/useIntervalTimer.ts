import { useState, useRef, useCallback } from 'react';

/**
 * useIntervalTimer Hook
 * For real-time recording and acquisition of time interval
 */
export const useIntervalTimer = () => {
    const [duration, setDuration] = useState<number>(0);
    const [isRunning, setIsRunning] = useState<boolean>(false);
    
    const startTimeRef = useRef<number | null>(null);
    const intervalIdRef = useRef<NodeJS.Timeout | null>(null);

    /**
     * Start the timer.
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
        }, 100); // 100ms Refresh it once. Make sure it's real time.
    }, [isRunning]);

    /**
     * End time
     * @returns Final duration (ms)
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
     * Reset Timer
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
        duration,     // Current time passed (ms)
        isRunning,    // Whether or not the timing is ongoing
        start,        // Start method
        end,          // Method of termination
        reset         // Reset Method
    };
};
