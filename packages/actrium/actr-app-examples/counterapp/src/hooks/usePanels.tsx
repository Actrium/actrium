import { useMemo } from 'react';
import Stats from '../pages/Stats';
import { Text } from 'keel/guest';

export function usePanels() {
  const left = useMemo(() => <Text>123</Text>, []);
  const right = useMemo(() => <Stats />, []);

  return { left, right };
}
