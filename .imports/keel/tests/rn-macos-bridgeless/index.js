/**
 * @format
 */

import {AppRegistry} from 'react-native';
import App from './App';

// Keel sandbox JSI bindings are auto-installed via KeelSandboxNative's
// constructor swizzle on [RCTHost start]. No manual TurboModule loading needed.

AppRegistry.registerComponent('KeelMacOSTest', () => App);
