import { NativeWorkerDispatch } from './native-worker-dispatch';
import type {
  MainToNativeWorkerMessage,
  NativeWorkerToMainMessage,
} from './native-worker-protocol';

interface NativeWorkerScope {
  onmessage: ((event: { data: MainToNativeWorkerMessage }) => void) | null;
  postMessage(message: NativeWorkerToMainMessage, transfer?: Transferable[]): void;
}

const scope = self as unknown as NativeWorkerScope;
const dispatch = new NativeWorkerDispatch({
  post: (message, transfer) => scope.postMessage(message, transfer),
});

scope.onmessage = (event) => dispatch.handle(event.data);
