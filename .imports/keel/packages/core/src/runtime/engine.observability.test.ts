import { describe, it, expect } from 'bun:test';
import { Engine } from './engine';

class MockContext { eval(code:string){ return (0,eval)(code); } setGlobal(n:string,v:any){ (globalThis as any)[n]=v; } getGlobal(n:string){ return (globalThis as any)[n]; } dispose(){} }
class MockRuntime { createContext(){ return new MockContext(); } dispose(){} }
const MockQuickJSProvider = { createRuntime(){ return new MockRuntime(); } };

describe('Engine observability metrics', () => {
  it('should call onMetric with named metrics', async () => {
    const metrics: any[] = [];
    const engine = new Engine({ quickjs: MockQuickJSProvider as any, onMetric: (n,v,e)=>metrics.push({n,v,e}), debug: false });
    await engine.loadBundle('/* noop */');
    expect(metrics.find(m => m.n === 'engine.initializeRuntime')).toBeTruthy();
    expect(metrics.find(m => m.n === 'engine.executeBundle')).toBeTruthy();
    const health = engine.getHealth();
    expect('loaded' in health && 'receiverNodes' in health).toBe(true);
    engine.destroy();
  });
});
